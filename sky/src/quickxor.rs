//! `quickXorHash`, sjekksummen OneDrive oppgir for alle filer. Brukes til å bekrefte at en nedlasting er hel,
//! også når den er satt sammen av flere biter etter brudd på nettet.
//! Algoritmen er beskrevet av Microsoft: <https://learn.microsoft.com/onedrive/developer/code-snippets/quickxorhash>.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

const BREDDE: usize = 160;
const SKIFT: usize = 11;

#[derive(Default)]
pub struct QuickXor {
    data: [u64; 3],
    skift: usize,
    lengde: u64,
}

impl QuickXor {
    pub fn oppdater(&mut self, biter: &[u8]) {
        let mut celle = self.skift / 64;
        let mut forskyvning = self.skift % 64;
        for i in 0..biter.len().min(BREDDE) {
            let siste = celle == self.data.len() - 1;
            let bits = if siste { BREDDE % 64 } else { 64 };
            let mut x = 0u8;
            for j in (i..biter.len()).step_by(BREDDE) {
                x ^= biter[j];
            }
            if forskyvning <= bits - 8 {
                self.data[celle] ^= (x as u64) << forskyvning;
            } else {
                let neste = if siste { 0 } else { celle + 1 };
                self.data[celle] ^= (x as u64) << forskyvning;
                self.data[neste] ^= (x as u64) >> (bits - forskyvning);
            }
            forskyvning += SKIFT;
            while forskyvning >= bits {
                celle = if siste { 0 } else { celle + 1 };
                forskyvning -= bits;
            }
        }
        self.skift = (self.skift + SKIFT * (biter.len() % BREDDE)) % BREDDE;
        self.lengde += biter.len() as u64;
    }

    pub fn ferdig(&self) -> String {
        let mut ut = [0u8; 20];
        for (i, d) in self.data.iter().enumerate() {
            let b = d.to_le_bytes();
            let n = (20 - i * 8).min(8);
            ut[i * 8..i * 8 + n].copy_from_slice(&b[..n]);
        }
        for (i, b) in self.lengde.to_le_bytes().iter().enumerate() {
            ut[12 + i] ^= b;
        }
        STANDARD.encode(ut)
    }
}

/// quickXorHash for en fil på disken.
pub fn for_fil(sti: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(sti)?;
    let mut h = QuickXor::default();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(h.ferdig());
        }
        h.oppdater(&buf[..n]);
    }
}

#[cfg(test)]
mod tester {
    use super::*;

    fn hash(data: &[u8]) -> String {
        let mut h = QuickXor::default();
        h.oppdater(data);
        h.ferdig()
    }

    // Testvektorer fra rclone sin implementasjon, som er kontrollert mot OneDrive.
    #[test]
    fn kjente_verdier() {
        assert_eq!(hash(b""), "AAAAAAAAAAAAAAAAAAAAAAAAAAA=");
        assert_eq!(hash(&STANDARD.decode("Sg==").unwrap()), "SgAAAAAAAAAAAAAAAQAAAAAAAAA=");
        assert_eq!(hash(&STANDARD.decode("h490d57Pqz5q2rtT").unwrap()), "h3gEHe7giWeswgdq3MYupgAAAAA=");
        assert_eq!(hash(&STANDARD.decode("T6LYJIfDh81JrAK309H2JMJTXis=").unwrap()), "zBTHrspn3mEcohlJdIUAbjGNaNg=");
        // 256 byte: flere runder på 160 byte.
        let lang = STANDARD
            .decode(concat!(
                "qwGf2ESubE5jOUHHyc94ORczFYYbc2OmEzo+hBIyzJiNwAzC8PvJqtTzwkWkSslgHFGWQZR2BV5+uYTr",
                "YT7HVwRM40vqfj0dBgeDENyTenIOL1LHkjtDKoXEnQ0mXAHoJ8PjbNC93zi5TovVRXTNzfGEs5dpWVqx",
                "Uzb5lc7dwkyvOluBw482mQ4xrzYyIY1t+//OrNi1ObGXuUw2jBQOFfJVj2Y6BOyYmfB1y36eBxi3zxeG",
                "5d5NYjm2GSh6e08QMAwu3zrINcqIzLOuNIiGXBtl7DjKt7b5wqi4oFiRpZsCyx2smhSrdrtK/CkdU6nD",
                "N+34vSR/M8rZpWQdBE7a8g==",
            ))
            .unwrap();
        assert_eq!(hash(&lang), "WYT9JY3JIo/pEBp+tIM6Gt2nyTM=");
    }

    #[test]
    fn biter_gir_samme_svar() {
        let data: Vec<u8> = (0..5000u32).map(|i| (i * 7 + i / 3) as u8).collect();
        let mut h = QuickXor::default();
        for del in data.chunks(333) {
            h.oppdater(del);
        }
        assert_eq!(h.ferdig(), hash(&data));
    }
}
