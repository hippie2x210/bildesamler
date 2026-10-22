//! Leser én fil: sjekksum, EXIF, størrelse, perseptuell hash og videodato.

use crate::dato;
use crate::modell::{Datokilde, Fil, Medietype, Tillit};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;

pub const BILDETYPER: &[&str] = &[
    "jpg", "jpeg", "png", "heic", "heif", "webp", "gif", "bmp", "tif", "tiff", "dng", "cr2", "cr3",
    "nef", "arw", "orf", "rw2", "raf",
];
pub const VIDEOTYPER: &[&str] = &["mp4", "mov", "m4v", "3gp", "avi", "mkv", "mts", "m2ts", "wmv"];

pub fn medietype(sti: &Path) -> Option<Medietype> {
    let ext = sti.extension()?.to_string_lossy().to_lowercase();
    if BILDETYPER.contains(&ext.as_str()) {
        Some(Medietype::Bilde)
    } else if VIDEOTYPER.contains(&ext.as_str()) {
        Some(Medietype::Video)
    } else {
        None
    }
}

pub fn les(sti: &Path) -> Result<Fil> {
    let medietype = medietype(sti).context("ikke et bilde eller en video")?;
    let meta = fs::metadata(sti)?;
    // Bilder leses helt inn (trengs for EXIF og hash). Videoer kan være mange GB og strømmes.
    let data = match medietype {
        Medietype::Bilde => fs::read(sti).with_context(|| format!("kunne ikke lese {}", sti.display()))?,
        Medietype::Video => Vec::new(),
    };
    let fildato = meta
        .modified()
        .ok()
        .map(|t| DateTime::<chrono::Local>::from(t).naive_local());

    let mut fil = Fil {
        sti: sti.to_path_buf(),
        storrelse: meta.len(),
        sha256: match medietype {
            Medietype::Bilde => hex(&Sha256::digest(&data)),
            Medietype::Video => strom_sha256(sti)?,
        },
        phash: None,
        bredde: 0,
        hoyde: 0,
        medietype,
        kamera: None,
        har_gps: false,
        metadata_dato: None,
        filnavn_dato: sti.file_name().and_then(|n| dato::fra_filnavn(&n.to_string_lossy())),
        takeout_dato: crate::takeout::sidecar_dato(sti),
        fildato,
        dato: None,
        datokilde: Datokilde::Ingen,
        tillit: Tillit::Ingen,
        advarsler: Vec::new(),
    };

    match medietype {
        Medietype::Bilde => {
            les_exif(&data, &mut fil);
            if let Ok(bilde) = image::load_from_memory(&data) {
                fil.bredde = bilde.width();
                fil.hoyde = bilde.height();
                fil.phash = Some(dhash(&bilde));
            }
        }
        Medietype::Video => fil.metadata_dato = mp4_dato(sti).ok().flatten(),
    }
    dato::velg(&mut fil);
    Ok(fil)
}

fn les_exif(data: &[u8], fil: &mut Fil) {
    let Ok(exif) = exif::Reader::new().read_from_container(&mut Cursor::new(data)) else {
        return;
    };
    let tekst = |tag| {
        exif.get_field(tag, exif::In::PRIMARY)
            .map(|f| f.display_value().to_string().trim_matches('"').trim().to_string())
            .filter(|s| !s.is_empty())
    };
    for tag in [exif::Tag::DateTimeOriginal, exif::Tag::DateTimeDigitized] {
        if let Some(f) = exif.get_field(tag, exif::In::PRIMARY) {
            if let exif::Value::Ascii(ref v) = f.value {
                if let Some(d) = v.first().and_then(|b| exif::DateTime::from_ascii(b).ok()) {
                    fil.metadata_dato = chrono::NaiveDate::from_ymd_opt(d.year as i32, d.month as u32, d.day as u32)
                        .and_then(|dt| dt.and_hms_opt(d.hour as u32, d.minute as u32, d.second as u32));
                    if fil.metadata_dato.is_some() {
                        break;
                    }
                }
            }
        }
    }
    let kamera = [tekst(exif::Tag::Make), tekst(exif::Tag::Model)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    if !kamera.is_empty() {
        fil.kamera = Some(kamera);
    }
    fil.har_gps = exif.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY).is_some();
}

/// dHash på 16x16: sammenligner nabopiksler i et nedskalert gråtonebilde.
/// Tåler ny størrelse og komprimering, men skiller ulike motiver.
pub fn dhash(bilde: &image::DynamicImage) -> [u64; 4] {
    let liten = bilde
        .resize_exact(17, 16, image::imageops::FilterType::Triangle)
        .to_luma8();
    let mut h = [0u64; 4];
    for y in 0..16u32 {
        for x in 0..16u32 {
            if liten.get_pixel(x, y)[0] < liten.get_pixel(x + 1, y)[0] {
                let bit = (y * 16 + x) as usize;
                h[bit / 64] |= 1 << (bit % 64);
            }
        }
    }
    h
}

pub fn avstand(a: &[u64; 4], b: &[u64; 4]) -> u32 {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
}

fn strom_sha256(sti: &Path) -> Result<String> {
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
    Ok(hex(&h.finalize()))
}

/// Finner `moov`-boksen i en MP4/MOV uten å lese hele filen (den kan ligge til slutt).
fn mp4_dato(sti: &Path) -> Result<Option<NaiveDateTime>> {
    let mut f = fs::File::open(sti)?;
    let lengde = f.metadata()?.len();
    let mut pos = 0u64;
    while pos + 8 <= lengde {
        f.seek(SeekFrom::Start(pos))?;
        let mut hode = [0u8; 16];
        f.read_exact(&mut hode[..8])?;
        let mut storrelse = u32::from_be_bytes(hode[..4].try_into()?) as u64;
        let mut hodelengde = 8;
        if storrelse == 1 {
            f.read_exact(&mut hode[8..16])?;
            storrelse = u64::from_be_bytes(hode[8..16].try_into()?);
            hodelengde = 16;
        } else if storrelse == 0 {
            storrelse = lengde - pos;
        }
        if storrelse < hodelengde {
            return Ok(None);
        }
        if &hode[4..8] == b"moov" {
            let innhold = (storrelse - hodelengde).min(64 << 20) as usize;
            let mut moov = vec![0u8; innhold];
            f.read_exact(&mut moov)?;
            return Ok(mvhd_dato(&moov));
        }
        pos += storrelse;
    }
    Ok(None)
}

/// Opprettelsestid fra `mvhd` (sekunder siden 1904, UTC).
fn mvhd_dato(moov: &[u8]) -> Option<NaiveDateTime> {
    let mvhd = finn_boks(moov, b"mvhd")?;
    let versjon = *mvhd.first()?;
    let sek = if versjon == 1 {
        u64::from_be_bytes(mvhd.get(4..12)?.try_into().ok()?)
    } else {
        u32::from_be_bytes(mvhd.get(4..8)?.try_into().ok()?) as u64
    };
    const FRA_1904_TIL_1970: i64 = 2_082_844_800;
    if sek == 0 {
        return None;
    }
    DateTime::from_timestamp(sek as i64 - FRA_1904_TIL_1970, 0).map(|d| d.naive_utc())
}

fn finn_boks<'a>(mut data: &'a [u8], navn: &[u8; 4]) -> Option<&'a [u8]> {
    while data.len() >= 8 {
        let mut r = &data[..8];
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf).ok()?;
        let mut lengde = u32::from_be_bytes(buf) as u64;
        let typ = &data[4..8];
        let mut hode = 8usize;
        if lengde == 1 {
            lengde = u64::from_be_bytes(data.get(8..16)?.try_into().ok()?);
            hode = 16;
        } else if lengde == 0 {
            lengde = data.len() as u64;
        }
        let lengde = usize::try_from(lengde).ok()?.min(data.len());
        if lengde < hode {
            return None;
        }
        if typ == navn {
            return Some(&data[hode..lengde]);
        }
        data = &data[lengde..];
    }
    None
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
