//! Skallet rundt kjernen: kommandoer grensesnittet kan kalle, og fremdrift tilbake.

use cloudjumper_kjerne::{lag_plan, miniatyr, modell::{Datokilde, Tillit}, plan::Rolle, takeout, utfor, Plan};
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
    rapport: cloudjumper_kjerne::plan::Rapport,
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

/// Telefoner koblet til med kabel.
#[tauri::command]
async fn telefoner() -> Result<Vec<cloudjumper_telefon::Telefon>, String> {
    tauri::async_runtime::spawn_blocking(cloudjumper_telefon::finn)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

/// Kopierer bildene fra telefonen til `<samling>/.cloudjumper/mobil/<telefon>`, som så kan skannes.
/// Telefonen åpnes bare for lesing.
#[tauri::command]
async fn telefon_hent(app: AppHandle, id: String, navn: String, mal: String) -> Result<cloudjumper_telefon::Henting, String> {
    if mal.trim().is_empty() {
        return Err("Velg hvor bildene skal samles.".into());
    }
    utfor::flytt_gammel_mappe(Path::new(&mal));
    let mappe = PathBuf::from(&mal).join(utfor::LOGGMAPPE).join("mobil").join(cloudjumper_telefon::trygt(&navn));
    tauri::async_runtime::spawn_blocking(move || {
        cloudjumper_telefon::hent(&id, &mappe, &|ferdig, total| {
            let _ = app.emit("fremdrift", Fremdrift { fase: "telefon", ferdig, total });
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn angre(mal: String) -> Result<usize, String> {
    // Kan flytte mange filer, så det skjer utenfor hovedtråden og grensesnittet ikke fryser.
    tauri::async_runtime::spawn_blocking(move || utfor::angre_siste(Path::new(&mal)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
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
async fn google_bestill() -> Result<(), String> {
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

/// Forklaringen nederst i Google-vinduet, så brukeren vet hva hen skal trykke på. På siden med
/// eksportens filer legger den også en knapp «Last ned alle», så ingen må trykke «Last ned» 14 ganger.
/// Skriptet sender ingenting fra siden, bortsett fra adressene til nedlastingene når knappen trykkes.
const GOOGLE_HJELP: &str = r#"
(function () {
  if (window.top !== window) return;
  var status = "", opptatt = false, sist = "";
  function lenker() {
    var sett = {}, ut = [];
    var a = document.querySelectorAll('a[href*="takeout/download"]');
    for (var i = 0; i < a.length; i++) {
      var h = a[i].href;
      if (!sett[h]) { sett[h] = 1; ut.push(h); }
    }
    return ut;
  }
  // Reserve hvis Google ikke bruker vanlige lenker: knappene som heter «Last ned».
  function knapper() {
    var ut = [], e = document.querySelectorAll('a,button,[role="button"]');
    for (var i = 0; i < e.length; i++) {
      var t = (e[i].innerText || "").trim();
      if (/(^|\s)(Last ned|Download)$/i.test(t) && e[i].id !== "cloudjumper-alle") ut.push(e[i]);
    }
    // En lenke rundt en knapp skal bare trykkes én gang.
    return ut.filter(function (x) { return !ut.some(function (y) { return y !== x && y.contains(x); }); });
  }
  function antallFiler() { return lenker().length || knapper().length; }
  function klikkAlle() {
    var k = knapper(), i = 0;
    (function neste() {
      if (i >= k.length) {
        status = "Alle " + k.length + " nedlastingene er startet. La vinduet stå åpent til CloudJumper sier at de er ferdige.";
        opptatt = false; vis(); return;
      }
      status = "Starter fil " + (i + 1) + " av " + k.length + " …"; vis();
      k[i++].click();
      setTimeout(neste, 5000);
    })();
  }
  function tekst(antall) {
    var h = location.host, p = location.pathname;
    if (h.indexOf("accounts.google.") === 0)
      return "Logg inn med Google-kontoen din. CloudJumper ser aldri passordet ditt.";
    if (h !== "takeout.google.com") return "";
    if (status) return status;
    if (antall)
      return "Google har delt bildene i " + (antall === 1 ? "én fil" : antall + " filer") + ". Trykk den grønne knappen, så henter CloudJumper alle.";
    if (p.indexOf("/takeout/download") === 0) return "Filen lastes ned til CloudJumper. Du kan vente her.";
    if (p.indexOf("/settings/takeout/custom") === 0)
      return "Bare Google Foto er valgt. Bla helt ned og trykk «Neste trinn», og så «Opprett eksport». Google sender en e-post når bildene er klare (ofte noen timer). Du kan lukke vinduet så lenge.";
    return "Trykk på den øverste linjen i listen (den nyeste) for å se filene. Står det ikke «Ferdig» ennå, er Google ikke klar.";
  }
  function vis() {
    var antall = antallFiler(), t = tekst(antall), d = document.getElementById("cloudjumper-hjelp");
    var nokkel = t + "|" + antall + "|" + opptatt;
    if (nokkel === sist && (d || !t)) return;
    sist = nokkel;
    if (!t) { if (d) d.remove(); return; }
    if (!d) {
      d = document.createElement("div");
      d.id = "cloudjumper-hjelp";
      d.style.cssText = "position:fixed;left:0;right:0;bottom:0;z-index:2147483647;background:#ffffcc;color:#000;border-top:3px solid #0a246a;font:20px Tahoma,Segoe UI,sans-serif;padding:14px 18px;box-shadow:0 -2px 8px rgba(0,0,0,.3);pointer-events:none;display:flex;align-items:center;gap:16px";
      (document.body || document.documentElement).appendChild(d);
    }
    d.textContent = "";
    var s = document.createElement("span");
    s.style.flex = "1";
    s.textContent = "CloudJumper: " + t;
    d.appendChild(s);
    if (antall && !opptatt) {
      var b = document.createElement("button");
      b.id = "cloudjumper-alle";
      b.textContent = "Last ned alle (" + antall + ")";
      b.style.cssText = "pointer-events:auto;font:bold 20px Tahoma,Segoe UI,sans-serif;padding:10px 22px;background:#2fb32f;color:#fff;border:2px solid #1d7a1d;border-radius:4px;cursor:pointer";
      b.onclick = function () {
        status = "Starter nedlastingen …";
        opptatt = true;
        vis();
        if (!lenker().length) { klikkAlle(); return; }
        location.href = "https://cloudjumper.invalid/last-ned-alle?l=" + encodeURIComponent(JSON.stringify(lenker()));
      };
      d.appendChild(b);
    }
  }
  window.cloudjumperStatus = function (t, ferdig) { status = t; opptatt = !ferdig; vis(); };
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

/// «Last ned alle»: filene som venter, og hvor mange som lastes ned akkurat nå. To om gangen holder
/// linja i bruk uten at en treg forbindelse må dele seg på 14 store filer.
#[derive(Default)]
struct Ko {
    venter: std::collections::VecDeque<tauri::Url>,
    aktive: usize,
    totalt: usize,
    ferdige: usize,
    feilet: usize,
}

const SAMTIDIGE: usize = 2;
static KO: Mutex<Ko> = Mutex::new(Ko { venter: std::collections::VecDeque::new(), aktive: 0, totalt: 0, ferdige: 0, feilet: 0 });

/// Starter neste fil i køen hvis det er plass, og viser fremdriften nederst i Google-vinduet.
fn neste_i_ko(app: &AppHandle) {
    let (neste, tekst) = {
        let mut ko = KO.lock().unwrap();
        let neste = if ko.aktive < SAMTIDIGE { ko.venter.pop_front() } else { None };
        if neste.is_some() {
            ko.aktive += 1;
        }
        // (tekst, ferdig): når køen er tom kan knappen vises igjen.
        let tekst = if ko.totalt == 0 {
            None
        } else if ko.ferdige + ko.feilet < ko.totalt {
            Some((
                format!(
                    "Henter filene fra Google: {} av {} er ferdige. La vinduet stå åpent, dette kan ta lang tid.",
                    ko.ferdige, ko.totalt
                ),
                false,
            ))
        } else if ko.feilet > 0 {
            Some((
                format!(
                    "{} av {} filer stoppet underveis. Trykk «Last ned alle» igjen. Filene som er ferdige, hentes ikke på nytt.",
                    ko.feilet, ko.totalt
                ),
                true,
            ))
        } else {
            Some((format!("Alle {} filene er hentet. Lukk dette vinduet og trykk «Sorter bildene» i CloudJumper.", ko.totalt), true))
        };
        (neste, tekst)
    };
    let Some(vindu) = app.get_webview_window("google") else { return };
    if let Some((t, ferdig)) = tekst {
        let _ = vindu.eval(format!(
            "window.cloudjumperStatus && window.cloudjumperStatus({}, {ferdig})",
            serde_json::json!(t)
        ));
    }
    if let Some(url) = neste {
        let _ = vindu.navigate(url);
    }
}

/// Filene som er lastet helt ned før. De hoppes over hvis brukeren trykker «Last ned alle» igjen.
fn ferdigliste(mappe: &Path) -> PathBuf {
    mappe.join("ferdige.txt")
}

fn er_ferdig(mappe: &Path, fil: &Path) -> bool {
    let Some(navn) = fil.file_name().and_then(|n| n.to_str()) else { return false };
    fil.exists() && std::fs::read_to_string(ferdigliste(mappe)).is_ok_and(|s| s.lines().any(|l| l == navn))
}

fn merk_ferdig(mappe: &Path, fil: &Path) {
    use std::io::Write;
    if let Some(navn) = fil.file_name().and_then(|n| n.to_str()) {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(ferdigliste(mappe)) {
            let _ = writeln!(f, "{navn}");
        }
    }
}

/// Mappen Takeout-zip-ene lastes ned til: i samlingen hvis den er valgt, ellers i appens egen mappe.
fn nedlastingsmappe(app: &AppHandle, mal: Option<&str>) -> Result<PathBuf, String> {
    let mappe = match mal.filter(|m| !m.trim().is_empty()) {
        Some(m) => {
            utfor::flytt_gammel_mappe(Path::new(m));
            PathBuf::from(m).join(utfor::LOGGMAPPE).join("nedlastet")
        }
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
    let app3 = app.clone();
    let vindu = tauri::WebviewWindowBuilder::new(app, "google", tauri::WebviewUrl::External(url))
        .title("CloudJumper – Google Foto")
        .inner_size(1000.0, 800.0)
        .initialization_script(GOOGLE_HJELP)
        .on_navigation(move |url| {
            if url.host_str() != Some("cloudjumper.invalid") {
                return true;
            }
            let lenker: Vec<tauri::Url> = url
                .query_pairs()
                .find(|(k, _)| k == "l")
                .and_then(|(_, v)| serde_json::from_str::<Vec<String>>(&v).ok())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|l| l.parse::<tauri::Url>().ok())
                .filter(|l| l.host_str() == Some("takeout.google.com"))
                .collect();
            {
                let mut ko = KO.lock().unwrap();
                *ko = Ko { totalt: lenker.len(), venter: lenker.into(), ..Ko::default() };
            }
            let app = app3.clone();
            tauri::async_runtime::spawn(async move {
                for _ in 0..SAMTIDIGE {
                    neste_i_ko(&app);
                    // Litt pause, så den første nedlastingen rekker å starte før neste side åpnes.
                    pause(std::time::Duration::from_secs(3)).await;
                }
            });
            false
        })
        .on_download(move |_, hendelse| {
            match hendelse {
                tauri::webview::DownloadEvent::Requested { destination, .. } => {
                    let navn = destination.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "takeout.zip".into());
                    *destination = mappe.join(navn);
                    if er_ferdig(&mappe, destination) {
                        // Hentet før: hopp over og gå videre til neste.
                        {
                            let mut ko = KO.lock().unwrap();
                            ko.aktive = ko.aktive.saturating_sub(1);
                            ko.ferdige += 1;
                        }
                        let _ = app2.emit(
                            "google-nedlasting",
                            GoogleNedlasting { fil: destination.to_string_lossy().into_owned(), ferdig: true, ok: true },
                        );
                        let app = app2.clone();
                        tauri::async_runtime::spawn(async move { neste_i_ko(&app) });
                        return false;
                    }
                    let _ = app2.emit(
                        "google-nedlasting",
                        GoogleNedlasting { fil: destination.to_string_lossy().into_owned(), ferdig: false, ok: true },
                    );
                }
                tauri::webview::DownloadEvent::Finished { path, success, .. } => {
                    if let (Some(p), true) = (&path, success) {
                        merk_ferdig(&mappe, p);
                    }
                    let i_ko = {
                        let mut ko = KO.lock().unwrap();
                        let i_ko = ko.totalt > 0;
                        if i_ko {
                            ko.aktive = ko.aktive.saturating_sub(1);
                            if success {
                                ko.ferdige += 1;
                            } else {
                                ko.feilet += 1;
                            }
                        }
                        i_ko
                    };
                    if i_ko {
                        let app = app2.clone();
                        tauri::async_runtime::spawn(async move { neste_i_ko(&app) });
                    }
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
    let _ = vindu.set_focus();
    Ok(())
}

/// Åpner Takeout med bare Google Foto valgt, inne i CloudJumper.
/// Må være async: på Windows låser appen seg hvis et vindu lages fra en synkron kommando.
#[tauri::command]
async fn google_vindu(app: AppHandle, mal: Option<String>, side: Option<String>) -> Result<(), String> {
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

/// Forhåndsvisninger lages bare for filer appen har fått lov til å vise, og lagres i appens mellomlager
/// så de ikke må lages på nytt neste gang.
fn miniatyr(app: &AppHandle, sti: &Path) -> Result<Vec<u8>, String> {
    use std::hash::{Hash, Hasher};
    if !app.asset_protocol_scope().is_allowed(sti) {
        return Err("ikke tillatt".into());
    }
    let meta = std::fs::metadata(sti).map_err(|e| e.to_string())?;
    let mut h = std::hash::DefaultHasher::new();
    (sti, meta.len(), meta.modified().ok()).hash(&mut h);
    let lager = app.path().app_cache_dir().ok().map(|d| d.join("miniatyrer").join(format!("{:016x}.jpg", h.finish())));
    if let Some(data) = lager.as_ref().and_then(|l| std::fs::read(l).ok()) {
        return Ok(data);
    }
    let data = {
        // Høyst fire bilder pakkes ut samtidig, ellers kan en lang liste fylle minnet.
        let _plass = Plasser::ta();
        miniatyr::lag(sti).map_err(|e| format!("{e:#}"))?
    };
    if let Some(l) = lager {
        let _ = std::fs::create_dir_all(l.parent().unwrap());
        let _ = std::fs::write(l, &data);
    }
    Ok(data)
}

struct Plasser;

static LEDIGE: (Mutex<usize>, std::sync::Condvar) = (Mutex::new(4), std::sync::Condvar::new());

impl Plasser {
    fn ta() -> Self {
        let mut ledige = LEDIGE.1.wait_while(LEDIGE.0.lock().unwrap(), |n| *n == 0).unwrap();
        *ledige -= 1;
        Plasser
    }
}

impl Drop for Plasser {
    fn drop(&mut self) {
        *LEDIGE.0.lock().unwrap() += 1;
        LEDIGE.1.notify_one();
    }
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
        // mini://<sti> gir en liten forhåndsvisning av bildet i stedet for hele originalen.
        .register_asynchronous_uri_scheme_protocol("mini", |ctx, foresporsel, svar| {
            let app = ctx.app_handle().clone();
            let kodet = foresporsel.uri().path().trim_start_matches('/').to_string();
            tauri::async_runtime::spawn_blocking(move || {
                let sti = PathBuf::from(percent_encoding::percent_decode_str(&kodet).decode_utf8_lossy().into_owned());
                let respons = match miniatyr(&app, &sti) {
                    Ok(data) => tauri::http::Response::builder()
                        .header("Content-Type", "image/jpeg")
                        .header("Cache-Control", "max-age=31536000")
                        .body(data),
                    Err(e) => tauri::http::Response::builder().status(404).body(e.into_bytes()),
                };
                svar.respond(respons.unwrap());
            });
        })
        .invoke_handler(tauri::generate_handler![
            skann,
            sett_original,
            kopier,
            angre,
            telefoner,
            telefon_hent,
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
