//! Bildene som allerede ligger i samlingen fra tidligere kjøringer. De leses inn så nye kilder kan
//! sammenlignes med dem: et bilde som finnes fra før, skal ikke havne i `Bilder` en gang til.
//! Det som er lest lagres i `.cloudjumper/samling.json`, så bare nye eller endrede filer må leses neste gang.
//! Samlingen røres aldri: filene her blir verken kopiert, flyttet eller endret.

use crate::metadata;
use crate::modell::Fil;
use crate::plan::{MAPPE_BILDER, MAPPE_DUPLIKATER, MAPPE_UTEN_DATO, MAPPE_VIDEOER};
use crate::utfor::LOGGMAPPE;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const MELLOMLAGER: &str = "samling.json";
/// Økes når `metadata::les` gir et annet svar enn før, så det som er lagret leses på nytt.
/// 2: bildene snus etter EXIF før den perseptuelle hashen lages.
const LESEVERSJON: u32 = 2;

/// Det vi vet om samlingen.
#[derive(Debug, Default)]
pub struct Samling {
    /// Bildene og videoene i `Bilder`, `Videoer` og `Uten dato`. Nye filer sammenlignes med dem.
    pub originaler: Vec<Fil>,
    /// Sjekksummen til alt i samlingen, også `Duplikater`. En fil med samme innhold kopieres ikke igjen.
    pub sha: HashSet<String>,
}

#[derive(Serialize, Deserialize)]
struct Lagret {
    #[serde(default)]
    versjon: u32,
    storrelse: u64,
    endret: Option<i64>,
    fil: Fil,
}

fn endret(m: &fs::Metadata) -> Option<i64> {
    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(t.as_secs() as i64)
}

/// Relativ sti med `/`, så mellomlageret virker selv om disken får en ny bokstav.
fn nokkel(rel: &Path) -> String {
    rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// Leser samlingen i `mal`. En tom eller ny samling gir en tom liste.
pub fn les(mal: &Path, fremdrift: &(dyn Fn(usize, usize) + Sync)) -> Samling {
    let lagerfil = mal.join(LOGGMAPPE).join(MELLOMLAGER);
    let mut lager: HashMap<String, Lagret> = fs::read(&lagerfil)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();

    // (relativ sti, original?, størrelse, endret)
    let mut funnet = Vec::new();
    for (mappe, original) in [(MAPPE_BILDER, true), (MAPPE_VIDEOER, true), (MAPPE_UTEN_DATO, true), (MAPPE_DUPLIKATER, false)] {
        for e in walkdir::WalkDir::new(mal.join(mappe)).into_iter().filter_map(Result::ok) {
            if !e.file_type().is_file() || metadata::medietype(e.path()).is_none() {
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            let Ok(rel) = e.path().strip_prefix(mal) else { continue };
            funnet.push((rel.to_path_buf(), original, m.len(), endret(&m)));
        }
    }

    let nye: Vec<&(PathBuf, bool, u64, Option<i64>)> = funnet
        .iter()
        .filter(|(rel, _, storrelse, endret)| {
            !lager
                .get(&nokkel(rel))
                .is_some_and(|l| l.versjon == LESEVERSJON && l.storrelse == *storrelse && l.endret == *endret)
        })
        .collect();
    let ferdig = AtomicUsize::new(0);
    let leste: Vec<(String, Lagret)> = nye
        .par_iter()
        .filter_map(|(rel, _, storrelse, endret)| {
            let fil = metadata::les(&mal.join(rel)).ok();
            fremdrift(ferdig.fetch_add(1, Ordering::Relaxed) + 1, nye.len());
            Some((nokkel(rel), Lagret { versjon: LESEVERSJON, storrelse: *storrelse, endret: *endret, fil: fil? }))
        })
        .collect();
    let endret_noe = !leste.is_empty() || lager.len() != funnet.len();
    lager.extend(leste);

    let mut samling = Samling::default();
    let mut beholdt = HashMap::with_capacity(funnet.len());
    for (rel, original, _, _) in &funnet {
        let n = nokkel(rel);
        let Some(l) = lager.remove(&n) else { continue };
        samling.sha.insert(l.fil.sha256.clone());
        if *original {
            let mut fil = l.fil.clone();
            fil.sti = mal.join(rel);
            fil.i_samlingen = true;
            samling.originaler.push(fil);
        }
        beholdt.insert(n, l);
    }

    if endret_noe {
        lagre(&lagerfil, &beholdt);
    }
    samling
}

/// Skriver mellomlageret. Feiler det, leses samlingen bare på nytt neste gang.
fn lagre(sti: &Path, lager: &HashMap<String, Lagret>) {
    let Ok(data) = serde_json::to_vec(lager) else { return };
    let tmp = sti.with_extension("json.cloudjumper-del");
    if sti.parent().is_some_and(|p| fs::create_dir_all(p).is_ok()) && fs::write(&tmp, data).is_ok() {
        let _ = fs::rename(&tmp, sti);
    }
}

#[cfg(test)]
mod tester {
    use super::*;

    fn bilde(sti: &Path, lys: u8) {
        fs::create_dir_all(sti.parent().unwrap()).unwrap();
        image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([lys, (x * 4) as u8, (y * 5) as u8])).save(sti).unwrap();
    }

    #[test]
    fn leser_samlingen_og_husker_den() {
        let d = tempfile::tempdir().unwrap();
        let mal = d.path();
        bilde(&mal.join("Bilder/2019/07/IMG_20190704_120000.jpg"), 10);
        bilde(&mal.join("Duplikater/2019/07/IMG-20190704-WA0001.jpg"), 20);
        bilde(&mal.join("_Angret/2026-10-22/Bilder/2019/07/x.jpg"), 30);

        let s = les(mal, &|_, _| {});
        assert_eq!(s.originaler.len(), 1);
        assert!(s.originaler[0].i_samlingen);
        assert_eq!(s.sha.len(), 2);
        assert!(mal.join(".cloudjumper/samling.json").exists());

        // Andre gang leses ingenting på nytt.
        let lest = AtomicUsize::new(0);
        let s = les(mal, &|_, _| {
            lest.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(lest.load(Ordering::Relaxed), 0);
        assert_eq!(s.originaler.len(), 1);
        assert_eq!(s.originaler[0].sti, mal.join("Bilder/2019/07/IMG_20190704_120000.jpg"));
    }
}
