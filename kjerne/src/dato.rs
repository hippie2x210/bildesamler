//! Velger den sikreste datoen for en fil og oppdager datoer som lyver.

use crate::modell::{Datokilde, Fil, Medietype, Tillit};
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, Timelike};
use regex::Regex;
use std::sync::OnceLock;

/// Eldre enn dette regner vi ikke med digitale bilder (skannede bilder får EXIF fra skanneren).
const FORSTE_AR: i32 = 1990;

/// Leser dato fra filnavn som `IMG_20190704_120000`, `IMG-20190704-WA0001`,
/// `Screenshot_20200310-143005` eller `2019-07-04 12.00.00`.
pub fn fra_filnavn(navn: &str) -> Option<NaiveDateTime> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?:^|[^0-9])((?:19|20)\d{2})[-_.]?(\d{2})[-_.]?(\d{2})(?:[-_ T.]?(\d{2})[-_.:]?(\d{2})[-_.:]?(\d{2}))?(?:[^0-9]|$)",
        )
        .unwrap()
    });
    let c = re.captures(navn)?;
    let tall = |i: usize| c.get(i).map(|m| m.as_str().parse::<u32>().unwrap_or(99));
    let dato = NaiveDate::from_ymd_opt(tall(1)? as i32, tall(2)?, tall(3)?)?;
    let tid = match (tall(4), tall(5), tall(6)) {
        (Some(t), Some(m), Some(s)) => dato.and_hms_opt(t, m, s),
        _ => dato.and_hms_opt(12, 0, 0),
    }?;
    sannsynlig(tid).then_some(tid)
}

/// Er datoen mulig for et digitalt bilde? Avviser fremtiden, tiden før digitalkamera
/// og standarddatoer fra kameraer som har mistet klokka.
pub fn sannsynlig(d: NaiveDateTime) -> bool {
    let i_morgen = Local::now().naive_local() + chrono::Duration::days(1);
    if d.year() < FORSTE_AR || d > i_morgen {
        return false;
    }
    let nullstilt = d.month() == 1 && d.day() == 1 && d.hour() == 0 && d.minute() == 0;
    !(nullstilt && matches!(d.year(), 2000 | 2001 | 2002 | 2003 | 2004 | 2005 | 2010))
}

/// Fyller inn `dato`, `datokilde`, `tillit` og `advarsler` ut fra det som er lest.
pub fn velg(fil: &mut Fil) {
    if let Some(d) = fil.metadata_dato {
        if !sannsynlig(d) {
            fil.advarsler.push(format!("Datoen i metadata ({d}) er umulig og brukes ikke"));
            fil.metadata_dato = None;
        }
    }

    let (dato, kilde, tillit) = match (fil.metadata_dato, fil.filnavn_dato) {
        (Some(m), f) => {
            if let Some(f) = f {
                if (m - f).num_hours().abs() > 24 {
                    fil.advarsler.push(format!(
                        "Datoen i metadata ({}) og i filnavnet ({}) er uenige",
                        m.date(),
                        f.date()
                    ));
                }
            }
            let kilde = match fil.medietype {
                Medietype::Bilde => Datokilde::Exif,
                Medietype::Video => Datokilde::Video,
            };
            (Some(m), kilde, Tillit::Hoy)
        }
        (None, f) if fil.takeout_dato.is_some_and(sannsynlig) => {
            let t = fil.takeout_dato.unwrap();
            match f {
                // Uten EXIF er Googles dato ofte opplastingstiden. Da stoler vi mer på filnavnet.
                Some(f) if (t - f).num_hours().abs() > 24 => {
                    fil.advarsler.push(format!(
                        "Google Foto sier {}, men filnavnet sier {}. Bruker filnavnet",
                        t.date(),
                        f.date()
                    ));
                    (Some(f), Datokilde::Filnavn, Tillit::Middels)
                }
                _ => (Some(t), Datokilde::Takeout, Tillit::Middels),
            }
        }
        (None, Some(f)) => (Some(f), Datokilde::Filnavn, Tillit::Middels),
        (None, None) => {
            if let Some(f) = fil.fildato {
                fil.advarsler.push(format!(
                    "Ingen sikker dato. Fildatoen er {}, men den kan være nedlastingsdatoen",
                    f.date()
                ));
            }
            (None, Datokilde::Ingen, Tillit::Ingen)
        }
    };
    fil.dato = dato;
    fil.datokilde = kilde;
    fil.tillit = tillit;
}

#[cfg(test)]
mod tester {
    use super::*;

    fn d(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn leser_vanlige_filnavn() {
        assert_eq!(fra_filnavn("IMG_20190704_120501.jpg"), Some(d("2019-07-04 12:05:01")));
        assert_eq!(fra_filnavn("IMG-20190704-WA0001.jpg"), Some(d("2019-07-04 12:00:00")));
        assert_eq!(fra_filnavn("Screenshot_20200310-143005.png"), Some(d("2020-03-10 14:30:05")));
        assert_eq!(fra_filnavn("2019-07-04 12.00.00.jpg"), Some(d("2019-07-04 12:00:00")));
        assert_eq!(fra_filnavn("VID_20230317_084500.mp4"), Some(d("2023-03-17 08:45:00")));
    }

    #[test]
    fn ignorerer_tall_som_ikke_er_datoer() {
        assert_eq!(fra_filnavn("IMG_1000.JPG"), None);
        assert_eq!(fra_filnavn("DSC_9000.JPG"), None);
        assert_eq!(fra_filnavn("bilde_20191340.jpg"), None);
        assert_eq!(fra_filnavn("P1234567890123.jpg"), None);
    }

    fn fil(metadata: Option<&str>, filnavn: Option<&str>, takeout: Option<&str>) -> Fil {
        Fil {
            sti: "x.jpg".into(),
            storrelse: 0,
            sha256: String::new(),
            phash: None,
            bredde: 0,
            hoyde: 0,
            medietype: Medietype::Bilde,
            kamera: None,
            har_gps: false,
            metadata_dato: metadata.map(d),
            filnavn_dato: filnavn.map(d),
            takeout_dato: takeout.map(d),
            fildato: None,
            dato: None,
            datokilde: Datokilde::Ingen,
            tillit: Tillit::Ingen,
            advarsler: Vec::new(),
            i_samlingen: false,
        }
    }

    #[test]
    fn takeout_dato_brukes_uten_exif() {
        let mut f = fil(None, None, Some("2018-05-17 10:00:00"));
        velg(&mut f);
        assert_eq!((f.dato, f.datokilde), (Some(d("2018-05-17 10:00:00")), Datokilde::Takeout));

        let mut f = fil(Some("2017-01-02 03:04:05"), None, Some("2018-05-17 10:00:00"));
        velg(&mut f);
        assert_eq!(f.datokilde, Datokilde::Exif);

        // WhatsApp-bilde lastet opp senere: filnavnet vinner, med advarsel.
        let mut f = fil(None, Some("2016-07-04 12:00:00"), Some("2021-03-01 09:00:00"));
        velg(&mut f);
        assert_eq!((f.dato, f.datokilde), (Some(d("2016-07-04 12:00:00")), Datokilde::Filnavn));
        assert_eq!(f.advarsler.len(), 1);
    }

    #[test]
    fn avviser_umulige_datoer() {
        assert!(!sannsynlig(d("2000-01-01 00:00:00")));
        assert!(!sannsynlig(d("1970-01-01 00:00:00")));
        assert!(!sannsynlig(d("2099-05-05 12:00:00")));
        assert!(sannsynlig(d("2000-01-01 14:23:10")));
        assert!(sannsynlig(d("2016-12-24 18:00:00")));
    }
}
