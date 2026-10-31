//! Henter bilder og videoer fra en mobil koblet til med kabel, via Windows sin egen driver (WPD).
//! Telefonen åpnes bare for lesing: ingenting på den endres eller slettes.
//!
//! Filene kopieres til en mellommappe (`<samling>/.cloudjumper/mobil/<telefon>/...`) som så sorteres
//! på vanlig måte. Filer som allerede er hentet med samme størrelse hoppes over, så en avbrutt
//! henting fortsetter der den slapp.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Telefon {
    /// Windows sin enhets-ID. Brukes for å åpne telefonen.
    pub id: String,
    /// Navnet brukeren kjenner igjen, for eksempel «Galaxy S21» eller «Apple iPhone».
    pub navn: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Henting {
    pub hentet: usize,
    pub fantes: usize,
    pub feil: Vec<String>,
    pub mappe: PathBuf,
}

/// Mapper på telefonen vi leter i. Resten (apper, systemfiler) hoppes over, ellers tar det evig tid.
const TOPPMAPPER: &[&str] = &["dcim", "pictures", "movies", "camera", "download", "whatsapp", "snapchat", "telegram"];
const WHATSAPP_MEDIA: &[&str] = &["android", "media", "com.whatsapp", "whatsapp", "media"];

pub const MEDIETYPER: &[&str] = &[
    "jpg", "jpeg", "png", "heic", "heif", "webp", "gif", "bmp", "tif", "tiff", "dng", "mp4", "mov", "m4v", "3gp", "avi",
    "mkv",
];

/// Skal vi gå inn i denne mappen? `sti` er mappenavnene fra lagringsområdets rot, med små bokstaver.
pub fn interessant(sti: &[String]) -> bool {
    let Some(forste) = sti.first() else { return true };
    if TOPPMAPPER.contains(&forste.as_str()) {
        return true;
    }
    // Android/media/com.whatsapp/WhatsApp/Media: bare veien dit, og alt under.
    sti.iter().zip(WHATSAPP_MEDIA).all(|(a, b)| a == b)
}

pub fn er_media(navn: &str) -> bool {
    navn.rsplit_once('.').is_some_and(|(_, e)| MEDIETYPER.contains(&e.to_lowercase().as_str()))
}

/// Gjør et navn fra telefonen trygt som mappenavn på Windows.
pub fn trygt(navn: &str) -> String {
    let s: String = navn.chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect();
    let s = s.trim().trim_end_matches('.').to_string();
    if s.is_empty() {
        "_".into()
    } else {
        s
    }
}

/// Finnes filen allerede helt hentet?
pub fn allerede_hentet(sti: &Path, storrelse: Option<u64>) -> bool {
    match (std::fs::metadata(sti), storrelse) {
        (Ok(m), Some(s)) => m.len() == s,
        (Ok(m), None) => m.len() > 0,
        _ => false,
    }
}

#[cfg(windows)]
mod wpd;

#[cfg(windows)]
pub use wpd::{finn, hent, tell};

#[cfg(not(windows))]
pub fn finn() -> anyhow::Result<Vec<Telefon>> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub fn tell(_id: &str) -> anyhow::Result<(usize, u64)> {
    Ok((0, 0))
}

#[cfg(not(windows))]
pub fn hent(_id: &str, _mal: &Path, _fremdrift: &dyn Fn(usize, usize)) -> anyhow::Result<Henting> {
    anyhow::bail!("Henting fra telefon med kabel virker bare på Windows")
}

#[cfg(test)]
mod tester {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn leter_bare_i_bildemapper() {
        assert!(interessant(&s(&["dcim"])));
        assert!(interessant(&s(&["dcim", "camera"])));
        assert!(interessant(&s(&["android"])));
        assert!(interessant(&s(&["android", "media", "com.whatsapp"])));
        assert!(interessant(&s(&["android", "media", "com.whatsapp", "whatsapp", "media", "whatsapp images"])));
        assert!(!interessant(&s(&["android", "data"])));
        assert!(!interessant(&s(&["music"])));
    }

    #[test]
    fn kjenner_igjen_bilder() {
        assert!(er_media("IMG_0001.HEIC"));
        assert!(er_media("VID_20190704.mp4"));
        assert!(!er_media("notat.txt"));
        assert!(!er_media("uten_endelse"));
    }

    #[test]
    fn trygge_navn() {
        assert_eq!(trygt("Intern lagring"), "Intern lagring");
        assert_eq!(trygt("SD:kort?"), "SD_kort_");
        assert_eq!(trygt(" . "), "_");
    }
}
