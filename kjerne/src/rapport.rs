//! En vanlig tekstfil i samlingen som forteller hva CloudJumper har gjort, kjøring for kjøring.
//! Den er ment for den som hjelper til senere: «hvor ble bildene av, og hva ble lagt til side?».

use crate::plan::{Plan, Rolle};
use crate::utfor::Resultat;
use anyhow::Result;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub const FILNAVN: &str = "Hva CloudJumper har gjort.txt";

fn tidspunkt() -> String {
    chrono::Local::now().format("%d.%m.%Y kl. %H:%M").to_string()
}

fn legg_til(mal: &Path, tekst: &str) -> Result<PathBuf> {
    let sti = mal.join(FILNAVN);
    let ny = !sti.exists();
    let mut f = fs::File::options().create(true).append(true).open(&sti)?;
    if ny {
        // Notisblokk på eldre Windows trenger BOM for å vise æ, ø og å riktig.
        f.write_all("\u{feff}Denne filen skrives av CloudJumper. Den nyeste kjøringen står nederst.\r\n".as_bytes())?;
    }
    f.write_all(tekst.replace('\n', "\r\n").as_bytes())?;
    Ok(sti)
}

/// Beskriver én kopiering i klartekst.
pub fn tekst(plan: &Plan, res: &Resultat, kilde: &str) -> String {
    let mut t = String::new();
    let antall = |rolle: Rolle| plan.handlinger.iter().filter(|h| h.rolle == rolle).count();
    let _ = writeln!(t, "\n=== {} ===", tidspunkt());
    if !kilde.is_empty() {
        let _ = writeln!(t, "Hentet fra: {kilde}");
    }
    let _ = writeln!(t, "Kopiert nå: {}", res.kopiert);
    let _ = writeln!(t, "  Bilder og videoer sortert i år og måned: {}", antall(Rolle::Original));
    let _ = writeln!(t, "  Lagt i «Duplikater» fordi de finnes fra før: {}", antall(Rolle::Duplikat));
    let _ = writeln!(t, "  Lagt i «Uten dato»: {}", antall(Rolle::UtenDato));
    let i_samlingen = plan.rapport.allerede_i_samlingen + res.allerede_der;
    if i_samlingen > 0 {
        let _ = writeln!(t, "Lå allerede i samlingen og ble ikke kopiert igjen: {i_samlingen}");
    }
    if plan.rapport.ulesbare > 0 {
        let _ = writeln!(t, "Kunne ikke leses og ble hoppet over: {}", plan.rapport.ulesbare);
        for (sti, feil) in plan.ulesbare.iter().take(50) {
            let _ = writeln!(t, "  {} ({feil})", sti.display());
        }
    }
    if !res.feil.is_empty() {
        let _ = writeln!(t, "Kunne ikke kopieres: {}", res.feil.len());
        for (sti, feil) in res.feil.iter().take(50) {
            let _ = writeln!(t, "  {} ({feil})", sti.display());
        }
    }
    let _ = writeln!(t, "Ingenting er slettet. Originalene ligger der de lå.");
    let _ = writeln!(t, "Angre: trykk «Angre» i CloudJumper. Kopiene fra denne kjøringen flyttes da til «_Angret».");
    t
}

/// Legger til en kopiering i rapporten. Feiler det, er kopieringen likevel gjort.
pub fn skriv(plan: &Plan, res: &Resultat, kilde: &str) -> Result<PathBuf> {
    legg_til(&plan.mal, &tekst(plan, res, kilde))
}

pub fn skriv_angret(mal: &Path, flyttet: usize) -> Result<PathBuf> {
    legg_til(
        mal,
        &format!(
            "\n=== {} ===\nAngret siste kopiering: {flyttet} filer flyttet til «_Angret» (eller tilbake til Google-mappen).\n",
            tidspunkt()
        ),
    )
}

#[cfg(test)]
mod tester {
    use super::*;

    #[test]
    fn skriver_rapport_som_vokser() {
        let d = tempfile::tempdir().unwrap();
        let plan = Plan::ny(d.path(), Vec::new(), Vec::new(), Vec::new());
        let res = Resultat { kopiert: 3, ..Default::default() };
        let sti = skriv(&plan, &res, "Mobilen").unwrap();
        skriv_angret(d.path(), 3).unwrap();
        let innhold = fs::read_to_string(&sti).unwrap();
        assert!(innhold.starts_with('\u{feff}'));
        assert!(innhold.contains("Hentet fra: Mobilen\r\n"));
        assert!(innhold.contains("Kopiert nå: 3"));
        assert!(innhold.contains("Angret siste kopiering: 3"));
        assert_eq!(innhold.matches("Denne filen skrives").count(), 1);
    }
}
