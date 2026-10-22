//! Grupperer like filer og velger hvilken som er originalen.

use crate::metadata::avstand;
use crate::modell::{Fil, Medietype};
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Maks antall ulike biter (av 256) for at to bilder regnes som samme motiv.
/// Målt på testsettet: ny størrelse og hard komprimering gir 3–15, ulike motiver 60+.
/// Serieopptak kan ligge på 12–17, så de skilles på opptakstid i stedet (se `nesten_like`).
pub const TERSKEL: u32 = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gruppe {
    /// Indeks i fil-listen til filen som beholdes.
    pub original: usize,
    pub duplikater: Vec<usize>,
    pub eksakt: bool,
}

pub fn grupper(filer: &[Fil]) -> Vec<Gruppe> {
    let mut uf = UnionFind::new(filer.len());

    let mut per_hash: HashMap<&str, usize> = HashMap::new();
    for (i, f) in filer.iter().enumerate() {
        if let Some(&j) = per_hash.get(f.sha256.as_str()) {
            uf.forbind(i, j);
        } else {
            per_hash.insert(&f.sha256, i);
        }
    }

    // Unike bilder med perseptuell hash sammenlignes parvis.
    let kandidater: Vec<usize> = per_hash
        .values()
        .copied()
        .filter(|&i| filer[i].medietype == Medietype::Bilde && filer[i].phash.is_some())
        .collect();
    let par: Vec<(usize, usize)> = kandidater
        .par_iter()
        .enumerate()
        .flat_map_iter(|(n, &i)| {
            kandidater[n + 1..]
                .iter()
                .filter(move |&&j| nesten_like(&filer[i], &filer[j]))
                .map(move |&j| (i, j))
        })
        .collect();
    for (i, j) in par {
        uf.forbind(i, j);
    }

    let mut medlemmer: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..filer.len() {
        medlemmer.entry(uf.rot(i)).or_default().push(i);
    }
    let mut grupper: Vec<Gruppe> = medlemmer
        .into_values()
        .filter(|m| m.len() > 1)
        .map(|mut m| {
            m.sort_by(|&a, &b| rangering(&filer[b]).cmp(&rangering(&filer[a])).then(a.cmp(&b)));
            let eksakt = m.iter().all(|&i| filer[i].sha256 == filer[m[0]].sha256);
            Gruppe { original: m[0], duplikater: m[1..].to_vec(), eksakt }
        })
        .collect();
    grupper.sort_by_key(|g| g.original);
    grupper
}

fn nesten_like(a: &Fil, b: &Fil) -> bool {
    let (Some(ha), Some(hb)) = (&a.phash, &b.phash) else {
        return false;
    };
    if avstand(ha, hb) > TERSKEL {
        return false;
    }
    // Ulikt format (beskåret) er ikke samme bilde.
    let forhold = |f: &Fil| f.bredde as f64 / f.hoyde.max(1) as f64;
    if (forhold(a) / forhold(b) - 1.0).abs() > 0.03 {
        return false;
    }
    // En redigert versjon («-redigert», «-edited») er et eget bilde brukeren har laget, ikke en kopi.
    if er_redigert(&a.filnavn()) != er_redigert(&b.filnavn()) {
        return false;
    }
    // To bilder med hver sin sikre opptakstid er serieopptak, ikke kopier.
    !matches!((a.metadata_dato, b.metadata_dato), (Some(x), Some(y)) if x != y)
}

/// Høyere er bedre. Riktig dato og metadata teller mest, deretter kvalitet.
fn rangering(f: &Fil) -> (u8, bool, bool, u64, u64, bool, std::cmp::Reverse<usize>) {
    (
        f.tillit as u8,
        f.kamera.is_some(),
        f.har_gps,
        f.piksler(),
        f.storrelse,
        !ser_ut_som_kopi(&f.filnavn()),
        std::cmp::Reverse(f.sti.as_os_str().len()),
    )
}

pub fn ser_ut_som_kopi(navn: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\(\d+\)|\bkopi\b|\bcopy\b|-WA\d{4}|_edited|-redigert").unwrap())
        .is_match(navn)
}

pub fn er_redigert(navn: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)[-_ ](edited|redigert|bearbeitet|modifié|editado)\b").unwrap())
        .is_match(navn)
}

struct UnionFind(Vec<usize>);

impl UnionFind {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }
    fn rot(&mut self, mut i: usize) -> usize {
        while self.0[i] != i {
            self.0[i] = self.0[self.0[i]];
            i = self.0[i];
        }
        i
    }
    fn forbind(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.rot(a), self.rot(b));
        if ra != rb {
            self.0[ra.max(rb)] = ra.min(rb);
        }
    }
}
