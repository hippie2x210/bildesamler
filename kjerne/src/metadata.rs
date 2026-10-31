//! Leser én fil: sjekksum, EXIF, størrelse, perseptuell hash og videodato.

use crate::dato;
use crate::modell::{Datokilde, Fil, Medietype, Tillit};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, TimeZone};
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
        i_samlingen: false,
    };

    match medietype {
        Medietype::Bilde => {
            let retning = les_exif(&data, &mut fil);
            if let Ok(mut bilde) = image::load_from_memory(&data) {
                // Snu bildet slik det vises. Google Foto og WhatsApp lagrer ofte kopien ferdig snudd, mens
                // originalen fra kameraet bare har et merke om hvilken vei den skal snus. Uten dette ble
                // de ikke gjenkjent som samme bilde, fordi både motivet og formatet var på tvers.
                if let Some(r) = retning {
                    bilde.apply_orientation(r);
                }
                fil.bredde = bilde.width();
                fil.hoyde = bilde.height();
                fil.phash = Some(dhash(&bilde));
            }
        }
        Medietype::Video => {
            let (apple, mvhd) = mp4_datoer(sti).unwrap_or_default();
            fil.metadata_dato = apple.or_else(|| mvhd.map(|utc| videotid(utc, fil.filnavn_dato)));
        }
    }
    dato::velg(&mut fil);
    Ok(fil)
}

/// Leser dato, kamera og GPS. Returnerer hvilken vei bildet skal snus for å vises riktig.
fn les_exif(data: &[u8], fil: &mut Fil) -> Option<image::metadata::Orientation> {
    let Ok(exif) = exif::Reader::new().read_from_container(&mut Cursor::new(data)) else {
        return None;
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
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
        .and_then(|v| image::metadata::Orientation::from_exif(v as u8))
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

/// QuickTime-standarden lagrer opprettelsestiden i UTC, og det gjør mobiler. Noen kameraer skriver
/// likevel lokal tid. Stemmer tiden minutt for minutt med filnavnet, er den allerede lokal.
pub fn videotid(utc: NaiveDateTime, filnavn: Option<NaiveDateTime>) -> NaiveDateTime {
    if filnavn.is_some_and(|f| (f - utc).num_seconds().abs() <= 120) {
        return utc;
    }
    chrono::Local.from_utc_datetime(&utc).naive_local()
}

/// Leser datoene i en MP4/MOV: Apples `com.apple.quicktime.creationdate` (lokal tid med sone, slik
/// iPhone skriver den) og `mvhd` (UTC). Finner `moov` uten å lese hele filen (den kan ligge til slutt).
fn mp4_datoer(sti: &Path) -> Result<(Option<NaiveDateTime>, Option<NaiveDateTime>)> {
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
            return Ok((None, None));
        }
        if &hode[4..8] == b"moov" {
            let innhold = (storrelse - hodelengde).min(64 << 20) as usize;
            let mut moov = vec![0u8; innhold];
            f.read_exact(&mut moov)?;
            return Ok((apple_dato(&moov), mvhd_dato(&moov)));
        }
        pos += storrelse;
    }
    Ok((None, None))
}

/// `moov/meta` med `keys` og `ilst`: finner verdien til `com.apple.quicktime.creationdate`,
/// for eksempel «2019-07-04T14:30:00+0200», og bruker klokkeslettet slik det står (lokal tid der bildet ble tatt).
fn apple_dato(moov: &[u8]) -> Option<NaiveDateTime> {
    let mut meta = finn_boks(moov, b"meta")?;
    // I ISO-filer har `meta` versjon og flagg først; i QuickTime-filer kommer boksene rett på.
    if meta.get(4..8) != Some(b"hdlr".as_slice()) && meta.get(4..8) != Some(b"keys".as_slice()) {
        meta = meta.get(4..)?;
    }
    let keys = finn_boks(meta, b"keys")?;
    let antall = u32::from_be_bytes(keys.get(4..8)?.try_into().ok()?) as usize;
    let mut rest = keys.get(8..)?;
    let mut nummer = None;
    for i in 1..=antall {
        let lengde = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?) as usize;
        if rest.get(8..lengde)? == b"com.apple.quicktime.creationdate" {
            nummer = Some(i as u32);
            break;
        }
        rest = rest.get(lengde..)?;
    }
    let nummer = nummer?;
    let mut ilst = finn_boks(meta, b"ilst")?;
    while ilst.len() >= 8 {
        let lengde = u32::from_be_bytes(ilst[..4].try_into().ok()?) as usize;
        let element = ilst.get(..lengde.max(8))?;
        if u32::from_be_bytes(element[4..8].try_into().ok()?) == nummer {
            let data = finn_boks(&element[8..], b"data")?;
            let tekst = std::str::from_utf8(data.get(8..)?).ok()?.trim();
            return DateTime::parse_from_str(tekst, "%Y-%m-%dT%H:%M:%S%z").ok().map(|d| d.naive_local());
        }
        ilst = ilst.get(lengde.max(8)..)?;
    }
    None
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

#[cfg(test)]
mod tester {
    use super::*;

    fn boks(navn: &[u8; 4], innhold: &[u8]) -> Vec<u8> {
        let mut b = ((innhold.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend_from_slice(navn);
        b.extend_from_slice(innhold);
        b
    }

    #[test]
    fn leser_apples_dato_med_sone() {
        let nokkel = b"com.apple.quicktime.creationdate";
        let mut keys = vec![0, 0, 0, 0, 0, 0, 0, 1];
        keys.extend(((nokkel.len() + 8) as u32).to_be_bytes());
        keys.extend(b"mdta");
        keys.extend(nokkel);
        let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0];
        data.extend(b"2019-07-04T23:30:00+0200");
        let element = {
            let innhold = boks(b"data", &data);
            let mut e = ((innhold.len() + 8) as u32).to_be_bytes().to_vec();
            e.extend(1u32.to_be_bytes());
            e.extend(innhold);
            e
        };
        let mut meta = boks(b"hdlr", &[0; 24]);
        meta.extend(boks(b"keys", &keys));
        meta.extend(boks(b"ilst", &element));
        let moov = boks(b"meta", &meta);
        let ventet = NaiveDateTime::parse_from_str("2019-07-04 23:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        assert_eq!(apple_dato(&moov), Some(ventet));
    }

    #[test]
    fn videotid_som_stemmer_med_filnavnet_er_lokal() {
        let t = NaiveDateTime::parse_from_str("2019-07-04 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        assert_eq!(videotid(t, Some(t + chrono::Duration::seconds(30))), t);
    }
}
