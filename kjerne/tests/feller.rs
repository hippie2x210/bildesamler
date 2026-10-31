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

#[test]
fn miniatyrer_legges_for_seg_og_skjulte_mapper_hoppes_over() {
    let d = tempfile::tempdir().unwrap();
    let kilde = d.path().join("Mobil");
    let mal = d.path().join("Samling");
    let lag = |sti: &std::path::Path, bredde: u32| {
        std::fs::create_dir_all(sti.parent().unwrap()).unwrap();
        image::RgbImage::from_fn(bredde, bredde * 3 / 4, |x, y| {
            let (x, y) = (x * 64 / bredde, y * 48 / (bredde * 3 / 4));
            image::Rgb([(x * 4) as u8, (y * 5) as u8, ((x + y) * 2) as u8])
        })
        .save(sti)
        .unwrap();
    };
    lag(&kilde.join("Camera/IMG_20190704_120000.jpg"), 1600);
    // En miniatyr et program har lagt ved siden av, og en i telefonens skjulte mappe.
    lag(&kilde.join("Camera/thumb/IMG_20190704_120000.jpg"), 320);
    lag(&kilde.join(".thumbnails/1234.jpg"), 200);
    let plan = cloudjumper_kjerne::lag_plan(&[kilde], &mal, &|_, _| {});
    assert_eq!(plan.filer.len(), 2, "skjulte mapper skal ikke leses");
    assert_eq!(plan.rapport.miniatyrer, 1);
    let mini = plan.handlinger.iter().find(|h| h.rolle == cloudjumper_kjerne::plan::Rolle::Duplikat).unwrap();
    assert!(mini.til.starts_with("Duplikater/Miniatyrbilder/2019/07"), "{:?}", mini.til);
}
