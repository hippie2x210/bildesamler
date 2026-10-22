//! Skallet rundt kjernen: kommandoer grensesnittet kan kalle, og fremdrift tilbake.

use bildesamler_kjerne::{lag_plan, modell::{Datokilde, Tillit}, plan::Rolle, takeout, utfor, Plan};
use cloudjumper_sky::google;
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

// --- Google Foto via Takeout og Gmail ---

/// Klienten fra Google Cloud-prosjektet. Bygget inn ved kompilering (GitHub-hemmelighet), ellers lest fra
/// `google-klient.json` i appens innstillingsmappe eller ved siden av programmet.
fn google_klient(app: &AppHandle) -> Result<google::Klient, String> {
    if let Some(innebygd) = option_env!("CLOUDJUMPER_GOOGLE_KLIENT").filter(|s| !s.trim().is_empty()) {
        return google::Klient::fra_json(innebygd).map_err(|e| format!("{e:#}"));
    }
    let mut steder = Vec::new();
    if let Ok(d) = app.path().app_config_dir() {
        steder.push(d.join("google-klient.json"));
    }
    if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        steder.push(d.join("google-klient.json"));
    }
    for sti in &steder {
        if let Ok(tekst) = std::fs::read_to_string(sti) {
            return google::Klient::fra_json(&tekst).map_err(|e| format!("{e:#}"));
        }
    }
    Err(format!(
        "Google-innlogging er ikke satt opp i denne versjonen. Legg google-klient.json i {}",
        steder.first().map(|p| p.display().to_string()).unwrap_or_default()
    ))
}

#[derive(Serialize)]
struct GoogleStatus {
    /// Gmail-sjekk er mulig (klienten fra Google Cloud finnes).
    satt_opp: bool,
    innlogget: bool,
    nedlastinger: Option<String>,
    /// Forslag til samling: Bilder\CloudJumper i brukerens egen bildemappe.
    standard_samling: Option<String>,
}

#[tauri::command]
fn google_status(app: AppHandle) -> GoogleStatus {
    let sti = |p: Result<PathBuf, tauri::Error>| p.ok().map(|p| p.to_string_lossy().into_owned());
    GoogleStatus {
        satt_opp: google_klient(&app).is_ok(),
        innlogget: google::er_innlogget(),
        nedlastinger: sti(app.path().download_dir()),
        standard_samling: sti(app.path().picture_dir().map(|p| p.join("CloudJumper"))),
    }
}

/// Mappen nedlastinger fra Google-vinduet havner i, så veiviseren kan skanne den.
#[tauri::command]
fn google_nedlastingsmappe(app: AppHandle, mal: Option<String>) -> Result<String, String> {
    nedlastingsmappe(&app, mal.as_deref()).map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
fn google_bestill() -> Result<(), String> {
    tauri_plugin_opener::open_url(google::TAKEOUT_FOTO, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
async fn google_logg_inn(app: AppHandle) -> Result<String, String> {
    let klient = google_klient(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        google::logg_inn(&klient, &|adresse| {
            tauri_plugin_opener::open_url(adresse, None::<&str>).map_err(|e| anyhow::anyhow!(e.to_string()))
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn google_logg_ut() -> Result<(), String> {
    google::logg_ut().map_err(|e| format!("{e:#}"))
}

/// Forklaringen som vises øverst i Google-vinduet, så brukeren vet hva hen skal trykke på.
/// Skriptet leser ingenting fra siden, det legger bare en stripe med tekst over den.
const GOOGLE_HJELP: &str = r#"
(function () {
  if (window.top !== window) return;
  function tekst() {
    var h = location.host, p = location.pathname;
    if (h.indexOf("accounts.google.") === 0) return "Logg inn med Google-kontoen din. CloudJumper ser aldri passordet ditt.";
    if (h === "takeout.google.com" && p.indexOf("/settings/takeout/downloads") === 0)
      return "Trykk «Last ned» ved hver fil. CloudJumper tar imot filene og sorterer bildene selv.";
    if (h === "takeout.google.com" && p.indexOf("/takeout/download") === 0)
      return "Filen lastes ned til CloudJumper. Du kan vente her.";
    if (h === "takeout.google.com")
      return "Bare Google Foto er valgt. Bla helt ned og trykk «Neste trinn», og så «Opprett eksport». Google sender en e-post når bildene er klare (ofte noen timer). Du kan lukke vinduet så lenge.";
    return "";
  }
  function vis() {
    var t = tekst(), d = document.getElementById("cloudjumper-hjelp");
    if (!t) { if (d) d.remove(); return; }
    if (!d) {
      d = document.createElement("div");
      d.id = "cloudjumper-hjelp";
      d.style.cssText = "position:fixed;left:0;right:0;bottom:0;z-index:2147483647;background:#ffffcc;color:#000;border-top:3px solid #0a246a;font:20px Tahoma,Segoe UI,sans-serif;padding:14px 18px;box-shadow:0 -2px 8px rgba(0,0,0,.3);pointer-events:none";
      (document.body || document.documentElement).appendChild(d);
    }
    d.textContent = "CloudJumper: " + t;
  }
  document.addEventListener("DOMContentLoaded", vis);
  setInterval(vis, 1500);
})();
"#;

#[derive(Clone, Serialize)]
struct GoogleNedlasting {
    fil: String,
    ferdig: bool,
    ok: bool,
}

/// Mappen Takeout-zip-ene lastes ned til: i samlingen hvis den er valgt, ellers i appens egen mappe.
fn nedlastingsmappe(app: &AppHandle, mal: Option<&str>) -> Result<PathBuf, String> {
    let mappe = match mal.filter(|m| !m.trim().is_empty()) {
        Some(m) => PathBuf::from(m).join(utfor::LOGGMAPPE).join("nedlastet"),
        None => app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("nedlastet"),
    };
    std::fs::create_dir_all(&mappe).map_err(|e| format!("Kunne ikke lage {}: {e}", mappe.display()))?;
    Ok(mappe)
}

/// Åpner (eller bytter side i) CloudJumper sitt eget Google-vindu. Brukeren forlater aldri appen,
/// og nedlastinger fanges opp og legges i `nedlastet`-mappen i stedet for i Nedlastinger.
fn apne_google_vindu(app: &AppHandle, adresse: &str, mal: Option<&str>) -> Result<(), String> {
    let url: tauri::Url = adresse.parse().map_err(|e| format!("{e}"))?;
    if let Some(vindu) = app.get_webview_window("google") {
        vindu.navigate(url).map_err(|e| e.to_string())?;
        let _ = vindu.unminimize();
        let _ = vindu.set_focus();
        return Ok(());
    }
    let mappe = nedlastingsmappe(app, mal)?;
    let app2 = app.clone();
    tauri::WebviewWindowBuilder::new(app, "google", tauri::WebviewUrl::External(url))
        .title("CloudJumper – Google Foto")
        .inner_size(1000.0, 800.0)
        .initialization_script(GOOGLE_HJELP)
        .on_download(move |_, hendelse| {
            match hendelse {
                tauri::webview::DownloadEvent::Requested { destination, .. } => {
                    let navn = destination.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "takeout.zip".into());
                    *destination = mappe.join(navn);
                    let _ = app2.emit(
                        "google-nedlasting",
                        GoogleNedlasting { fil: destination.to_string_lossy().into_owned(), ferdig: false, ok: true },
                    );
                }
                tauri::webview::DownloadEvent::Finished { path, success, .. } => {
                    let _ = app2.emit(
                        "google-nedlasting",
                        GoogleNedlasting {
                            fil: path.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
                            ferdig: true,
                            ok: success,
                        },
                    );
                }
                _ => {}
            }
            true
        })
        .build()
        .map_err(|e| format!("Kunne ikke åpne Google-vinduet: {e}"))?;
    Ok(())
}

/// Åpner Takeout med bare Google Foto valgt, inne i CloudJumper.
#[tauri::command]
fn google_vindu(app: AppHandle, mal: Option<String>, side: Option<String>) -> Result<(), String> {
    let adresse = match side.as_deref() {
        Some("nedlastinger") => google::TAKEOUT_NEDLASTINGER,
        _ => google::TAKEOUT_FOTO,
    };
    apne_google_vindu(&app, adresse, mal.as_deref())
}

/// Finner den nyeste Takeout-e-posten i Gmail og åpner nedlastingslenkene i Google-vinduet.
#[tauri::command]
async fn google_hent(app: AppHandle, mal: Option<String>) -> Result<Option<google::TakeoutPost>, String> {
    let klient = google_klient(&app)?;
    let poster = tauri::async_runtime::spawn_blocking(move || google::finn_takeout(&klient))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?;
    let Some(nyeste) = poster.into_iter().next() else { return Ok(None) };
    for lenke in &nyeste.lenker {
        apne_google_vindu(&app, lenke, mal.as_deref())?;
        // Litt pause mellom delene, så hver nedlasting rekker å starte.
        pause(std::time::Duration::from_secs(3)).await;
    }
    Ok(Some(nyeste))
}

async fn pause(d: std::time::Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Starter man appen en gang til, hentes vinduet som allerede er åpent fram.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| hent_fram(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Tilstand::default())
        .invoke_handler(tauri::generate_handler![
            skann,
            sett_original,
            kopier,
            angre,
            apne_mappe,
            google_status,
            google_bestill,
            google_vindu,
            google_nedlastingsmappe,
            google_logg_inn,
            google_logg_ut,
            google_hent
        ])
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
