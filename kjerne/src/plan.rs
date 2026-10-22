//! Tørrkjøring: bestemmer hvor hver fil skal, uten å røre noe.

use crate::likhet::Gruppe;
use crate::modell::{Fil, Medietype};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const MAPPE_BILDER: &str = "Bilder";
pub const MAPPE_VIDEOER: &str = "Videoer";
pub const MAPPE_DUPLIKATER: &str = "Duplikater";
pub const MAPPE_UTEN_DATO: &str = "Uten dato";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rolle {
    Original,
    Duplikat,
    UtenDato,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handling {
    pub fil: usize,
    /// Relativ sti under målmappen.
    pub til: PathBuf,
    pub rolle: Rolle,
    pub gruppe: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Rapport {
    pub filer: usize,
    pub bilder: usize,
    pub videoer: usize,
    pub duplikater: usize,
    pub duplikat_bytes: u64,
    pub uten_dato: usize,
    pub med_advarsel: usize,
    pub ulesbare: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub mal: PathBuf,
    pub filer: Vec<Fil>,
    pub grupper: Vec<Gruppe>,
    pub handlinger: Vec<Handling>,
    pub rapport: Rapport,
    /// Filer som ikke kunne leses, med feilmelding.
    pub ulesbare: Vec<(PathBuf, String)>,
}

impl Plan {
    pub fn ny(mal: &Path, filer: Vec<Fil>, grupper: Vec<Gruppe>, ulesbare: Vec<(PathBuf, String)>) -> Self {
        let mut plan = Plan {
            mal: mal.to_path_buf(),
            filer,
            grupper,
            handlinger: Vec::new(),
            rapport: Rapport::default(),
            ulesbare,
        };
        plan.bygg();
        plan
    }

    /// Brukeren overstyrer hvilken fil i en gruppe som er originalen.
    pub fn sett_original(&mut self, gruppe: usize, fil: usize) -> bool {
        let Some(g) = self.grupper.get_mut(gruppe) else { return false };
        let Some(pos) = g.duplikater.iter().position(|&d| d == fil) else {
            return g.original == fil;
        };
        g.duplikater[pos] = g.original;
        g.original = fil;
        self.bygg();
        true
    }

    fn bygg(&mut self) {
        let mut gruppe_for = vec![None; self.filer.len()];
        let mut original_for = vec![None; self.filer.len()];
        for (gi, g) in self.grupper.iter().enumerate() {
            gruppe_for[g.original] = Some(gi);
            for &d in &g.duplikater {
                gruppe_for[d] = Some(gi);
                original_for[d] = Some(g.original);
            }
        }

        let mut brukt = HashSet::new();
        let mut handlinger = Vec::with_capacity(self.filer.len());
        let mut r = Rapport { filer: self.filer.len(), ulesbare: self.ulesbare.len(), ..Default::default() };

        for (i, f) in self.filer.iter().enumerate() {
            let navn = trygt_filnavn(&f.filnavn());
            let (mappe, rolle) = if let Some(o) = original_for[i] {
                r.duplikater += 1;
                r.duplikat_bytes += f.storrelse;
                // Duplikater sorteres etter originalens dato, så de er lette å finne igjen.
                let dato = self.filer[o].dato.or(f.dato);
                let under = match dato {
                    Some(d) => PathBuf::from(d.format("%Y").to_string()).join(d.format("%m").to_string()),
                    None => PathBuf::from(MAPPE_UTEN_DATO),
                };
                (PathBuf::from(MAPPE_DUPLIKATER).join(under), Rolle::Duplikat)
            } else {
                match f.medietype {
                    Medietype::Bilde => r.bilder += 1,
                    Medietype::Video => r.videoer += 1,
                }
                match f.dato {
                    Some(d) => {
                        let topp = match f.medietype {
                            Medietype::Bilde => MAPPE_BILDER,
                            Medietype::Video => MAPPE_VIDEOER,
                        };
                        let mappe = PathBuf::from(topp).join(d.format("%Y").to_string()).join(d.format("%m").to_string());
                        (mappe, Rolle::Original)
                    }
                    None => {
                        r.uten_dato += 1;
                        (PathBuf::from(MAPPE_UTEN_DATO), Rolle::UtenDato)
                    }
                }
            };
            if !f.advarsler.is_empty() {
                r.med_advarsel += 1;
            }
            let til = ledig_navn(&mappe, &navn, &mut brukt);
            handlinger.push(Handling { fil: i, til, rolle, gruppe: gruppe_for[i] });
        }
        self.handlinger = handlinger;
        self.rapport = r;
    }
}

/// Finner et navn som ikke er brukt i planen. Windows skiller ikke på store og små bokstaver.
fn ledig_navn(mappe: &Path, navn: &str, brukt: &mut HashSet<String>) -> PathBuf {
    let (stamme, ext) = match navn.rfind('.') {
        Some(p) if p > 0 => (&navn[..p], &navn[p..]),
        _ => (navn, ""),
    };
    let mut n = 1;
    loop {
        let kandidat = if n == 1 { navn.to_string() } else { format!("{stamme} ({n}){ext}") };
        let sti = mappe.join(&kandidat);
        if brukt.insert(sti.to_string_lossy().to_lowercase()) {
            return sti;
        }
        n += 1;
    }
}

/// Fjerner tegn Windows ikke tillater i filnavn (skytjenester tillater dem).
pub fn trygt_filnavn(navn: &str) -> String {
    let renset: String = navn
        .chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect();
    let renset = renset.trim_end_matches(['.', ' ']).to_string();
    let stamme = renset.split('.').next().unwrap_or("").to_uppercase();
    let reservert = matches!(stamme.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stamme.starts_with("COM") || stamme.starts_with("LPT")) && stamme.len() == 4);
    if renset.is_empty() || reservert {
        format!("_{renset}")
    } else {
        renset
    }
}

#[cfg(test)]
mod tester {
    use super::*;

    #[test]
    fn renser_filnavn() {
        assert_eq!(trygt_filnavn("ferie: dag 1?.jpg"), "ferie_ dag 1_.jpg");
        assert_eq!(trygt_filnavn("CON.jpg"), "_CON.jpg");
        assert_eq!(trygt_filnavn("bilde. "), "bilde");
        assert_eq!(trygt_filnavn("Julaften på hytta æøå.jpg"), "Julaften på hytta æøå.jpg");
    }

    #[test]
    fn like_navn_far_nummer() {
        let mut brukt = HashSet::new();
        let m = Path::new("Bilder/2019/07");
        assert_eq!(ledig_navn(m, "IMG.jpg", &mut brukt), m.join("IMG.jpg"));
        assert_eq!(ledig_navn(m, "img.JPG", &mut brukt), m.join("img (2).JPG"));
    }
}
