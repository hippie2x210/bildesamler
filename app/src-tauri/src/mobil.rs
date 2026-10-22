//! Det telefonen trenger i stedet for vanlige mappedialoger og CloudJumper sitt eget Google-vindu.
//!
//! Android har ingen dialog som gir en vanlig filsti til en mappe, så grensesnittet viser sin egen enkle
//! mappeliste fra `mappeliste`. Det virker fordi appen ber om tilgang til alle filer (se MainActivity.kt).

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Der nettleseren på telefonen legger nedlastinger, også Takeout-filene fra Google.
#[cfg(mobile)]
pub const NEDLASTINGER: &str = "/storage/emulated/0/Download";

/// Forslag til samling på telefonen: en ekstern disk eller et minnekort hvis det er koblet til, ellers
/// Bilder/CloudJumper på telefonen.
#[cfg(mobile)]
pub fn standard_samling() -> Option<String> {
    let ekstern = std::fs::read_dir("/storage").ok().and_then(|d| {
        d.flatten()
            .map(|e| e.path())
            .filter(|p| !matches!(p.file_name().and_then(|n| n.to_str()), Some("emulated" | "self")))
            .find(|p| std::fs::read_dir(p).is_ok())
    });
    let rot = ekstern.unwrap_or_else(|| PathBuf::from("/storage/emulated/0/Pictures"));
    Some(rot.join("CloudJumper").to_string_lossy().into_owned())
}

/// På telefonen åpnes Google-sidene i nettleseren. Filene havner da i Nedlastinger.
#[cfg(mobile)]
pub fn apne_nettside(app: &tauri::AppHandle, adresse: &str, _mal: Option<&str>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(adresse, None::<&str>).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct Oppforing {
    navn: String,
    sti: String,
}

#[derive(Serialize)]
pub struct Mappeliste {
    sti: String,
    forelder: Option<String>,
    mapper: Vec<Oppforing>,
    /// Bare filer med en av de ønskede endelsene, for eksempel zip for Takeout.
    filer: Vec<Oppforing>,
}

/// Undermappene (og eventuelt filene) i en mappe, sortert etter navn. Skjulte mapper vises ikke.
#[tauri::command]
pub fn mappeliste(sti: String, endelser: Option<Vec<String>>) -> Result<Mappeliste, String> {
    let mappe = Path::new(&sti);
    let lest = std::fs::read_dir(mappe).map_err(|e| format!("Kan ikke åpne {}: {e}", mappe.display()))?;
    let endelser: Vec<String> = endelser.unwrap_or_default().iter().map(|e| e.to_lowercase()).collect();
    let (mut mapper, mut filer) = (Vec::new(), Vec::new());
    for e in lest.flatten() {
        let navn = e.file_name().to_string_lossy().into_owned();
        if navn.starts_with('.') {
            continue;
        }
        let p = e.path();
        let o = Oppforing { navn, sti: p.to_string_lossy().into_owned() };
        if p.is_dir() {
            mapper.push(o);
        } else if p
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| endelser.contains(&x.to_lowercase()))
        {
            filer.push(o);
        }
    }
    let sorter = |v: &mut Vec<Oppforing>| v.sort_by_key(|o| o.navn.to_lowercase());
    sorter(&mut mapper);
    sorter(&mut filer);
    Ok(Mappeliste {
        sti: sti.clone(),
        forelder: mappe.parent().map(|p| p.to_string_lossy().into_owned()),
        mapper,
        filer,
    })
}

#[cfg(test)]
mod tester {
    use super::*;

    #[test]
    fn viser_mapper_og_bare_valgte_filer() {
        let d = std::env::temp_dir().join(format!("cj-mappeliste-{}", std::process::id()));
        std::fs::create_dir_all(d.join("Bilder")).unwrap();
        std::fs::create_dir_all(d.join(".skjult")).unwrap();
        std::fs::write(d.join("takeout-1.ZIP"), b"").unwrap();
        std::fs::write(d.join("notat.txt"), b"").unwrap();
        let l = mappeliste(d.to_string_lossy().into_owned(), Some(vec!["zip".into()])).unwrap();
        assert_eq!(l.mapper.iter().map(|o| o.navn.as_str()).collect::<Vec<_>>(), ["Bilder"]);
        assert_eq!(l.filer.iter().map(|o| o.navn.as_str()).collect::<Vec<_>>(), ["takeout-1.ZIP"]);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
