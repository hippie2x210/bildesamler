//! Hele veien for en Google Takeout-zip: utpakking, dato fra JSON, sortering og albumkopier.

use cloudjumper_kjerne::{lag_plan, modell::Datokilde, plan::Rolle, takeout, utfor};
use std::fs;
use std::io::{Cursor, Write};

fn jpeg(farge: u8) -> Vec<u8> {
    let bilde = image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([farge, (x * 4) as u8, (y * 5) as u8]));
    let mut ut = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(bilde).write_to(&mut ut, image::ImageFormat::Jpeg).unwrap();
    ut.into_inner()
}

#[test]
fn takeout_zip_sorteres_med_googles_dato() {
    let d = tempfile::tempdir().unwrap();
    let nedlastet = d.path().join("Nedlastinger");
    fs::create_dir_all(&nedlastet).unwrap();
    let ferie = jpeg(200);
    // 4. juli 2019 12:00 UTC
    let json = r#"{"title":"ferie.jpg","photoTakenTime":{"timestamp":"1562241600"}}"#;

    let mut z = zip::ZipWriter::new(fs::File::create(nedlastet.join("takeout-20261022T120000Z-001.zip")).unwrap());
    let valg = zip::write::SimpleFileOptions::default();
    for (navn, innhold) in [
        ("Takeout/Google Foto/Photos from 2019/ferie.jpg", ferie.as_slice()),
        ("Takeout/Google Foto/Photos from 2019/ferie.jpg.supplemental-metadata.json", json.as_bytes()),
        ("Takeout/Google Foto/Sommer på hytta/ferie.jpg", ferie.as_slice()),
        ("Takeout/Google Foto/Sommer på hytta/ferie.jpg.supplemental-metadata.json", json.as_bytes()),
        ("Takeout/Google Foto/Sommer på hytta/metadata.json", br#"{"title":"Sommer"}"#.as_slice()),
    ] {
        z.start_file(navn, valg).unwrap();
        z.write_all(innhold).unwrap();
    }
    z.finish().unwrap();

    let mal = d.path().join("Samling");
    let (kilder, varsler) = takeout::forbered_kilder(&[nedlastet.clone()], &mal, &|_, _| {});
    assert!(varsler.is_empty(), "{varsler:?}");
    let plan = lag_plan(&kilder, &mal, &|_, _| {});

    assert_eq!(plan.rapport.filer, 2);
    assert_eq!(plan.rapport.takeout_albumkopier, 1);
    assert_eq!(plan.rapport.bilder, 1);
    assert_eq!(plan.handlinger.len(), 1);
    let h = &plan.handlinger[0];
    assert_eq!(h.rolle, Rolle::Original);
    assert!(h.til.starts_with("Bilder/2019/07"), "{:?}", h.til);
    assert_eq!(plan.filer[h.fil].datokilde, Datokilde::Takeout);
}

#[test]
fn takeout_bilder_flyttes_og_pakkes_ikke_ut_igjen() {
    let d = tempfile::tempdir().unwrap();
    let nedlastet = d.path().join("Nedlastinger");
    fs::create_dir_all(&nedlastet).unwrap();
    let zipsti = nedlastet.join("takeout-20261022T120000Z-001.zip");
    let mut z = zip::ZipWriter::new(fs::File::create(&zipsti).unwrap());
    z.start_file("Takeout/Google Foto/Photos from 2019/IMG_20190704_120000.jpg", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(&jpeg(90)).unwrap();
    z.finish().unwrap();
    let zip_foer = fs::read(&zipsti).unwrap();

    let mal = d.path().join("Samling");
    let utpakket = mal.join(".cloudjumper/takeout/takeout-20261022T120000Z-001/Takeout/Google Foto/Photos from 2019/IMG_20190704_120000.jpg");
    let (kilder, _) = takeout::forbered_kilder(&[nedlastet.clone()], &mal, &|_, _| {});
    let plan = lag_plan(&kilder, &mal, &|_, _| {});
    assert!(utpakket.exists());
    assert_eq!(utfor::utfor(&plan, &|_, _| {}).unwrap().kopiert, 1);

    // Bildet ligger én gang i samlingen, ikke også i den utpakkede mappen. Zip-en er urørt.
    let i_samlingen = mal.join("Bilder/2019/07/IMG_20190704_120000.jpg");
    assert!(i_samlingen.exists());
    assert!(!utpakket.exists());
    assert_eq!(fs::read(&zipsti).unwrap(), zip_foer);

    // Neste skanning pakker ikke ut på nytt og finner ingenting nytt.
    let (kilder, _) = takeout::forbered_kilder(&[nedlastet], &mal, &|_, _| {});
    assert!(!utpakket.exists());
    assert!(lag_plan(&kilder, &mal, &|_, _| {}).handlinger.is_empty());

    // Angre legger bildet tilbake i den utpakkede mappen.
    assert_eq!(utfor::angre_siste(&mal).unwrap(), 1);
    assert!(utpakket.exists());
    assert!(!i_samlingen.exists());
}
