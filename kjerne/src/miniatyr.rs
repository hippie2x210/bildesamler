//! Små forhåndsvisninger av bilder. Å vise originalene direkte i listene fikk WebView2 til å pakke ut
//! hundrevis av bilder på 12 megapiksler, som spiste minne og gjorde vinduet tregt å flytte og minimere.

use anyhow::{Context, Result};
use image::{imageops::FilterType, metadata::Orientation, DynamicImage, ImageFormat};
use std::io::Cursor;
use std::path::Path;

/// Største side i pikslene vi lager. Rikelig for kortene (180×120) og listene (48×48).
pub const STORRELSE: u32 = 240;

/// Lager en liten JPEG av bildet. Bruker miniatyren kameraet har lagt i EXIF når den finnes, så vi
/// slipper å pakke ut hele bildet.
pub fn lag(sti: &Path) -> Result<Vec<u8>> {
    let data = std::fs::read(sti).with_context(|| format!("kunne ikke lese {}", sti.display()))?;
    let exif = exif::Reader::new().read_from_container(&mut Cursor::new(&data)).ok();
    let retning = exif
        .as_ref()
        .and_then(|e| e.get_field(exif::Tag::Orientation, exif::In::PRIMARY))
        .and_then(|f| f.value.get_uint(0))
        .and_then(|v| Orientation::from_exif(v as u8));

    let bilde = match exif.as_ref().and_then(innebygd) {
        Some(b) => b,
        None => image::load_from_memory(&data).context("kunne ikke lese bildet")?,
    };
    let mut bilde = bilde.resize(STORRELSE, STORRELSE, FilterType::Triangle);
    if let Some(r) = retning {
        bilde.apply_orientation(r);
    }
    let mut ut = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(bilde.to_rgb8()).write_to(&mut ut, ImageFormat::Jpeg)?;
    Ok(ut.into_inner())
}

/// Miniatyren i EXIF, hvis den er stor nok til å se skarp ut.
fn innebygd(exif: &exif::Exif) -> Option<DynamicImage> {
    let felt = |t| exif.get_field(t, exif::In::THUMBNAIL).and_then(|f| f.value.get_uint(0)).map(|v| v as usize);
    let start = felt(exif::Tag::JPEGInterchangeFormat)?;
    let lengde = felt(exif::Tag::JPEGInterchangeFormatLength)?;
    let bytes = exif.buf().get(start..start.checked_add(lengde)?)?;
    let bilde = image::load_from_memory_with_format(bytes, ImageFormat::Jpeg).ok()?;
    (bilde.width().max(bilde.height()) >= 160).then_some(bilde)
}

#[cfg(test)]
mod tester {
    use super::*;

    #[test]
    fn lager_liten_jpeg() {
        let d = tempfile::tempdir().unwrap();
        let sti = d.path().join("stort.png");
        image::RgbImage::from_pixel(1200, 800, image::Rgb([10, 200, 30])).save(&sti).unwrap();
        let mini = image::load_from_memory(&lag(&sti).unwrap()).unwrap();
        assert_eq!((mini.width(), mini.height()), (240, 160));
    }
}
