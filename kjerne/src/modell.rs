use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Medietype {
    Bilde,
    Video,
}

/// Hvor datoen kommer fra. Rekkefølgen her er også tilliten: EXIF først, fildato sist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Datokilde {
    Exif,
    Video,
    Filnavn,
    /// Datoen Google Foto har lagret i Takeout-JSON-filen.
    Takeout,
    Ingen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tillit {
    Ingen = 0,
    Middels = 1,
    Hoy = 2,
}

/// Alt vi vet om én fil etter skanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fil {
    pub sti: PathBuf,
    pub storrelse: u64,
    pub sha256: String,
    /// Perseptuell hash (dHash, 256 bit). Bare for bilder vi kan dekode.
    pub phash: Option<[u64; 4]>,
    pub bredde: u32,
    pub hoyde: u32,
    pub medietype: Medietype,
    pub kamera: Option<String>,
    pub har_gps: bool,
    pub metadata_dato: Option<NaiveDateTime>,
    pub filnavn_dato: Option<NaiveDateTime>,
    /// Datoen fra Google Takeout sin JSON-fil, hvis bildet kom fra Takeout.
    #[serde(default)]
    pub takeout_dato: Option<NaiveDateTime>,
    /// Endringstid fra filsystemet. Brukes aldri automatisk, bare som forslag.
    pub fildato: Option<NaiveDateTime>,
    pub dato: Option<NaiveDateTime>,
    pub datokilde: Datokilde,
    pub tillit: Tillit,
    pub advarsler: Vec<String>,
    /// Ligger allerede i samlingen fra en tidligere kjøring. Kopieres aldri, men brukes til å finne duplikater.
    #[serde(default)]
    pub i_samlingen: bool,
}

impl Fil {
    pub fn piksler(&self) -> u64 {
        self.bredde as u64 * self.hoyde as u64
    }

    pub fn filnavn(&self) -> String {
        self.sti
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}
