//! OAuth for skrivebordsapper: nettleseren åpnes, brukeren logger inn, og svaret kommer tilbake
//! til en midlertidig adresse på 127.0.0.1. PKCE gjør at en kode som fanges opp ikke kan brukes av andre.

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

/// Hvor lenge vi venter på at brukeren logger inn i nettleseren.
const VENTETID: Duration = Duration::from_secs(300);

pub struct Pkce {
    pub verifikator: String,
    pub utfordring: String,
}

impl Pkce {
    pub fn ny() -> Result<Self> {
        let mut tilfeldig = [0u8; 32];
        getrandom::getrandom(&mut tilfeldig).context("fant ingen tilfeldighetskilde")?;
        let verifikator = URL_SAFE_NO_PAD.encode(tilfeldig);
        let utfordring = URL_SAFE_NO_PAD.encode(Sha256::digest(verifikator.as_bytes()));
        Ok(Pkce { verifikator, utfordring })
    }
}

pub fn tilfeldig_tilstand() -> Result<String> {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).context("fant ingen tilfeldighetskilde")?;
    Ok(URL_SAFE_NO_PAD.encode(b))
}

/// Lytter på en ledig port på 127.0.0.1. Returnerer lytteren og adressen nettleseren skal sendes tilbake til.
pub fn lytt() -> Result<(TcpListener, String)> {
    let lytter = TcpListener::bind("127.0.0.1:0").context("kunne ikke åpne en lokal port for innloggingen")?;
    let port = lytter.local_addr()?.port();
    Ok((lytter, format!("http://127.0.0.1:{port}")))
}

/// Lytter på en fast port på 127.0.0.1, for tjenester som krever at adressen er registrert på forhånd
/// (Dropbox). Prøver neste port i lista hvis en er opptatt.
pub fn lytt_paa(porter: &[u16]) -> Result<(TcpListener, String)> {
    for &port in porter {
        if let Ok(lytter) = TcpListener::bind(("127.0.0.1", port)) {
            let port = lytter.local_addr()?.port();
            return Ok((lytter, format!("http://localhost:{port}/")));
        }
    }
    bail!("kunne ikke åpne en lokal port for innloggingen. Lukk andre programmer og prøv igjen")
}

/// Venter på at nettleseren kommer tilbake med `?code=...&state=...`, og svarer med en enkel side.
pub fn vent_paa_kode(lytter: TcpListener, tilstand: &str) -> Result<String> {
    lytter.set_nonblocking(true)?;
    let start = Instant::now();
    loop {
        match lytter.accept() {
            Ok((mut strom, _)) => {
                strom.set_nonblocking(false)?;
                strom.set_read_timeout(Some(Duration::from_secs(10)))?;
                let mut linje = String::new();
                BufReader::new(&strom).read_line(&mut linje)?;
                // Nettlesere spør også etter favicon.ico. Det har ingen kode og ignoreres.
                let Some(sti) = linje.split_whitespace().nth(1) else { continue };
                let svar = tolk_svar(sti, tilstand);
                let tekst = match &svar {
                    Some(Ok(_)) => "Innloggingen er ferdig. Du kan lukke denne fanen og gå tilbake til CloudJumper.",
                    Some(Err(_)) => "Innloggingen ble avbrutt. Gå tilbake til CloudJumper og prøv igjen.",
                    None => continue,
                };
                let html = format!(
                    "<!doctype html><meta charset=utf-8><title>CloudJumper</title><body style=\"font-family:Tahoma,sans-serif;padding:2em\"><p>{tekst}</p>"
                );
                let _ = write!(
                    strom,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
                    html.len()
                );
                return svar.unwrap();
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if start.elapsed() > VENTETID {
                    bail!("Fikk ikke svar fra innloggingen i nettleseren innen fem minutter");
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Leser `code` fra adressen nettleseren kom tilbake til. `None` betyr at forespørselen ikke var et svar.
pub fn tolk_svar(sti: &str, tilstand: &str) -> Option<Result<String>> {
    let url = url::Url::parse(&format!("http://127.0.0.1{sti}")).ok()?;
    let mut kode = None;
    let mut feil = None;
    let mut fikk_tilstand = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "code" => kode = Some(v.into_owned()),
            "error" => feil = Some(v.into_owned()),
            "state" => fikk_tilstand = Some(v.into_owned()),
            _ => {}
        }
    }
    if kode.is_none() && feil.is_none() {
        return None;
    }
    if fikk_tilstand.as_deref() != Some(tilstand) {
        return Some(Err(anyhow::anyhow!("Svaret fra innloggingen hørte ikke til denne innloggingen")));
    }
    Some(match (kode, feil) {
        (Some(k), _) => Ok(k),
        (None, Some(f)) if f == "access_denied" => Err(anyhow::anyhow!("Du sa nei til tilgangen i nettleseren")),
        (None, Some(f)) => Err(anyhow::anyhow!("Innloggingen feilet: {f}")),
        (None, None) => unreachable!(),
    })
}

#[cfg(test)]
mod tester {
    use super::*;

    #[test]
    fn pkce_er_riktig_kodet() {
        let p = Pkce::ny().unwrap();
        assert_eq!(p.verifikator.len(), 43);
        assert_eq!(URL_SAFE_NO_PAD.encode(Sha256::digest(p.verifikator.as_bytes())), p.utfordring);
    }

    #[test]
    fn tolker_svar_fra_nettleseren() {
        assert_eq!(tolk_svar("/?state=abc&code=4%2F0xyz&scope=x", "abc").unwrap().unwrap(), "4/0xyz");
        assert!(tolk_svar("/?state=feil&code=1", "abc").unwrap().is_err());
        assert!(tolk_svar("/?error=access_denied&state=abc", "abc").unwrap().is_err());
        assert!(tolk_svar("/favicon.ico", "abc").is_none());
    }

    #[test]
    fn mottar_kode_over_lokal_port() {
        let (lytter, adresse) = lytt().unwrap();
        let port = adresse.rsplit(':').next().unwrap().to_string();
        let t = std::thread::spawn(move || {
            let mut s = std::net::TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
            write!(s, "GET /?code=hemmelig&state=tt HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut svar = String::new();
            std::io::Read::read_to_string(&mut s, &mut svar).unwrap();
            svar
        });
        assert_eq!(vent_paa_kode(lytter, "tt").unwrap(), "hemmelig");
        assert!(t.join().unwrap().contains("Innloggingen er ferdig"));
    }
}
