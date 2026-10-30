//! Flere kjøringer mot samme samling: det som finnes fra før, skal ikke havne i `Bilder` en gang til.

use cloudjumper_kjerne::{lag_plan, plan::Rolle, utfor};
use std::fs;
use std::path::Path;

fn bilde(sti: &Path, bredde: u32, kvalitet: u8) {
    fs::create_dir_all(sti.parent().unwrap()).unwrap();
    let b = image::RgbImage::from_fn(bredde, bredde * 3 / 4, |x, y| {
        let (x, y) = (x * 64 / bredde, y * 48 / (bredde * 3 / 4));
        image::Rgb([(x * 4) as u8, (y * 5) as u8, ((x + y) * 2) as u8])
    });
    let mut ut = fs::File::create(sti).unwrap();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut ut, kvalitet).encode_image(&b).unwrap();
}

#[test]
fn ny_kilde_sammenlignes_med_samlingen() {
    let d = tempfile::tempdir().unwrap();
    let samling = d.path().join("Samling");

    // Første kjøring: bildet fra mobilen.
    let mobil = d.path().join("Mobil");
    bilde(&mobil.join("IMG_20190704_120000.jpg"), 640, 95);
    let plan = lag_plan(&[mobil.clone()], &samling, &|_, _| {});
    assert_eq!(utfor::utfor(&plan, &|_, _| {}).unwrap().kopiert, 1);

    // Andre kjøring: samme bilde fra WhatsApp (mindre og hardere komprimert), og mobilen en gang til.
    let whatsapp = d.path().join("WhatsApp");
    bilde(&whatsapp.join("IMG-20190704-WA0001.jpg"), 320, 60);
    let plan = lag_plan(&[whatsapp, mobil], &samling, &|_, _| {});
    assert_eq!(plan.rapport.filer, 2);
    assert_eq!(plan.rapport.allerede_i_samlingen, 1);
    assert_eq!(plan.handlinger.len(), 1, "{:?}", plan.handlinger);
    let h = &plan.handlinger[0];
    assert_eq!(h.rolle, Rolle::Duplikat);
    assert!(h.til.starts_with("Duplikater/2019/07"), "{:?}", h.til);

    let res = utfor::utfor(&plan, &|_, _| {}).unwrap();
    assert_eq!(res.kopiert, 1);
    assert_eq!(fs::read_dir(samling.join("Bilder/2019/07")).unwrap().count(), 1);

    // Tredje kjøring: ingenting nytt.
    let plan = lag_plan(&[d.path().join("WhatsApp"), d.path().join("Mobil")], &samling, &|_, _| {});
    assert!(plan.handlinger.is_empty(), "{:?}", plan.handlinger);
    assert_eq!(plan.rapport.allerede_i_samlingen, 2);
}

#[test]
fn kopi_med_dato_slar_bildet_i_uten_dato() {
    let d = tempfile::tempdir().unwrap();
    let samling = d.path().join("Samling");
    bilde(&samling.join("Uten dato/bilde1.jpg"), 320, 60);

    let mobil = d.path().join("Mobil");
    bilde(&mobil.join("IMG_20190704_120000.jpg"), 640, 95);
    let plan = lag_plan(&[mobil], &samling, &|_, _| {});
    assert_eq!(plan.handlinger.len(), 1);
    assert_eq!(plan.handlinger[0].rolle, Rolle::Original);
    assert!(plan.handlinger[0].til.starts_with("Bilder/2019/07"));
    // Bildet i samlingen blir liggende.
    assert!(samling.join("Uten dato/bilde1.jpg").exists());
}

#[test]
fn kopiering_overskriver_aldri() {
    let d = tempfile::tempdir().unwrap();
    let samling = d.path().join("Samling");
    let mobil = d.path().join("Mobil");
    bilde(&mobil.join("IMG_20190704_120000.jpg"), 640, 95);
    let plan = lag_plan(&[mobil], &samling, &|_, _| {});

    // En annen fil med samme navn dukker opp etter skanningen.
    let fremmed = samling.join("Bilder/2019/07/IMG_20190704_120000.jpg");
    fs::create_dir_all(fremmed.parent().unwrap()).unwrap();
    fs::write(&fremmed, b"brukerens egen fil").unwrap();

    let res = utfor::utfor(&plan, &|_, _| {}).unwrap();
    assert_eq!(res.kopiert, 1);
    assert_eq!(fs::read(&fremmed).unwrap(), b"brukerens egen fil");
    assert!(samling.join("Bilder/2019/07/IMG_20190704_120000 (2).jpg").exists());
    // Ingen plassholdere eller halve filer igjen.
    let rester: Vec<_> = fs::read_dir(samling.join("Bilder/2019/07"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with("cloudjumper-del"))
        .collect();
    assert!(rester.is_empty());
}

#[test]
fn tom_plassholder_fra_avbrutt_kjoring_brukes_igjen() {
    let d = tempfile::tempdir().unwrap();
    let samling = d.path().join("Samling");
    let mobil = d.path().join("Mobil");
    bilde(&mobil.join("IMG_20190704_120000.jpg"), 640, 95);
    let plan = lag_plan(&[mobil.clone()], &samling, &|_, _| {});

    // Appen krasjet etter at navnet var reservert.
    let plassholder = samling.join("Bilder/2019/07/IMG_20190704_120000.jpg");
    fs::create_dir_all(plassholder.parent().unwrap()).unwrap();
    fs::write(&plassholder, b"").unwrap();

    assert_eq!(utfor::utfor(&plan, &|_, _| {}).unwrap().kopiert, 1);
    assert_eq!(fs::read(&plassholder).unwrap(), fs::read(mobil.join("IMG_20190704_120000.jpg")).unwrap());
    assert_eq!(fs::read_dir(samling.join("Bilder/2019/07")).unwrap().count(), 1);
}

/// JPEG med et EXIF-merke om at bildet skal snus 90° med klokka (Orientation = 6), slik mobiler lagrer stående bilder.
fn med_retning(b: &image::RgbImage, sti: &Path) {
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95).encode_image(b).unwrap();
    // TIFF (little endian) med én IFD og ett felt: 0x0112 SHORT 1 = 6.
    let mut tiff = b"II*\0\x08\0\0\0\x01\0".to_vec();
    tiff.extend([0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0]);
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend(tiff);
    let mut ut = jpeg[..2].to_vec();
    ut.extend([0xFF, 0xE1]);
    ut.extend(((app1.len() + 2) as u16).to_be_bytes());
    ut.extend(app1);
    ut.extend(&jpeg[2..]);
    fs::create_dir_all(sti.parent().unwrap()).unwrap();
    fs::write(sti, ut).unwrap();
}

#[test]
fn snudd_kopi_er_samme_bilde() {
    let d = tempfile::tempdir().unwrap();
    let lagret = image::RgbImage::from_fn(640, 480, |x, y| {
        let (x, y) = (x / 10, y / 10);
        image::Rgb([(x * 4) as u8, (y * 5) as u8, ((x * y) % 255) as u8])
    });
    // Originalen fra mobilen er lagret liggende med et merke om å snus. Kopien fra Google er ferdig snudd og mindre.
    med_retning(&lagret, &d.path().join("Mobil/IMG_20190704_120000.jpg"));
    let snudd = image::imageops::rotate90(&lagret);
    let liten = image::imageops::resize(&snudd, 240, 320, image::imageops::FilterType::Triangle);
    fs::create_dir_all(d.path().join("Google")).unwrap();
    liten.save(d.path().join("Google/IMG_20190704_120000.jpg")).unwrap();

    let plan = lag_plan(&[d.path().join("Mobil"), d.path().join("Google")], &d.path().join("Samling"), &|_, _| {});
    assert_eq!(plan.grupper.len(), 1, "den snudde kopien ble ikke gjenkjent");
    let original = &plan.filer[plan.grupper[0].original];
    assert!(original.sti.starts_with(d.path().join("Mobil")));
    assert_eq!((original.bredde, original.hoyde), (480, 640));
}

#[test]
fn kopien_far_bildets_dato() {
    let d = tempfile::tempdir().unwrap();
    // Som en fil lastet ned fra skyen i dag: endringstiden er nå, men filnavnet har datoen.
    bilde(&d.path().join("OneDrive/IMG_20190704_120000.jpg"), 320, 90);
    let samling = d.path().join("Samling");
    let plan = lag_plan(&[d.path().join("OneDrive")], &samling, &|_, _| {});
    utfor::utfor(&plan, &|_, _| {}).unwrap();
    let endret = fs::metadata(samling.join("Bilder/2019/07/IMG_20190704_120000.jpg")).unwrap().modified().unwrap();
    let endret = chrono::DateTime::<chrono::Local>::from(endret).naive_local();
    assert_eq!(endret.to_string(), "2019-07-04 12:00:00");
}
