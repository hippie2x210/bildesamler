//! Google: innlogging med lesetilgang til Gmail, og leting etter Takeout-e-posten med nedlastingslenkene.
//! Google har ingen offisiell måte å bestille eller laste ned Takeout fra et program. Appen åpner derfor
//! Takeout-siden og nedlastingslenkene i brukerens nettleser, der brukeren allerede er logget inn.

use crate::oauth;
use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

/// Takeout-siden med bare Google Foto valgt.
pub const TAKEOUT_FOTO: &str = "https://takeout.google.com/settings/takeout/custom/photos";
/// Takeout-siden med eksportene som er klare til nedlasting.
pub const TAKEOUT_NEDLASTINGER: &str = "https://takeout.google.com/settings/takeout/downloads";
const AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const GMAIL: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
const OMFANG: &str = "https://www.googleapis.com/auth/gmail.readonly";
const NOKKELRING: &str = "CloudJumper";
const NOKKEL: &str = "google";
/// Takeout-lenker utløper etter en uke, så eldre e-post er uinteressant.
const SOK: &str = "from:google.com takeout newer_than:14d";

/// OAuth-klienten fra Google Cloud-prosjektet (filen som lastes ned for en «Datamaskinapp»).
/// For skrivebordsapper regner ikke Google klienthemmeligheten som hemmelig.
#[derive(Debug, Clone, Deserialize)]
pub struct Klient {
    pub client_id: String,
    pub client_secret: String,
}

impl Klient {
    /// Leser JSON-filen slik Google laster den ned: `{"installed": {"client_id": ..., ...}}`.
    pub fn fra_json(tekst: &str) -> Result<Self> {
        let v: Value = serde_json::from_str(tekst).context("klientfilen er ikke gyldig JSON")?;
        let indre = v.get("installed").or_else(|| v.get("web")).unwrap_or(&v);
        serde_json::from_value(indre.clone())
            .context("klientfilen mangler client_id eller client_secret. Er det en klient av typen «Datamaskinapp»?")
    }
}

/// En Takeout-e-post med lenkene til zip-filene.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TakeoutPost {
    pub id: String,
    /// Når e-posten kom, i millisekunder siden 1970.
    pub mottatt_ms: i64,
    pub emne: String,
    pub lenker: Vec<String>,
}

fn agent() -> ureq::Agent {
    // Trege nett: god tid til å koble til og lese, men ikke uendelig.
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .build()
}

fn lagret_token() -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(NOKKELRING, NOKKEL)?)
}

/// Er brukeren logget inn på denne PC-en fra før?
pub fn er_innlogget() -> bool {
    lagret_token().and_then(|e| Ok(e.get_password()?)).is_ok()
}

/// Glemmer innloggingen på denne PC-en. Ingenting endres i Google-kontoen.
pub fn logg_ut() -> Result<()> {
    match lagret_token()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Lager adressen nettleseren skal åpne for å logge inn.
pub fn innloggingsadresse(klient: &Klient, tilbake: &str, pkce: &oauth::Pkce, tilstand: &str) -> String {
    let mut u = url::Url::parse(AUTH).unwrap();
    u.query_pairs_mut()
        .append_pair("client_id", &klient.client_id)
        .append_pair("redirect_uri", tilbake)
        .append_pair("response_type", "code")
        .append_pair("scope", OMFANG)
        .append_pair("code_challenge", &pkce.utfordring)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", tilstand)
        // Uten disse får vi ikke et fornyelsestoken, og brukeren må logge inn hver time.
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    u.into()
}

/// Logger inn: åpner nettleseren med `apne`, venter på svaret og lagrer fornyelsestokenet
/// i Windows sin legitimasjonsbehandling. Returnerer e-postadressen til kontoen.
pub fn logg_inn(klient: &Klient, apne: &dyn Fn(&str) -> Result<()>) -> Result<String> {
    let (lytter, tilbake) = oauth::lytt()?;
    let pkce = oauth::Pkce::ny()?;
    let tilstand = oauth::tilfeldig_tilstand()?;
    apne(&innloggingsadresse(klient, &tilbake, &pkce, &tilstand))?;
    let kode = oauth::vent_paa_kode(lytter, &tilstand)?;

    let svar: Value = agent()
        .post(TOKEN)
        .send_form(&[
            ("client_id", klient.client_id.as_str()),
            ("client_secret", klient.client_secret.as_str()),
            ("code", kode.as_str()),
            ("code_verifier", pkce.verifikator.as_str()),
            ("redirect_uri", tilbake.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .map_err(http_feil)?
        .into_json()?;
    let fornyelse = svar["refresh_token"].as_str().context("Google sendte ikke et fornyelsestoken")?;
    let tilgang = svar["access_token"].as_str().context("Google sendte ikke et tilgangstoken")?;
    lagret_token()?.set_password(fornyelse).context("kunne ikke lagre innloggingen på denne PC-en")?;
    epost(tilgang)
}

fn tilgangstoken(klient: &Klient) -> Result<String> {
    let fornyelse = lagret_token()?.get_password().context("Du er ikke logget inn hos Google på denne PC-en")?;
    let svar = agent().post(TOKEN).send_form(&[
        ("client_id", klient.client_id.as_str()),
        ("client_secret", klient.client_secret.as_str()),
        ("refresh_token", fornyelse.as_str()),
        ("grant_type", "refresh_token"),
    ]);
    match svar {
        Ok(s) => {
            let v: Value = s.into_json()?;
            Ok(v["access_token"].as_str().context("Google sendte ikke et tilgangstoken")?.to_string())
        }
        // Tilgangen er trukket tilbake eller utløpt. Da må brukeren logge inn på nytt.
        Err(ureq::Error::Status(400 | 401, _)) => {
            let _ = logg_ut();
            bail!("Innloggingen hos Google er utløpt. Logg inn på nytt")
        }
        Err(e) => Err(http_feil(e)),
    }
}

fn epost(tilgang: &str) -> Result<String> {
    let v: Value = agent()
        .get(&format!("{GMAIL}/profile"))
        .set("Authorization", &format!("Bearer {tilgang}"))
        .call()
        .map_err(http_feil)?
        .into_json()?;
    Ok(v["emailAddress"].as_str().unwrap_or_default().to_string())
}

/// Finner Takeout-e-poster fra de siste to ukene, nyeste først. Leser bare, endrer ingenting i Gmail.
pub fn finn_takeout(klient: &Klient) -> Result<Vec<TakeoutPost>> {
    let tilgang = tilgangstoken(klient)?;
    let hent = |sti: &str, sporring: &[(&str, &str)]| -> Result<Value> {
        let mut r = agent().get(&format!("{GMAIL}/{sti}")).set("Authorization", &format!("Bearer {tilgang}"));
        for (k, v) in sporring {
            r = r.query(k, v);
        }
        Ok(r.call().map_err(http_feil)?.into_json()?)
    };
    let liste = hent("messages", &[("q", SOK), ("maxResults", "20")])?;
    let mut poster = Vec::new();
    for m in liste["messages"].as_array().into_iter().flatten() {
        let Some(id) = m["id"].as_str() else { continue };
        let melding = hent(&format!("messages/{id}"), &[("format", "full")])?;
        if let Some(p) = tolk_melding(&melding) {
            poster.push(p);
        }
    }
    poster.sort_by_key(|p| std::cmp::Reverse(p.mottatt_ms));
    Ok(poster)
}

/// Henter emne, tidspunkt og Takeout-nedlastingslenker fra en Gmail-melding (format=full).
pub fn tolk_melding(melding: &Value) -> Option<TakeoutPost> {
    let mut tekst = String::new();
    samle_tekst(&melding["payload"], &mut tekst);
    let lenker = takeout_lenker(&tekst);
    if lenker.is_empty() {
        return None;
    }
    let emne = melding["payload"]["headers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|h| h["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case("subject")))
        .and_then(|h| h["value"].as_str())
        .unwrap_or_default()
        .to_string();
    Some(TakeoutPost {
        id: melding["id"].as_str().unwrap_or_default().to_string(),
        mottatt_ms: melding["internalDate"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0),
        emne,
        lenker,
    })
}

fn samle_tekst(del: &Value, ut: &mut String) {
    if let Some(data) = del["body"]["data"].as_str() {
        let bytes = URL_SAFE.decode(data).or_else(|_| URL_SAFE_NO_PAD.decode(data.trim_end_matches('=')));
        if let Ok(b) = bytes {
            ut.push_str(&String::from_utf8_lossy(&b));
            ut.push('\n');
        }
    }
    for d in del["parts"].as_array().into_iter().flatten() {
        samle_tekst(d, ut);
    }
}

/// Plukker ut lenker til takeout.google.com som laster ned filer, i den rekkefølgen de står.
pub fn takeout_lenker(tekst: &str) -> Vec<String> {
    let mut lenker: Vec<String> = Vec::new();
    for start in tekst.match_indices("https://takeout.google.com/").map(|(i, _)| i) {
        let slutt = tekst[start..]
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')'))
            .map_or(tekst.len(), |i| start + i);
        let lenke = tekst[start..slutt].replace("&amp;", "&");
        if lenke.contains("download") && !lenker.contains(&lenke) {
            lenker.push(lenke);
        }
    }
    lenker
}

fn http_feil(e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::Status(kode, svar) => {
            let tekst = svar.into_string().unwrap_or_default();
            anyhow::anyhow!("Google svarte med feil {kode}: {}", tekst.chars().take(300).collect::<String>())
        }
        ureq::Error::Transport(t) => anyhow::anyhow!("Fikk ikke kontakt med Google. Sjekk nettet og prøv igjen ({t})"),
    }
}

#[cfg(test)]
mod tester {
    use super::*;

    #[test]
    fn leser_klientfil_fra_google() {
        let k = Klient::fra_json(
            r#"{"installed":{"client_id":"123.apps.googleusercontent.com","project_id":"cloudjumper","client_secret":"GOCSPX-abc","redirect_uris":["http://localhost"]}}"#,
        )
        .unwrap();
        assert_eq!(k.client_id, "123.apps.googleusercontent.com");
        assert!(Klient::fra_json(r#"{"installed":{"client_id":"x"}}"#).is_err());
    }

    #[test]
    fn innloggingsadressen_ber_bare_om_lesetilgang() {
        let k = Klient { client_id: "id".into(), client_secret: "s".into() };
        let p = oauth::Pkce::ny().unwrap();
        let a = innloggingsadresse(&k, "http://127.0.0.1:5000", &p, "tt");
        assert!(a.contains("gmail.readonly"));
        assert!(a.contains("code_challenge_method=S256"));
        assert!(a.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A5000"));
    }

    #[test]
    fn finner_takeout_lenker_i_epost() {
        let html = r#"<p>Filene er klare.</p>
            <a href="https://takeout.google.com/takeout/download?j=abc&amp;i=0&amp;user=1">Del 1</a>
            <a href="https://takeout.google.com/takeout/download?j=abc&amp;i=1&amp;user=1">Del 2</a>
            <a href="https://takeout.google.com/takeout/download?j=abc&amp;i=0&amp;user=1">Del 1 igjen</a>
            <a href="https://takeout.google.com/settings/takeout">Ny eksport</a>
            <a href="https://support.google.com/accounts">Hjelp</a>"#;
        let data = URL_SAFE.encode(html);
        let melding = serde_json::json!({
            "id": "m1",
            "internalDate": "1792670000000",
            "payload": {
                "headers": [{"name": "Subject", "value": "Google-dataene dine er klare til nedlasting"}],
                "parts": [{"mimeType": "text/html", "body": {"data": data}}]
            }
        });
        let p = tolk_melding(&melding).unwrap();
        assert_eq!(p.emne, "Google-dataene dine er klare til nedlasting");
        assert_eq!(p.mottatt_ms, 1792670000000);
        assert_eq!(
            p.lenker,
            vec![
                "https://takeout.google.com/takeout/download?j=abc&i=0&user=1",
                "https://takeout.google.com/takeout/download?j=abc&i=1&user=1",
            ]
        );
    }

    #[test]
    fn annen_epost_gir_ingenting() {
        let data = URL_SAFE.encode("Sikkerhetsvarsel for kontoen din");
        let melding = serde_json::json!({"id": "m2", "payload": {"body": {"data": data}}});
        assert!(tolk_melding(&melding).is_none());
    }
}
