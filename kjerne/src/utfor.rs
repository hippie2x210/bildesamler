//! Utfører en plan ved å KOPIERE filer til målmappen, og angrer ved å flytte kopiene til side.
//! Ingenting slettes noen gang, og kildefilene røres aldri.

use crate::plan::Plan;
use anyhow::{bail, Context, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

pub const LOGGMAPPE: &str = ".cloudjumper";
/// Navnet mappen hadde før appen het CloudJumper.
pub const GAMMEL_LOGGMAPPE: &str = ".bildesamler";

/// Gir en gammel `.bildesamler`-mappe det nye navnet, så angre-logger og nedlastinger blir med.
/// Bare et navnebytte i appens egen mappe; ingenting slettes.
pub fn flytt_gammel_mappe(mal: &Path) {
    let (gammel, ny) = (mal.join(GAMMEL_LOGGMAPPE), mal.join(LOGGMAPPE));
    if gammel.is_dir() && !ny.exists() {
        let _ = fs::rename(gammel, ny);
    }
}
pub const ANGRET: &str = "_Angret";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logglinje {
    pub kilde: PathBuf,
    /// Relativ til målmappen.
    pub til: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Resultat {
    pub kopiert: usize,
    pub allerede_der: usize,
    pub feil: Vec<(PathBuf, String)>,
    pub logg: Option<PathBuf>,
}

pub fn utfor(plan: &Plan, fremdrift: &dyn Fn(usize, usize)) -> Result<Resultat> {
    flytt_gammel_mappe(&plan.mal);
    let loggmappe = plan.mal.join(LOGGMAPPE).join("logg");
    fs::create_dir_all(&loggmappe).context("kunne ikke lage loggmappe i målet")?;
    let (loggsti, mut logg) = ny_logg(&loggmappe)?;
    let mut res = Resultat { logg: Some(loggsti), ..Default::default() };

    for (n, h) in plan.handlinger.iter().enumerate() {
        fremdrift(n, plan.handlinger.len());
        let fil = &plan.filer[h.fil];
        match kopier_en(&fil.sti, &plan.mal, &h.til, &fil.sha256) {
            Ok(Some(til)) => {
                res.kopiert += 1;
                let linje = Logglinje { kilde: fil.sti.clone(), til, sha256: fil.sha256.clone() };
                writeln!(logg, "{}", serde_json::to_string(&linje)?)?;
                logg.flush()?;
            }
            Ok(None) => res.allerede_der += 1,
            Err(e) => res.feil.push((fil.sti.clone(), format!("{e:#}"))),
        }
    }
    fremdrift(plan.handlinger.len(), plan.handlinger.len());
    Ok(res)
}

/// Lager en ny loggfil som aldri overskriver en eldre, selv om to kjøringer starter samme sekund.
fn ny_logg(mappe: &Path) -> Result<(PathBuf, fs::File)> {
    let tid = Local::now().format("%Y-%m-%d_%H%M%S");
    for n in 1.. {
        let sti = mappe.join(format!("{tid}_{n:03}.jsonl"));
        match fs::File::options().write(true).create_new(true).open(&sti) {
            Ok(f) => return Ok((sti, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    unreachable!()
}

/// Kopierer én fil og bekrefter sjekksummen. `None` betyr at akkurat denne filen allerede ligger der.
///
/// På Windows overskriver `rename` en fil som finnes. Derfor reserveres navnet først med en tom fil som
/// bare kan lages hvis navnet er ledig, og kopien flyttes over den. Da kan ingen annen fil bli overskrevet,
/// selv om noe dukker opp i mappen mens vi kopierer.
fn kopier_en(kilde: &Path, mal: &Path, til: &Path, sha: &str) -> Result<Option<PathBuf>> {
    let (stamme, ext) = del_navn(til);
    if let Some(forelder) = mal.join(til).parent() {
        fs::create_dir_all(forelder)?;
    }
    let mut n = 1;
    let (rel, full) = loop {
        let rel = if n == 1 { til.to_path_buf() } else { til.with_file_name(format!("{stamme} ({n}){ext}")) };
        let full = mal.join(&rel);
        match fs::File::options().write(true).create_new(true).open(&full) {
            Ok(_) => break (rel, full),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if sha256_fil(&full)? == sha {
                    return Ok(None);
                }
                // En tom fil er en plassholder fra en kjøring som ble avbrutt (strømbrudd, appen krasjet).
                // Den har ikke noe innhold å miste, så den brukes igjen i stedet for å bli stående.
                if fs::metadata(&full).is_ok_and(|m| m.len() == 0) {
                    break (rel, full);
                }
                n += 1;
            }
            Err(e) => return Err(e).with_context(|| format!("kunne ikke lage {}", full.display())),
        }
    };
    let tmp = full.with_extension("cloudjumper-del");
    let kopiert = fs::copy(kilde, &tmp)
        .with_context(|| format!("kopiering feilet: {}", kilde.display()))
        .and_then(|_| {
            if sha256_fil(&tmp)? != sha {
                // Ikke slett: la den halve filen ligge synlig så brukeren ser hva som skjedde.
                bail!("sjekksummen stemmer ikke etter kopiering, kopien ligger i {}", tmp.display());
            }
            Ok(())
        });
    if let Err(e) = kopiert {
        frigi(&full);
        return Err(e);
    }
    if let Ok(m) = fs::metadata(kilde).and_then(|m| m.modified()) {
        let _ = fs::File::options().write(true).open(&tmp).and_then(|f| f.set_modified(m));
    }
    // Erstatter bare den tomme plassholderen vi laget selv.
    if fs::metadata(&full).map(|m| m.len()).unwrap_or(1) != 0 {
        bail!("{} ble endret av et annet program mens vi kopierte. Kopien ligger i {}", full.display(), tmp.display());
    }
    fs::rename(&tmp, &full)?;
    Ok(Some(rel))
}

/// Fjerner den tomme plassholderen vi selv laget, hvis kopieringen feilet. Er den ikke tom lenger,
/// har noe annet skrevet til den, og da røres den ikke.
fn frigi(plassholder: &Path) {
    if fs::metadata(plassholder).is_ok_and(|m| m.len() == 0) {
        let _ = fs::remove_file(plassholder);
    }
}

/// Angrer siste kjøring: flytter kopiene den laget til `_Angret/<tidspunkt>/`, bortsett fra
/// filer som er endret siden. Returnerer antall flyttede filer.
pub fn angre_siste(mal: &Path) -> Result<usize> {
    flytt_gammel_mappe(mal);
    let loggmappe = mal.join(LOGGMAPPE).join("logg");
    let mut logger: Vec<PathBuf> = fs::read_dir(&loggmappe)
        .context("fant ingen tidligere kjøring å angre")?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    logger.sort();
    let Some(siste) = logger.pop() else { bail!("fant ingen tidligere kjøring å angre") };
    let navn = siste.file_stem().unwrap().to_string_lossy().into_owned();
    let angret = mal.join(ANGRET).join(&navn);

    let mut flyttet = 0;
    for linje in BufReader::new(fs::File::open(&siste)?).lines() {
        let l: Logglinje = serde_json::from_str(&linje?)?;
        let fra = mal.join(&l.til);
        if !fra.exists() || sha256_fil(&fra)? != l.sha256 {
            continue;
        }
        let til = angret.join(&l.til);
        fs::create_dir_all(til.parent().unwrap())?;
        fs::rename(&fra, &til)?;
        flyttet += 1;
    }
    fs::rename(&siste, siste.with_extension("angret"))?;
    Ok(flyttet)
}

/// Ledig plass på disken der samlingen skal ligge. Finnes ikke mappen ennå, brukes nærmeste mappe over.
pub fn ledig_plass(mal: &Path) -> Option<u64> {
    mal.ancestors().find(|p| p.is_dir()).and_then(|p| fs4::available_space(p).ok())
}

pub fn sha256_fil(sti: &Path) -> Result<String> {
    let mut f = fs::File::open(sti)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn del_navn(sti: &Path) -> (String, String) {
    let stamme = sti.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = sti.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (stamme, ext)
}
