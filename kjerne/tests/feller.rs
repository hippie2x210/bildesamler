//! Feller andre bildeprogrammer har gått i: Live Photos som skilles, og redigerte bilder som blir «duplikater».

use cloudjumper_kjerne::{lag_plan, plan::Rolle};
use std::fs;

fn bilde(sti: &std::path::Path, lys: u8) {
    image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([lys, (x * 4) as u8, (y * 5) as u8])).save(sti).unwrap();
}

#[test]
fn live_photo_holdes_sammen_med_bildet() {
    let d = tempfile::tempdir().unwrap();
    let inn = d.path().join("Mobil");
    fs::create_dir_all(&inn).unwrap();
    bilde(&inn.join("IMG_20190704_120000.jpg"), 100);
    fs::write(inn.join("IMG_20190704_120000.MOV"), b"ikke en ekte video").unwrap();
    let plan = lag_plan(&[inn], &d.path().join("Samling"), &|_, _| {});
    assert_eq!(plan.handlinger.len(), 2);
    for h in &plan.handlinger {
        assert!(h.til.starts_with("Bilder/2019/07"), "{:?}", h.til);
    }
}

#[test]
fn redigert_bilde_er_ikke_duplikat() {
    let d = tempfile::tempdir().unwrap();
    let inn = d.path().join("Google");
    fs::create_dir_all(&inn).unwrap();
    bilde(&inn.join("IMG_20190704_120000.jpg"), 100);
    bilde(&inn.join("IMG_20190704_120000-redigert.jpg"), 110);
    let plan = lag_plan(&[inn], &d.path().join("Samling"), &|_, _| {});
    assert!(plan.grupper.is_empty());
    assert!(plan.handlinger.iter().all(|h| h.rolle == Rolle::Original));
}
