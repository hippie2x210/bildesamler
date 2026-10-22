//! Kjernen i Bildesamler: skanner mapper, leser datoer, finner duplikater og lager en plan.
//! Grunnregel: kjernen sletter aldri en fil.

pub mod dato;
pub mod likhet;
pub mod metadata;
pub mod modell;
pub mod plan;
pub mod takeout;
pub mod utfor;

use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub use plan::Plan;

/// Finner alle bilder og videoer under kildene. Hopper over målmappen og egne mapper.
pub fn finn_filer(kilder: &[PathBuf], mal: &Path) -> Vec<PathBuf> {
    let mal = mal.canonicalize().unwrap_or_else(|_| mal.to_path_buf());
    let mut filer = Vec::new();
    for kilde in kilder {
        for e in walkdir::WalkDir::new(kilde)
            .into_iter()
            .filter_entry(|e| {
                let navn = e.file_name().to_string_lossy();
                let egen = navn == utfor::LOGGMAPPE || navn == utfor::ANGRET;
                let er_mal = e.path().canonicalize().is_ok_and(|p| p == mal);
                !(egen || er_mal)
            })
            .filter_map(Result::ok)
        {
            if e.file_type().is_file() && metadata::medietype(e.path()).is_some() {
                filer.push(e.into_path());
            }
        }
    }
    filer.sort();
    filer.dedup();
    filer
}

/// Skanner kildene og lager en plan (tørrkjøring). Ingen filer endres.
pub fn lag_plan(kilder: &[PathBuf], mal: &Path, fremdrift: &(dyn Fn(usize, usize) + Sync)) -> Plan {
    let stier = finn_filer(kilder, mal);
    let ferdig = AtomicUsize::new(0);
    let total = stier.len();
    let resultater: Vec<_> = stier
        .par_iter()
        .map(|s| {
            let r = metadata::les(s);
            fremdrift(ferdig.fetch_add(1, Ordering::Relaxed) + 1, total);
            (s.clone(), r)
        })
        .collect();

    let mut filer = Vec::new();
    let mut ulesbare = Vec::new();
    for (sti, r) in resultater {
        match r {
            Ok(f) => filer.push(f),
            Err(e) => ulesbare.push((sti, format!("{e:#}"))),
        }
    }
    let grupper = likhet::grupper(&filer);
    Plan::ny(mal, filer, grupper, ulesbare)
}
