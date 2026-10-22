//! OneDrive (personlige Microsoft-kontoer) via Microsoft Graph. Bare lesetilgang (`Files.Read`):
//! CloudJumper kan ikke endre eller slette noe i OneDrive.
//!
//! Appen registreres i Azure som «Bare personlige Microsoft-kontoer», uten klienthemmelighet.
//! Innloggingen bruker PKCE og svar til localhost, slik Microsoft anbefaler for skrivebordsapper.

use crate::oauth;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const AUTH: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
const TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const GRAPH: &str = "https://graph.microsoft.com/v1.0";
const OMFANG: &str = "Files.Read offline_access";
const NOKKELRING: &str = "CloudJumper";
const NOKKEL: &str = "onedrive";
/// Hvor mange ganger vi prøver igjen når Microsoft ber oss vente (429/503) eller nettet faller ut.
const FORSOK: u32 = 6;

/// Programmets ID fra app-registreringen i Azure. Ikke hemmelig.
#[derive(Debug, Clone)]
pub struct Klient {
    pub client_id: String,
}

/// En fil i OneDrive.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub id: String,
    /// Sti fra roten av OneDrive, for eksempel `Bilder/Kamerarull/IMG_0001.jpg`.
    pub sti: PathBuf,
    pub storrelse: u64,
    /// `quickXorHash`, den eneste sjekksummen OneDrive lover for alle kontoer.
    pub quick_xor: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Henting {
    pub hentet: usize,
    pub fantes: usize,
    pub feil: Vec<String>,
    pub mappe: PathBuf,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(120))
        .build()
}

fn lagret_token() -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(NOKKELRING, NOKKEL)?)
}

pub fn er_innlogget() -> bool {
    lagret_token().and_then(|e| Ok(e.get_password()?)).is_ok()
}

/// Glemmer innloggingen på denne PC-en. Ingenting endres i Microsoft-kontoen.
pub fn logg_ut() -> Result<()> {
    match lagret_token()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn innloggingsadresse(klient: &Klient, tilbake: &str, pkce: &oauth::Pkce, tilstand: &str) -> String {
    let mut u = url::Url::parse(AUTH).unwrap();
    u.query_pairs_mut()
        .append_pair("client_id", &klient.client_id)
        .append_pair("redirect_uri", tilbake)
        .append_pair("response_type", "code")
        .append_pair("response_mode", "query")
        .append_pair("scope", OMFANG)
        .append_pair("code_challenge", &pkce.utfordring)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", tilstand)
        .append_pair("prompt", "select_account");
    u.into()
}

/// Microsoft godtar `http://localhost` med hvilken som helst port for skrivebordsapper.
fn localhost(tilbake: &str) -> String {
    tilbake.replace("127.0.0.1", "localhost")
}

/// Logger inn i nettleseren og lagrer fornyelsestokenet i Windows sin legitimasjonsbehandling.
/// Returnerer navnet på kontoen.
pub fn logg_inn(klient: &Klient, apne: &dyn Fn(&str) -> Result<()>) -> Result<String> {
    let (lytter, tilbake) = oauth::lytt()?;
    let tilbake = localhost(&tilbake);
    let pkce = oauth::Pkce::ny()?;
    let tilstand = oauth::tilfeldig_tilstand()?;
    apne(&innloggingsadresse(klient, &tilbake, &pkce, &tilstand))?;
    let kode = oauth::vent_paa_kode(lytter, &tilstand)?;
    let svar: Value = agent()
        .post(TOKEN)
        .send_form(&[
            ("client_id", klient.client_id.as_str()),
            ("code", kode.as_str()),
            ("code_verifier", pkce.verifikator.as_str()),
            ("redirect_uri", tilbake.as_str()),
            ("grant_type", "authorization_code"),
            ("scope", OMFANG),
        ])
        .map_err(http_feil)?
        .into_json()?;
    let tilgang = lagre(&svar)?;
    let disk = hent_json(&tilgang, &format!("{GRAPH}/me/drive"))?;
    Ok(disk["owner"]["user"]["displayName"].as_str().unwrap_or("OneDrive").to_string())
}

/// Lagrer det nye fornyelsestokenet (Microsoft bytter det hver gang) og gir tilgangstokenet.
fn lagre(svar: &Value) -> Result<String> {
    if let Some(f) = svar["refresh_token"].as_str() {
        lagret_token()?.set_password(f).context("kunne ikke lagre innloggingen på denne PC-en")?;
    }
    Ok(svar["access_token"].as_str().context("Microsoft sendte ikke et tilgangstoken")?.to_string())
}

fn tilgangstoken(klient: &Klient) -> Result<String> {
    let fornyelse = lagret_token()?.get_password().context("Du er ikke logget inn på OneDrive på denne PC-en")?;
    let svar = agent().post(TOKEN).send_form(&[
        ("client_id", klient.client_id.as_str()),
        ("refresh_token", fornyelse.as_str()),
        ("grant_type", "refresh_token"),
        ("scope", OMFANG),
    ]);
    match svar {
        Ok(s) => lagre(&s.into_json()?),
        Err(ureq::Error::Status(400 | 401, _)) => {
            let _ = logg_ut();
            bail!("Innloggingen på OneDrive er utløpt. Logg inn på nytt")
        }
        Err(e) => Err(http_feil(e)),
    }
}

/// Venter så lenge Microsoft ber om ved 429/503, og prøver igjen ved nettbrudd.
fn med_venting<T>(mut kall: impl FnMut() -> std::result::Result<T, ureq::Error>) -> Result<T> {
    let mut forsok = 0;
    loop {
        match kall() {
            Ok(t) => return Ok(t),
            Err(ureq::Error::Status(429 | 503 | 504, svar)) if forsok < FORSOK => {
                let sek = svar.header("Retry-After").and_then(|s| s.parse().ok()).unwrap_or(10u64 << forsok);
                std::thread::sleep(Duration::from_secs(sek.min(300)));
            }
            Err(ureq::Error::Transport(_)) if forsok < FORSOK => std::thread::sleep(Duration::from_secs(5 << forsok)),
            Err(e) => return Err(http_feil(e)),
        }
        forsok += 1;
    }
}

fn hent_json(tilgang: &str, adresse: &str) -> Result<Value> {
    let svar = med_venting(|| agent().get(adresse).set("Authorization", &format!("Bearer {tilgang}")).call())?;
    Ok(svar.into_json()?)
}

pub fn er_media(v: &Value) -> bool {
    if v.get("photo").is_some() || v.get("video").is_some() {
        return true;
    }
    let mime = v["file"]["mimeType"].as_str().unwrap_or_default();
    mime.starts_with("image/") || mime.starts_with("video/")
}

/// Bygger stier fra en delta-liste. Delta gir ikke hele stien, bare forelderens ID.
pub fn tolk_delta(elementer: &[Value]) -> Vec<Element> {
    let mut mapper: HashMap<&str, (&str, Option<&str>)> = HashMap::new();
    for v in elementer {
        if v.get("folder").is_some() || v.get("root").is_some() {
            let id = v["id"].as_str().unwrap_or_default();
            let navn = if v.get("root").is_some() { "" } else { v["name"].as_str().unwrap_or_default() };
            mapper.insert(id, (navn, v["parentReference"]["id"].as_str()));
        }
    }
    let sti_til = |start: Option<&str>| {
        let mut id = start.map(str::to_string);
        let mut deler = Vec::new();
        while let Some(i) = id.take() {
            let Some(&(navn, forelder)) = mapper.get(i.as_str()) else { break };
            if !navn.is_empty() {
                deler.push(navn);
            }
            id = forelder.map(str::to_string);
            if deler.len() > 64 {
                break;
            }
        }
        deler.iter().rev().collect::<PathBuf>()
    };
    elementer
        .iter()
        .filter(|v| v.get("file").is_some() && v.get("deleted").is_none() && er_media(v))
        .map(|v| Element {
            id: v["id"].as_str().unwrap_or_default().to_string(),
            sti: sti_til(v["parentReference"]["id"].as_str()).join(v["name"].as_str().unwrap_or("uten navn")),
            storrelse: v["size"].as_u64().unwrap_or(0),
            quick_xor: v["file"]["hashes"]["quickXorHash"].as_str().map(str::to_string),
        })
        .collect()
}

/// Lister alle bilder og videoer i OneDrive.
pub fn liste(klient: &Klient) -> Result<Vec<Element>> {
    let tilgang = tilgangstoken(klient)?;
    let mut adresse = format!("{GRAPH}/me/drive/root/delta?$select=id,name,size,file,folder,root,photo,video,parentReference,deleted");
    let mut alle = Vec::new();
    loop {
        let side = hent_json(&tilgang, &adresse)?;
        alle.extend(side["value"].as_array().cloned().unwrap_or_default());
        match side["@odata.nextLink"].as_str() {
            Some(neste) => adresse = neste.to_string(),
            None => break,
        }
    }
    Ok(tolk_delta(&alle))
}

/// Laster ned én fil. Fortsetter fra der en avbrutt nedlasting slapp.
fn last_ned(tilgang: &str, e: &Element, til: &Path) -> Result<()> {
    let tmp = til.with_extension("cloudjumper-del");
    for _ in 0..FORSOK {
        // Nedlastingslenken varer bare noen minutter, så den hentes rett før hver runde.
        let info = hent_json(tilgang, &format!("{GRAPH}/me/drive/items/{}?$select=@microsoft.graph.downloadUrl", e.id))?;
        let lenke = info["@microsoft.graph.downloadUrl"].as_str().context("OneDrive ga ingen nedlastingslenke")?;
        let start = fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
        if start >= e.storrelse && e.storrelse > 0 {
            fs::rename(&tmp, til)?;
            return Ok(());
        }
        let mut r = agent().get(lenke);
        if start > 0 {
            r = r.set("Range", &format!("bytes={start}-"));
        }
        let svar = match r.call() {
            Ok(s) => s,
            Err(ureq::Error::Status(416, _)) => {
                // Live Photos: OneDrive gir bare stillbildet, så størrelsen stemmer ikke. Begynn på nytt.
                let _ = fs::remove_file(&tmp);
                continue;
            }
            Err(ureq::Error::Transport(_)) => continue,
            Err(err) => return Err(http_feil(err)),
        };
        let fortsetter = svar.status() == 206;
        let mut ut = fs::OpenOptions::new().create(true).write(true).append(fortsetter).truncate(!fortsetter).open(&tmp)?;
        let mut kilde = svar.into_reader();
        let mut buf = vec![0u8; 1 << 20];
        let ok = loop {
            match kilde.read(&mut buf) {
                Ok(0) => break true,
                Ok(n) => ut.write_all(&buf[..n])?,
                Err(_) => break false,
            }
        };
        ut.flush()?;
        if ok {
            fs::rename(&tmp, til)?;
            return Ok(());
        }
    }
    // Den halve filen blir liggende, så neste forsøk fortsetter der det stoppet.
    bail!("nedlastingen stoppet flere ganger")
}

/// Laster ned alle bilder og videoer til `mal`, med samme mappestruktur som i OneDrive.
/// Filer som allerede er hentet med riktig størrelse hoppes over.
pub fn hent(klient: &Klient, mal: &Path, fremdrift: &dyn Fn(usize, usize)) -> Result<Henting> {
    let elementer = liste(klient)?;
    let mut tilgang = tilgangstoken(klient)?;
    let mut fornyet = std::time::Instant::now();
    let mut h = Henting { mappe: mal.to_path_buf(), ..Default::default() };
    let total = elementer.len();
    for (n, e) in elementer.iter().enumerate() {
        fremdrift(n, total);
        let til = mal.join(trygg_sti(&e.sti));
        if fs::metadata(&til).is_ok_and(|m| m.len() == e.storrelse) {
            h.fantes += 1;
            continue;
        }
        // Tilgangstokenet varer en time.
        if fornyet.elapsed() > Duration::from_secs(45 * 60) {
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

/// Gjør hver del av stien trygg på Windows (OneDrive tillater tegn som Windows ikke gjør).
pub fn trygg_sti(sti: &Path) -> PathBuf {
    sti.components()
        .map(|c| {
            let s: String = c
                .as_os_str()
                .to_string_lossy()
                .chars()
                .map(|t| if t.is_control() || r#"<>:"/\|?*"#.contains(t) { '_' } else { t })
                .collect();
            let s = s.trim_end_matches(['.', ' ']).to_string();
            if s.is_empty() || s == ".." {
                "_".to_string()
            } else {
                s
            }
        })
        .collect()
}

fn http_feil(e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::Status(kode, svar) => {
            let tekst = svar.into_string().unwrap_or_default();
            anyhow::anyhow!("Microsoft svarte med feil {kode}: {}", tekst.chars().take(300).collect::<String>())
        }
        ureq::Error::Transport(t) => anyhow::anyhow!("Fikk ikke kontakt med OneDrive. Sjekk nettet og prøv igjen ({t})"),
    }
}

#[cfg(test)]
mod tester {
    use super::*;
    use serde_json::json;

    #[test]
    fn innlogging_ber_bare_om_lesetilgang() {
        let pkce = oauth::Pkce::ny().unwrap();
        let a = innloggingsadresse(&Klient { client_id: "abc".into() }, "http://localhost:1234", &pkce, "t");
        assert!(a.starts_with(AUTH));
        assert!(a.contains("scope=Files.Read+offline_access"));
        assert!(!a.contains("ReadWrite"));
        assert_eq!(localhost("http://127.0.0.1:5555"), "http://localhost:5555");
    }

    #[test]
    fn bygger_stier_fra_delta() {
        let delta = vec![
            json!({"id": "R", "name": "root", "root": {}}),
            json!({"id": "B", "name": "Bilder", "folder": {}, "parentReference": {"id": "R"}}),
            json!({"id": "K", "name": "Kamerarull", "folder": {}, "parentReference": {"id": "B"}}),
            json!({"id": "1", "name": "IMG_0001.jpg", "size": 10, "file": {"mimeType": "image/jpeg", "hashes": {"quickXorHash": "q"}}, "photo": {}, "parentReference": {"id": "K"}}),
            json!({"id": "2", "name": "notat.docx", "size": 5, "file": {"mimeType": "application/msword"}, "parentReference": {"id": "B"}}),
            json!({"id": "3", "name": "VID.mp4", "size": 7, "file": {"mimeType": "video/mp4"}, "parentReference": {"id": "R"}}),
        ];
        let e = tolk_delta(&delta);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].sti, PathBuf::from("Bilder/Kamerarull/IMG_0001.jpg"));
        assert_eq!(e[0].quick_xor.as_deref(), Some("q"));
        assert_eq!(e[1].sti, PathBuf::from("VID.mp4"));
    }

    #[test]
    fn trygge_stier() {
        assert_eq!(trygg_sti(Path::new("Ferie: 2019/bilde?.jpg")), PathBuf::from("Ferie_ 2019/bilde_.jpg"));
    }
}
