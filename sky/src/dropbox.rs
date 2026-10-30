//! Dropbox via API v2. Bare lesetilgang (`files.metadata.read`, `files.content.read`):
//! CloudJumper kan ikke endre eller slette noe i Dropbox.
//!
//! Appen registreres gratis på <https://www.dropbox.com/developers/apps> («Scoped access», «Full Dropbox»),
//! uten hemmelighet. Innloggingen bruker PKCE. Dropbox krever at svaradressen er registrert på forhånd,
//! så vi lytter på faste porter (`PORTER`) som må stå i app-oppsettet.

use crate::{dropbox_hash, oauth};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const AUTH: &str = "https://www.dropbox.com/oauth2/authorize";
const TOKEN: &str = "https://api.dropboxapi.com/oauth2/token";
const API: &str = "https://api.dropboxapi.com/2";
const INNHOLD: &str = "https://content.dropboxapi.com/2";
const OMFANG: &str = "account_info.read files.metadata.read files.content.read";
/// Svaradressene `http://localhost:<port>/` må være lagt inn under «Redirect URIs» i Dropbox-appen.
pub const PORTER: &[u16] = &[53682, 53683, 53684];
const NOKKELRING: &str = "CloudJumper";
const NOKKEL: &str = "dropbox";
const FORSOK: u32 = 6;

const MEDIETYPER: &[&str] = &[
    "jpg", "jpeg", "png", "heic", "heif", "webp", "gif", "bmp", "tif", "tiff", "dng", "cr2", "nef", "arw", "mp4", "mov",
    "m4v", "3gp", "avi", "mkv", "mts",
];

/// App-nøkkelen fra Dropbox. Ikke hemmelig.
#[derive(Debug, Clone)]
pub struct Klient {
    pub app_key: String,
}

/// En fil i Dropbox.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub id: String,
    /// Sti fra roten av Dropbox, uten skråstrek først.
    pub sti: PathBuf,
    pub storrelse: u64,
    pub content_hash: Option<String>,
}

pub use crate::onedrive::Henting;

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(20)).timeout_read(Duration::from_secs(120)).build()
}

fn lagret_token() -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(NOKKELRING, NOKKEL)?)
}

pub fn er_innlogget() -> bool {
    lagret_token().and_then(|e| Ok(e.get_password()?)).is_ok()
}

/// Glemmer innloggingen på denne PC-en. Ingenting endres i Dropbox-kontoen.
pub fn logg_ut() -> Result<()> {
    match lagret_token()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn innloggingsadresse(klient: &Klient, tilbake: &str, pkce: &oauth::Pkce, tilstand: &str) -> String {
    let mut u = url::Url::parse(AUTH).unwrap();
    u.query_pairs_mut()
        .append_pair("client_id", &klient.app_key)
        .append_pair("redirect_uri", tilbake)
        .append_pair("response_type", "code")
        .append_pair("token_access_type", "offline")
        .append_pair("scope", OMFANG)
        .append_pair("code_challenge", &pkce.utfordring)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", tilstand);
    u.into()
}

/// Logger inn i nettleseren og lagrer fornyelsestokenet i Windows sin legitimasjonsbehandling.
/// Returnerer navnet på kontoen.
pub fn logg_inn(klient: &Klient, apne: &dyn Fn(&str) -> Result<()>) -> Result<String> {
    let (lytter, tilbake) = oauth::lytt_paa(PORTER)?;
    let pkce = oauth::Pkce::ny()?;
    let tilstand = oauth::tilfeldig_tilstand()?;
    apne(&innloggingsadresse(klient, &tilbake, &pkce, &tilstand))?;
    let kode = oauth::vent_paa_kode(lytter, &tilstand)?;
    let svar: Value = agent()
        .post(TOKEN)
        .send_form(&[
            ("client_id", klient.app_key.as_str()),
            ("code", kode.as_str()),
            ("code_verifier", pkce.verifikator.as_str()),
            ("redirect_uri", tilbake.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .map_err(http_feil)?
        .into_json()?;
    let fornyelse = svar["refresh_token"].as_str().context("Dropbox sendte ikke et fornyelsestoken")?;
    lagret_token()?.set_password(fornyelse).context("kunne ikke lagre innloggingen på denne PC-en")?;
    let tilgang = svar["access_token"].as_str().context("Dropbox sendte ikke et tilgangstoken")?;
    let konto = med_venting(|| agent().post(&format!("{API}/users/get_current_account")).set("Authorization", &format!("Bearer {tilgang}")).call())?
        .into_json::<Value>()?;
    Ok(konto["name"]["display_name"].as_str().unwrap_or("Dropbox").to_string())
}

/// Dropbox sitt fornyelsestoken varer til brukeren logger ut, og byttes ikke.
fn tilgangstoken(klient: &Klient) -> Result<String> {
    let fornyelse = lagret_token()?.get_password().context("Du er ikke logget inn på Dropbox på denne PC-en")?;
    let svar = agent().post(TOKEN).send_form(&[
        ("client_id", klient.app_key.as_str()),
        ("refresh_token", fornyelse.as_str()),
        ("grant_type", "refresh_token"),
    ]);
    match svar {
        Ok(s) => Ok(s.into_json::<Value>()?["access_token"].as_str().context("Dropbox sendte ikke et tilgangstoken")?.to_string()),
        Err(ureq::Error::Status(400 | 401, _)) => {
            let _ = logg_ut();
            bail!("Innloggingen på Dropbox er utløpt. Logg inn på nytt")
        }
        Err(e) => Err(http_feil(e)),
    }
}

/// Venter så lenge Dropbox ber om ved 429/503, og prøver igjen ved nettbrudd.
fn med_venting<T>(mut kall: impl FnMut() -> std::result::Result<T, ureq::Error>) -> Result<T> {
    let mut forsok = 0;
    loop {
        match kall() {
            Ok(t) => return Ok(t),
            Err(ureq::Error::Status(429 | 500 | 503 | 504, svar)) if forsok < FORSOK => {
                let sek = svar.header("Retry-After").and_then(|s| s.parse().ok()).unwrap_or(10u64 << forsok);
                std::thread::sleep(Duration::from_secs(sek.min(300)));
            }
            Err(ureq::Error::Transport(_)) if forsok < FORSOK => std::thread::sleep(Duration::from_secs(5 << forsok)),
            Err(e) => return Err(http_feil(e)),
        }
        forsok += 1;
    }
}

fn post_json(tilgang: &str, adresse: &str, kropp: &Value) -> Result<Value> {
    let svar =
        med_venting(|| agent().post(adresse).set("Authorization", &format!("Bearer {tilgang}")).send_json(kropp.clone()))?;
    Ok(svar.into_json()?)
}

pub fn er_media(navn: &str) -> bool {
    navn.rsplit_once('.').is_some_and(|(_, e)| MEDIETYPER.contains(&e.to_lowercase().as_str()))
}

/// Plukker ut bilder og videoer fra en side med `list_folder`-svar.
pub fn tolk_side(side: &Value) -> Vec<Element> {
    side["entries"]
        .as_array()
        .map(|v| v.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|e| e[".tag"] == "file" && er_media(e["name"].as_str().unwrap_or_default()))
        .map(|e| Element {
            id: e["id"].as_str().unwrap_or_default().to_string(),
            sti: PathBuf::from(e["path_display"].as_str().unwrap_or_default().trim_start_matches('/')),
            storrelse: e["size"].as_u64().unwrap_or(0),
            content_hash: e["content_hash"].as_str().map(str::to_string),
        })
        .collect()
}

/// Lister alle bilder og videoer i Dropbox.
pub fn liste(klient: &Klient) -> Result<Vec<Element>> {
    let tilgang = tilgangstoken(klient)?;
    let mut side = post_json(&tilgang, &format!("{API}/files/list_folder"), &json!({"path": "", "recursive": true, "limit": 2000}))?;
    let mut alle = tolk_side(&side);
    while side["has_more"].as_bool() == Some(true) {
        let markor = side["cursor"].as_str().context("Dropbox sendte ingen markør for neste side")?.to_string();
        side = post_json(&tilgang, &format!("{API}/files/list_folder/continue"), &json!({"cursor": markor}))?;
        alle.extend(tolk_side(&side));
    }
    Ok(alle)
}

/// Laster ned én fil. Fortsetter fra der en avbrutt nedlasting slapp. Filen godtas bare når størrelsen og
/// Dropbox sin sjekksum stemmer, så en fil som er satt sammen feil aldri havner i mappen som sorteres.
fn last_ned(tilgang: &str, e: &Element, til: &Path) -> Result<()> {
    let tmp = til.with_extension("cloudjumper-del");
    let arg = json!({ "path": e.id }).to_string();
    let mut sist = String::from("ukjent feil");
    for forsok in 0..FORSOK {
        if forsok > 0 {
            std::thread::sleep(Duration::from_secs(5 << forsok.min(5)));
        }
        let start = fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
        if start > 0 && start >= e.storrelse {
            if kontroller(e, &tmp)? {
                fs::rename(&tmp, til)?;
                return Ok(());
            }
            continue;
        }
        let mut r = agent()
            .post(&format!("{INNHOLD}/files/download"))
            .set("Authorization", &format!("Bearer {tilgang}"))
            .set("Dropbox-API-Arg", &arg);
        if start > 0 {
            r = r.set("Range", &format!("bytes={start}-"));
        }
        let svar = match r.call() {
            Ok(s) => s,
            Err(ureq::Error::Transport(t)) => {
                sist = t.to_string();
                continue;
            }
            Err(ureq::Error::Status(429 | 500 | 503 | 504, s)) => {
                sist = format!("Dropbox svarte {}", s.status());
                continue;
            }
            Err(err) => return Err(http_feil(err)),
        };
        let fortsetter = svar.status() == 206;
        let mut ut = fs::OpenOptions::new().create(true).write(true).append(fortsetter).truncate(!fortsetter).open(&tmp)?;
        let mut kilde = svar.into_reader();
        let mut buf = vec![0u8; 1 << 20];
        let lest_alt = loop {
            match kilde.read(&mut buf) {
                Ok(0) => break true,
                Ok(n) => ut.write_all(&buf[..n])?,
                Err(feil) => {
                    sist = feil.to_string();
                    break false;
                }
            }
        };
        ut.flush()?;
        drop(ut);
        let lengde = fs::metadata(&tmp)?.len();
        // Forbindelsen kan lukkes pent før filen er ferdig. Da fortsetter vi neste runde.
        if !lest_alt || lengde < e.storrelse {
            if lest_alt {
                sist = format!("fikk {lengde} av {} byte", e.storrelse);
            }
            continue;
        }
        if kontroller(e, &tmp)? {
            fs::rename(&tmp, til)?;
            return Ok(());
        }
        sist = "sjekksummen stemte ikke".into();
    }
    // Den halve filen blir liggende, så neste forsøk fortsetter der det stoppet.
    bail!("nedlastingen stoppet flere ganger ({sist})")
}

/// Stemmer størrelse og sjekksum? Hvis ikke, fjernes vår egen halvferdige fil så neste runde begynner på nytt.
fn kontroller(e: &Element, tmp: &Path) -> Result<bool> {
    let lengde = fs::metadata(tmp)?.len();
    let riktig = lengde == e.storrelse
        && match e.content_hash.as_deref() {
            Some(ventet) => dropbox_hash::for_fil(tmp)? == ventet,
            None => true,
        };
    if !riktig {
        let _ = fs::remove_file(tmp);
    }
    Ok(riktig)
}

/// Laster ned alle bilder og videoer til `mal`, med samme mappestruktur som i Dropbox.
/// Filer som allerede er hentet med riktig størrelse hoppes over.
pub fn hent(klient: &Klient, mal: &Path, fremdrift: &dyn Fn(usize, usize)) -> Result<Henting> {
    let elementer = liste(klient)?;
    let mut tilgang = tilgangstoken(klient)?;
    let mut fornyet = std::time::Instant::now();
    let mut h = Henting { mappe: mal.to_path_buf(), ..Default::default() };
    let total = elementer.len();
    for (n, e) in elementer.iter().enumerate() {
        fremdrift(n, total);
        let til = mal.join(crate::onedrive::trygg_sti(&e.sti));
        if fs::metadata(&til).is_ok_and(|m| m.len() == e.storrelse) {
            h.fantes += 1;
            continue;
        }
        // Tilgangstokenet varer omtrent fire timer. Fornyes i god tid.
        if fornyet.elapsed() > Duration::from_secs(3 * 60 * 60) {
            tilgang = tilgangstoken(klient)?;
            fornyet = std::time::Instant::now();
        }
        let res = til
            .parent()
            .map(fs::create_dir_all)
            .transpose()
            .map_err(anyhow::Error::from)
            .and_then(|_| last_ned(&tilgang, e, &til));
        match res {
            Ok(()) => h.hentet += 1,
            Err(err) => h.feil.push(format!("{}: {err:#}", e.sti.display())),
        }
    }
    fremdrift(total, total);
    Ok(h)
}

fn http_feil(e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::Status(kode, svar) => {
            let tekst = svar.into_string().unwrap_or_default();
            anyhow::anyhow!("Dropbox svarte med feil {kode}: {}", tekst.chars().take(300).collect::<String>())
        }
        ureq::Error::Transport(t) => anyhow::anyhow!("Fikk ikke kontakt med Dropbox. Sjekk nettet og prøv igjen ({t})"),
    }
}

#[cfg(test)]
mod tester {
    use super::*;

    #[test]
    fn innlogging_ber_bare_om_lesetilgang() {
        let pkce = oauth::Pkce::ny().unwrap();
        let a = innloggingsadresse(&Klient { app_key: "abc".into() }, "http://localhost:53682/", &pkce, "t");
        assert!(a.starts_with(AUTH));
        assert!(a.contains("token_access_type=offline"));
        assert!(a.contains("files.content.read"));
        assert!(!a.contains("write"));
        assert!(a.contains("redirect_uri=http%3A%2F%2Flocalhost%3A53682%2F"));
    }

    #[test]
    fn plukker_ut_bilder_og_videoer() {
        let side = json!({"entries": [
            {".tag": "folder", "name": "Kamera", "path_display": "/Kamera"},
            {".tag": "file", "name": "IMG_0001.JPG", "id": "id:a", "path_display": "/Kamera/IMG_0001.JPG", "size": 10, "content_hash": "h"},
            {".tag": "file", "name": "notat.txt", "id": "id:b", "path_display": "/notat.txt", "size": 3},
            {".tag": "deleted", "name": "gammel.jpg", "path_display": "/gammel.jpg"},
            {".tag": "file", "name": "film.mov", "id": "id:c", "path_display": "/film.mov", "size": 7},
        ], "has_more": false});
        let e = tolk_side(&side);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0], Element { id: "id:a".into(), sti: "Kamera/IMG_0001.JPG".into(), storrelse: 10, content_hash: Some("h".into()) });
        assert_eq!(e[1].sti, PathBuf::from("film.mov"));
    }

    #[test]
    fn faste_porter_svarer_paa_localhost() {
        let (_l, adresse) = oauth::lytt_paa(&[0]).unwrap();
        assert!(adresse.starts_with("http://localhost:"));
    }
}
