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
