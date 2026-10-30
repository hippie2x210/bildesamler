//! Dropbox sin `content_hash`: SHA-256 av hver blokk på 4 MB, og så SHA-256 av alle blokk-summene etter hverandre.
//! <https://www.dropbox.com/developers/reference/content-hash>

use anyhow::Result;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

const BLOKK: usize = 4 * 1024 * 1024;

pub fn for_leser(mut r: impl Read) -> Result<String> {
    let mut total = Sha256::new();
    let mut buf = vec![0u8; BLOKK];
    loop {
        // Fyll hele blokken før den summeres; `read` kan gi færre byte enn bedt om.
        let mut fylt = 0;
        while fylt < BLOKK {
            let n = r.read(&mut buf[fylt..])?;
            if n == 0 {
                break;
            }
            fylt += n;
        }
        if fylt == 0 {
            break;
        }
        total.update(Sha256::digest(&buf[..fylt]));
        if fylt < BLOKK {
            break;
        }
    }
    Ok(total.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn for_fil(sti: &Path) -> Result<String> {
    for_leser(std::fs::File::open(sti)?)
}

#[cfg(test)]
mod tester {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn tom_fil() {
        // Ingen blokker: summen av ingenting.
        assert_eq!(for_leser(&b""[..]).unwrap(), hex(&Sha256::digest(b"")));
    }

    #[test]
    fn deler_i_blokker_paa_4_mb() {
        let data: Vec<u8> = (0..BLOKK + 10).map(|i| (i % 251) as u8).collect();
        let mut forventet = Sha256::new();
        forventet.update(Sha256::digest(&data[..BLOKK]));
        forventet.update(Sha256::digest(&data[BLOKK..]));
        assert_eq!(for_leser(&data[..]).unwrap(), hex(&forventet.finalize()));
        // Nøyaktig én blokk gir ikke en ekstra tom blokk.
        let en = &data[..BLOKK];
        assert_eq!(for_leser(en).unwrap(), hex(&Sha256::digest(Sha256::digest(en))));
    }
}
