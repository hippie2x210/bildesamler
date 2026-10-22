//! Skallet rundt kjernen: kommandoer grensesnittet kan kalle, og fremdrift tilbake.

use bildesamler_kjerne::{lag_plan, modell::{Datokilde, Tillit}, plan::Rolle, takeout, utfor, Plan};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct Tilstand(Mutex<Option<Plan>>);

#[derive(Clone, Serialize)]
struct Fremdrift {
    fase: &'static str,
    ferdig: usize,
    total: usize,
}

#[derive(Serialize)]
struct FilVisning {
    indeks: usize,
    sti: String,
    navn: String,
    til: String,
    bredde: u32,
    hoyde: u32,
    storrelse: u64,
    dato: Option<String>,
    kilde: String,
    tillit: Tillit,
    kamera: Option<String>,
    advarsler: Vec<String>,
}

#[derive(Serialize)]
struct GruppeVisning {
    indeks: usize,
    eksakt: bool,
    filer: Vec<FilVisning>,
}

#[derive(Serialize)]
struct Oversikt {
    rapport: bildesamler_kjerne::plan::Rapport,
    grupper: Vec<GruppeVisning>,
    advarsler: Vec<FilVisning>,
    uten_dato: Vec<FilVisning>,
    ulesbare: Vec<(String, String)>,
    varsler: Vec<String>,
}

fn fil_visning(plan: &Plan, i: usize) -> FilVisning {
    let f = &plan.filer[i];
    let til = plan
        .handlinger
        .iter()
        .find(|h| h.fil == i)
        .map(|h| h.til.to_string_lossy().into_owned())
        .unwrap_or_default();
    FilVisning {
        indeks: i,
        sti: f.sti.to_string_lossy().into_owned(),
        navn: f.filnavn(),
        til,
        bredde: f.bredde,
        hoyde: f.hoyde,
        storrelse: f.storrelse,
        dato: f.dato.map(|d| d.format("%d.%m.%Y %H:%M").to_string()),
        kilde: match f.datokilde {
            Datokilde::Exif => "EXIF",
            Datokilde::Video => "video",
            Datokilde::Filnavn => "filnavn",
            Datokilde::Takeout => "Google Foto",
            Datokilde::Ingen => "ingen",
        }
        .into(),
        tillit: f.tillit,
        kamera: f.kamera.clone(),
        advarsler: f.advarsler.clone(),
    }
}

fn oversikt(plan: &Plan) -> Oversikt {
    let grupper = plan
        .grupper
        .iter()
        .enumerate()
        .map(|(gi, g)| GruppeVisning {
            indeks: gi,
            eksakt: g.eksakt,
            filer: std::iter::once(g.original)
                .chain(g.duplikater.iter().copied())
                .map(|i| fil_visning(plan, i))
                .collect(),
        })
        .collect();
    let advarsler = (0..plan.filer.len())
        .filter(|&i| !plan.filer[i].advarsler.is_empty())
        .map(|i| fil_visning(plan, i))
        .collect();
    let uten_dato = plan
        .handlinger
        .iter()
        .filter(|h| h.rolle == Rolle::UtenDato)
        .map(|h| fil_visning(plan, h.fil))
        .collect();
    Oversikt {
        rapport: plan.rapport.clone(),
        grupper,
        advarsler,
        uten_dato,
        ulesbare: plan
            .ulesbare
            .iter()
            .map(|(s, e)| (s.to_string_lossy().into_owned(), e.clone()))
            .collect(),
        varsler: varsler_for_mal(&plan.mal),
    }
}

/// Advarer hvis målet synkes til skyen: da lastes hele samlingen opp igjen.
fn varsler_for_mal(mal: &Path) -> Vec<String> {
    let mut v = Vec::new();
    for var in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(rot) = std::env::var_os(var) {
            if !rot.is_empty() && mal.starts_with(&rot) {
                v.push(format!(
                    "Målmappen ligger i OneDrive ({}). Alt som kopieres dit, lastes opp til skyen igjen. Velg heller en mappe utenfor OneDrive eller en ekstern disk.",
                    PathBuf::from(rot).display()
                ));
                break;
            }
        }
    }
    v
}

#[tauri::command]
async fn skann(app: AppHandle, tilstand: State<'_, Tilstand>, kilder: Vec<String>, mal: String) -> Result<Oversikt, String> {
    if kilder.is_empty() {
        return Err("Velg minst én mappe å hente bilder fra.".into());
    }
    if mal.trim().is_empty() {
        return Err("Velg hvor bildene skal samles.".into());
    }
    let kilder: Vec<PathBuf> = kilder.into_iter().map(PathBuf::from).collect();
    let mal = PathBuf::from(mal);
    for k in &kilder {
        let er_zip = k.is_file() && takeout::er_zip(k);
        if !k.is_dir() && !er_zip {
            return Err(format!("Finner ikke mappen {}", k.display()));
        }
    }

    let app2 = app.clone();
    let (plan, takeout_varsler, kilder) = tauri::async_runtime::spawn_blocking(move || {
        // Takeout-zip-er pakkes ut i samlingens egen mappe først. Zip-filene røres ikke.
        let (kilder, varsler) = takeout::forbered_kilder(&kilder, &mal, &|ferdig, total| {
            let _ = app2.emit("fremdrift", Fremdrift { fase: "pakker ut", ferdig, total });
        });
        let plan = lag_plan(&kilder, &mal, &|ferdig, total| {
            if ferdig % 25 == 0 || ferdig == total {
                let _ = app2.emit("fremdrift", Fremdrift { fase: "skanner", ferdig, total });
            }
        });
        (plan, varsler, kilder)
    })
    .await
    .map_err(|e| e.to_string())?;

    // Grensesnittet får vise forhåndsbilder fra mappene brukeren selv har valgt.
    for k in &kilder {
        let _ = app.asset_protocol_scope().allow_directory(k, true);
    }

    let mut o = oversikt(&plan);
    o.varsler.extend(takeout_varsler);
    *tilstand.0.lock().unwrap() = Some(plan);
    Ok(o)
}

#[tauri::command]
fn sett_original(tilstand: State<'_, Tilstand>, gruppe: usize, fil: usize) -> Result<Oversikt, String> {
    let mut laas = tilstand.0.lock().unwrap();
    let plan = laas.as_mut().ok_or("Skann først.")?;
    plan.sett_original(gruppe, fil);
    Ok(oversikt(plan))
}

#[tauri::command]
async fn kopier(app: AppHandle, tilstand: State<'_, Tilstand>) -> Result<utfor::Resultat, String> {
    let plan = tilstand.0.lock().unwrap().clone().ok_or("Skann først.")?;
    std::fs::create_dir_all(&plan.mal).map_err(|e| format!("Kunne ikke lage målmappen: {e}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        utfor::utfor(&plan, &|ferdig, total| {
            if ferdig % 10 == 0 || ferdig == total {
                let _ = app.emit("fremdrift", Fremdrift { fase: "kopierer", ferdig, total });
            }
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn angre(mal: String) -> Result<usize, String> {
    utfor::angre_siste(Path::new(&mal)).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn apne_mappe(sti: String) -> Result<(), String> {
    tauri_plugin_opener::open_path(sti, None::<&str>).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Starter man appen en gang til, hentes vinduet som allerede er åpent fram.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| hent_fram(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Tilstand::default())
        .invoke_handler(tauri::generate_handler![skann, sett_original, kopier, angre, apne_mappe])
        .setup(|app| {
            hent_fram(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("CloudJumper kunne ikke starte");
}

/// Windows lar ofte et nytt vindu bli liggende bak andre programmer, så vi ber om fokus selv.
fn hent_fram(app: &AppHandle) {
    if let Some(vindu) = app.get_webview_window("main") {
        let _ = vindu.unminimize();
        let _ = vindu.show();
        let _ = vindu.set_focus();
    }
}
