//! Google Takeout: pakker ut zip-filer og leser datoen Google lagrer i JSON-filen ved siden av hvert bilde.
//! Zip-filene røres aldri. Utpakkingen kan avbrytes og fortsetter der den slapp.

use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDateTime};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::metadata;

/// Mappe under samlingens loggmappe der Takeout-zip-er pakkes ut.
pub const UTPAKKET: &str = "takeout";

pub fn er_zip(sti: &Path) -> bool {
    sti.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// Takeout-zip-er heter `takeout-20241022T101500Z-001.zip`. Andre zip-er i en mappe lar vi være.
pub fn er_takeout_zip(sti: &Path) -> bool {
    er_zip(sti)
        && sti
            .file_name()
            .is_some_and(|n| n.to_string_lossy().to_lowercase().starts_with("takeout-"))
}

/// Er dette Google Foto-mappen i en Takeout? Navnet følger kontospråket:
/// «Google Photos», «Google Foto», «Google Fotos», «Google Photo» …
pub fn er_foto_mappe(navn: &str) -> bool {
    let n = navn.to_lowercase();
    n.starts_with("google photo") || n.starts_with("google foto")
}

/// Hører filen til Google Foto? En Takeout kan også ha Disk, Kontakter, spill og mye annet,
/// og de bildene skal ikke inn i samlingen. Zip-er uten `Takeout/`-mappe tas med som de er.
fn fra_google_foto(navn: &Path) -> bool {
    let mut deler = navn.components().map(|c| c.as_os_str().to_string_lossy().into_owned());
    match deler.next() {
        Some(topp) if topp.eq_ignore_ascii_case("takeout") => deler.next().is_some_and(|t| er_foto_mappe(&t)),
        _ => true,
    }
}

/// Pakker ut bilder, videoer og JSON-filer fra Google Foto-delen av én zip til `mappe`. Filer som allerede er pakket ut
/// med riktig størrelse hoppes over, så en avbrutt utpakking kan fortsette.
/// Zip-biblioteket kontrollerer CRC for hver fil vi leser helt ut.
pub fn pakk_ut(zip: &Path, mappe: &Path) -> Result<usize> {
    let fil = fs::File::open(zip).with_context(|| format!("kunne ikke åpne {}", zip.display()))?;
    let mut arkiv = zip::ZipArchive::new(io::BufReader::new(fil))
        .with_context(|| format!("{} er ikke en hel zip-fil. Er nedlastingen ferdig?", filnavn(zip)))?;
    let mut antall = 0;
    for i in 0..arkiv.len() {
        let mut del = arkiv.by_index(i)?;
        if del.is_dir() {
            continue;
        }
        // enclosed_name avviser stier som prøver å skrive utenfor mappen (../).
        let Some(navn) = del.enclosed_name() else { continue };
        if !fra_google_foto(&navn) {
            continue;
        }
        let er_json = navn.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"));
        if !er_json && metadata::medietype(&navn).is_none() {
            continue;
        }
        let mal = mappe.join(&navn);
        if fs::metadata(&mal).is_ok_and(|m| m.len() == del.size()) {
            antall += 1;
            continue;
        }
        if let Some(forelder) = mal.parent() {
            fs::create_dir_all(forelder)?;
        }
        let tmp = mal.with_extension("cloudjumper-tmp");
        let mut ut = fs::File::create(&tmp)?;
        io::copy(&mut del, &mut ut).with_context(|| format!("feil i {} inne i {}", navn.display(), filnavn(zip)))?;
        ut.sync_all()?;
        drop(ut);
        fs::rename(&tmp, &mal)?;
        antall += 1;
    }
    Ok(antall)
}

/// Bytter Takeout-zip-er blant kildene med mappene de pakkes ut i, under `<mål>/.cloudjumper/takeout`.
/// En kildemappe beholdes, og Takeout-zip-er rett i den (som i Nedlastinger) pakkes ut i tillegg.
/// Returnerer de nye kildene og varsler for zip-er som ikke kunne leses.
fn filnavn(sti: &Path) -> String {
    sti.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Er nedlastingen av denne zip-en fortsatt i gang? Nettlesere lager en tom fil med det endelige navnet
/// og skriver til en midlertidig fil ved siden av (`.part` i Firefox, `.crdownload` i Chrome og Edge).
pub fn lastes_ned(zip: &Path) -> bool {
    if fs::metadata(zip).is_ok_and(|m| m.len() == 0) {
        return true;
    }
    let (Some(mappe), Some(stamme)) = (zip.parent(), zip.file_stem().map(|s| s.to_string_lossy().into_owned())) else {
        return false;
    };
    fs::read_dir(mappe).is_ok_and(|innhold| {
        innhold.filter_map(Result::ok).any(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            n.starts_with(&stamme.to_lowercase()) && (n.ends_with(".part") || n.ends_with(".crdownload"))
        })
    })
}

/// `takeout-…-001(1).zip` eller `takeout-…-001 (2).zip` er samme fil lastet ned flere ganger.
/// Er originalen der med samme størrelse, trengs ikke kopien.
fn er_ekstra_nedlasting(zip: &Path) -> bool {
    let Some(navn) = zip.file_name().map(|n| n.to_string_lossy().into_owned()) else { return false };
    let Some(stamme) = navn.strip_suffix(".zip").or_else(|| navn.strip_suffix(".ZIP")) else { return false };
    let Some(start) = stamme.rfind('(') else { return false };
    if !stamme.ends_with(')') || stamme[start + 1..stamme.len() - 1].parse::<u32>().is_err() {
        return false;
    }
    let original = zip.with_file_name(format!("{}.zip", stamme[..start].trim_end()));
    match (fs::metadata(zip), fs::metadata(&original)) {
        (Ok(a), Ok(b)) => a.len() == b.len(),
        _ => false,
    }
}

/// Mappene med Google Foto i en utpakket Takeout (`Takeout/Google Foto`), eller hele mappen
/// hvis zip-en ikke var bygget som en vanlig Takeout.
fn foto_mapper(utpakket: &Path) -> Vec<PathBuf> {
    let takeout = fs::read_dir(utpakket)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case("takeout"))
        .map(|e| e.path());
    let Some(takeout) = takeout else { return vec![utpakket.to_path_buf()] };
    fs::read_dir(takeout)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir() && er_foto_mappe(&e.file_name().to_string_lossy()))
        .map(|e| e.path())
        .collect()
}

pub fn forbered_kilder(
    kilder: &[PathBuf],
    mal: &Path,
    fremdrift: &dyn Fn(usize, usize),
) -> (Vec<PathBuf>, Vec<String>) {
    crate::utfor::flytt_gammel_mappe(mal);
    let mut zipper = Vec::new();
    let mut nye = Vec::new();
    for kilde in kilder {
        if kilde.is_file() && er_zip(kilde) {
            zipper.push(kilde.clone());
            continue;
        }
        if let Ok(innhold) = fs::read_dir(kilde) {
            let mut funnet: Vec<_> = innhold.filter_map(Result::ok).map(|e| e.path()).filter(|p| er_takeout_zip(p)).collect();
            funnet.sort();
            zipper.extend(funnet);
        }
        nye.push(kilde.clone());
    }

    let rot = mal.join(crate::utfor::LOGGMAPPE).join(UTPAKKET);
    let mut varsler = Vec::new();
    zipper.retain(|z| !er_ekstra_nedlasting(z));
    let mut venter = Vec::new();
    zipper.retain(|z| {
        let ja = lastes_ned(z);
        if ja {
            venter.push(filnavn(z));
        }
        !ja
    });
    if !venter.is_empty() {
        varsler.push(format!(
            "{} lastes fortsatt ned og blir ikke med denne gangen: {}. Skann på nytt når nedlastingen er ferdig.",
            if venter.len() == 1 { "Én fil fra Google" } else { "Noen filer fra Google" },
            venter.join(", ")
        ));
    }
    for (i, zip) in zipper.iter().enumerate() {
        fremdrift(i, zipper.len());
        let stamme = zip.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let mappe = rot.join(stamme);
        match pakk_ut(zip, &mappe) {
            Ok(_) => nye.extend(foto_mapper(&mappe)),
            Err(e) => varsler.push(format!("{e:#}")),
        }
    }
    fremdrift(zipper.len(), zipper.len());
    (nye, varsler)
}

/// Datoen bildet ble tatt, slik Google har lagret den i JSON-filen.
/// For bilder uten EXIF er dette ofte opplastingstiden, så den regnes ikke som sikker.
pub fn sidecar_dato(sti: &Path) -> Option<NaiveDateTime> {
    let json = finn_sidecar(sti)?;
    let verdi: serde_json::Value = serde_json::from_slice(&fs::read(json).ok()?).ok()?;
    let sekunder: i64 = match &verdi["photoTakenTime"]["timestamp"] {
        serde_json::Value::String(s) => s.parse().ok()?,
        serde_json::Value::Number(n) => n.as_i64()?,
        _ => return None,
    };
    let utc = DateTime::from_timestamp(sekunder, 0)?;
    Some(utc.with_timezone(&Local).naive_local())
}

/// Google kutter JSON-navn til 46 tegn før `.json`. Kortere avkuttede navn regnes ikke som treff.
const MIN_KUTTET: usize = 40;

/// Finner JSON-filen som hører til et bilde. Google bruker flere navneformer:
/// `IMG_1.jpg.json`, `IMG_1.jpg.supplemental-metadata.json`, navn kuttet etter 46 tegn
/// (`IMG_veldig_langt_navn.jp.json`), `IMG_1(1).jpg` → `IMG_1.jpg(1).json`, og redigerte
/// kopier (`IMG_1-redigert.jpg`) som deler JSON med originalen.
pub fn finn_sidecar(sti: &Path) -> Option<PathBuf> {
    let mappe = sti.parent()?;
    let navn = sti.file_name()?.to_string_lossy().into_owned();
    let (grunn, teller) = skill_ut_teller(&navn);
    let mut kandidater = vec![grunn.clone()];
    if let Some(original) = uten_redigert(&grunn) {
        kandidater.push(original);
    }

    let jsonfiler: Vec<String> = fs::read_dir(mappe)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_lowercase().ends_with(".json"))
        .collect();

    for kandidat in &kandidater {
        let fullt = format!("{kandidat}.supplemental-metadata");
        let beste = jsonfiler
            .iter()
            .filter_map(|j| {
                let (stamme, j_teller) = skill_ut_teller(&j[..j.len() - 5]);
                // JSON-navnet er hele bildenavnet (ev. med tillegg), eller en start som Google har kuttet.
                let passer = j_teller == teller
                    && fullt.starts_with(&stamme)
                    && (stamme.len() >= kandidat.len() || stamme.chars().count() >= MIN_KUTTET);
                passer.then_some((stamme.len(), j))
            })
            .max_by_key(|(lengde, _)| *lengde);
        if let Some((_, j)) = beste {
            return Some(mappe.join(j));
        }
    }
    None
}

/// `IMG_1(2).jpg` → (`IMG_1.jpg`, Some(2)). `IMG_1.jpg(2)` → (`IMG_1.jpg`, Some(2)).
fn skill_ut_teller(navn: &str) -> (String, Option<u32>) {
    let finn = |s: &str| -> Option<(usize, usize, u32)> {
        let start = s.rfind('(')?;
        let slutt = start + s[start..].find(')')?;
        let tall = s[start + 1..slutt].parse().ok()?;
        Some((start, slutt, tall))
    };
    match finn(navn) {
        Some((start, slutt, tall)) => (format!("{}{}", &navn[..start], &navn[slutt + 1..]), Some(tall)),
        None => (navn.to_string(), None),
    }
}

/// `IMG_1-redigert.jpg` eller `IMG_1-edited.jpg` → `IMG_1.jpg`.
fn uten_redigert(navn: &str) -> Option<String> {
    let (stamme, ext) = navn.rsplit_once('.')?;
    for hale in ["-redigert", "-edited", "-bearbeitet", "-modifié", "-editado"] {
        if let Some(s) = stamme.strip_suffix(hale) {
            return Some(format!("{s}.{ext}"));
        }
    }
    None
}

#[cfg(test)]
mod tester {
    use super::*;

    fn mappe_med(filer: &[&str]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for f in filer {
            fs::write(d.path().join(f), b"{}").unwrap();
        }
        d
    }

    fn sidecar(d: &tempfile::TempDir, bilde: &str) -> Option<String> {
        finn_sidecar(&d.path().join(bilde)).map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
    }

    #[test]
    fn finner_vanlige_og_nye_navn() {
        let d = mappe_med(&["IMG_1.jpg.json", "IMG_2.jpg.supplemental-metadata.json", "metadata.json"]);
        assert_eq!(sidecar(&d, "IMG_1.jpg").as_deref(), Some("IMG_1.jpg.json"));
        assert_eq!(sidecar(&d, "IMG_2.jpg").as_deref(), Some("IMG_2.jpg.supplemental-metadata.json"));
        assert_eq!(sidecar(&d, "IMG_3.jpg"), None);
    }

    #[test]
    fn finner_avkuttede_navn() {
        let d = mappe_med(&[
            "20190704_120000_veldig_langt_filnavn_fra_kamera.j.json",
            "IMG_20200101_101010.jpg.supplemental-met.json",
        ]);
        assert_eq!(
            sidecar(&d, "20190704_120000_veldig_langt_filnavn_fra_kamera.jpg").as_deref(),
            Some("20190704_120000_veldig_langt_filnavn_fra_kamera.j.json")
        );
        assert_eq!(
            sidecar(&d, "IMG_20200101_101010.jpg").as_deref(),
            Some("IMG_20200101_101010.jpg.supplemental-met.json")
        );
    }

    #[test]
    fn finner_teller_og_redigerte() {
        let d = mappe_med(&["IMG_1.jpg.json", "IMG_1.jpg(1).json"]);
        assert_eq!(sidecar(&d, "IMG_1(1).jpg").as_deref(), Some("IMG_1.jpg(1).json"));
        assert_eq!(sidecar(&d, "IMG_1.jpg").as_deref(), Some("IMG_1.jpg.json"));
        assert_eq!(sidecar(&d, "IMG_1-redigert.jpg").as_deref(), Some("IMG_1.jpg.json"));
    }

    #[test]
    fn blander_ikke_like_starter() {
        let d = mappe_med(&["IMG_10.jpg.json"]);
        assert_eq!(sidecar(&d, "IMG_1.jpg"), None);
    }

    #[test]
    fn pakker_ut_og_fortsetter_uten_a_rore_zip() {
        use std::io::Write;
        let d = tempfile::tempdir().unwrap();
        let zipsti = d.path().join("takeout-20241022T101500Z-001.zip");
        let mut z = zip::ZipWriter::new(fs::File::create(&zipsti).unwrap());
        let valg = zip::write::SimpleFileOptions::default();
        for (navn, innhold) in [
            ("Takeout/Google Foto/Bilder fra 2019/IMG_1.jpg", &b"jpeg"[..]),
            ("Takeout/Google Foto/Bilder fra 2019/IMG_1.jpg.json", b"{}"),
            ("Takeout/Google Foto/arkiv_nettleser.html", b"<html>"),
            ("../utenfor.jpg", b"ondsinnet"),
        ] {
            z.start_file(navn, valg).unwrap();
            z.write_all(innhold).unwrap();
        }
        z.finish().unwrap();
        let foer = fs::read(&zipsti).unwrap();

        let mal = d.path().join("samling");
        let (kilder, varsler) = forbered_kilder(&[d.path().to_path_buf()], &mal, &|_, _| {});
        assert!(varsler.is_empty());
        assert_eq!(kilder.len(), 2);
        assert!(kilder[1].ends_with("Takeout/Google Foto"), "{:?}", kilder[1]);
        let ut = mal.join(".cloudjumper/takeout/takeout-20241022T101500Z-001/Takeout/Google Foto/Bilder fra 2019");
        assert_eq!(fs::read(ut.join("IMG_1.jpg")).unwrap(), b"jpeg");
        assert!(ut.join("IMG_1.jpg.json").exists());
        assert!(!ut.join("arkiv_nettleser.html").exists());
        assert!(!d.path().join("utenfor.jpg").exists());
        // Andre gang hoppes ferdige filer over, og zip-en er uendret.
        assert_eq!(pakk_ut(&zipsti, &mal.join(".cloudjumper/takeout/takeout-20241022T101500Z-001")).unwrap(), 2);
        assert_eq!(fs::read(&zipsti).unwrap(), foer);
    }

    #[test]
    fn tar_bare_google_foto_fra_takeout() {
        assert!(fra_google_foto(Path::new("Takeout/Google Foto/Bilder fra 2019/a.jpg")));
        assert!(fra_google_foto(Path::new("Takeout/Google Photos/Photos from 2019/a.jpg")));
        assert!(!fra_google_foto(Path::new("Takeout/Disk/Mine filer/a.jpg")));
        assert!(!fra_google_foto(Path::new("Takeout/Kontakter/Alle kontakter/Ola.jpg")));
        assert!(fra_google_foto(Path::new("Bilder/a.jpg")));
    }

    #[test]
    fn venter_paa_nedlastinger_og_hopper_over_ekstra_kopier() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        fs::write(p.join("takeout-1-001.zip"), b"").unwrap();
        fs::write(p.join("takeout-1-001.ogHNjvLq.zip.part"), b"halv").unwrap();
        fs::write(p.join("takeout-2-001.zip"), b"PK ikke ferdig").unwrap();
        fs::write(p.join("takeout-2-001.zip.crdownload"), b"halv").unwrap();
        fs::write(p.join("takeout-3-001.zip"), b"PK").unwrap();
        fs::write(p.join("takeout-3-001(1).zip"), b"PK").unwrap();
        fs::write(p.join("takeout-3-001 (2).zip"), b"PK annen").unwrap();
        assert!(lastes_ned(&p.join("takeout-1-001.zip")));
        assert!(lastes_ned(&p.join("takeout-2-001.zip")));
        assert!(!lastes_ned(&p.join("takeout-3-001.zip")));
        assert!(er_ekstra_nedlasting(&p.join("takeout-3-001(1).zip")));
        assert!(!er_ekstra_nedlasting(&p.join("takeout-3-001 (2).zip")));
        assert!(!er_ekstra_nedlasting(&p.join("takeout-3-001.zip")));
    }

    #[test]
    fn halv_nedlasting_gir_varsel() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("takeout-1.zip"), b"PK ikke ferdig").unwrap();
        let (kilder, varsler) = forbered_kilder(&[d.path().to_path_buf()], &d.path().join("m"), &|_, _| {});
        assert_eq!(kilder.len(), 1);
        assert_eq!(varsler.len(), 1);
    }

    #[test]
    fn leser_dato_fra_json() {
        let d = tempfile::tempdir().unwrap();
        fs::write(
            d.path().join("a.jpg.json"),
            r#"{"title":"a.jpg","photoTakenTime":{"timestamp":"1562241600","formatted":"4. juli 2019"}}"#,
        )
        .unwrap();
        let dato = sidecar_dato(&d.path().join("a.jpg")).unwrap();
        let forventet = DateTime::from_timestamp(1562241600, 0).unwrap().with_timezone(&Local).naive_local();
        assert_eq!(dato, forventet);
    }
}
