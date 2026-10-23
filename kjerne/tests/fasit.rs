//! Kjører hele flyten på testbildene i `testdata/` og sammenligner med `testdata/fasit.csv`.
//! Testsettet lages av `tools/lag_testbilder.py`.

use bildesamler_kjerne::{lag_plan, plan::Rolle, utfor};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../testdata")
}

#[derive(serde::Deserialize)]
struct Rad {
    fil: String,
    scenario: String,
    forventet_dato: String,
    #[serde(rename = "forventet_mål")]
    forventet_mal: String,
}

#[test]
fn testsettet_sorteres_som_fasiten() {
    let innboks = testdata().join("innboks");
    let mal = tempfile::tempdir().unwrap();
    let plan = lag_plan(&[innboks.clone()], mal.path(), &|_, _| {});
    assert!(plan.ulesbare.is_empty(), "ulesbare filer: {:?}", plan.ulesbare);

    let per_fil: HashMap<String, usize> = plan
        .handlinger
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let rel = plan.filer[h.fil].sti.strip_prefix(&innboks).unwrap();
            (rel.to_string_lossy().replace('\\', "/"), i)
        })
        .collect();

    let mut feil = Vec::new();
    let mut rader = 0;
    for rad in csv::Reader::from_path(testdata().join("fasit.csv")).unwrap().deserialize::<Rad>() {
        let rad = rad.unwrap();
        rader += 1;
        let Some(&i) = per_fil.get(&rad.fil) else {
            feil.push(format!("{}: ikke funnet", rad.fil));
            continue;
        };
        let h = &plan.handlinger[i];
        let f = &plan.filer[h.fil];
        let til = h.til.to_string_lossy().replace('\\', "/");
        if !til.starts_with(&rad.forventet_mal) {
            feil.push(format!("{} ({}): havnet i {til}, skulle til {}", rad.fil, rad.scenario, rad.forventet_mal));
        }
        if h.rolle != Rolle::Duplikat {
            let dato = f.dato.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()).unwrap_or_default();
            if dato != rad.forventet_dato {
                feil.push(format!("{} ({}): dato {dato:?}, forventet {:?}", rad.fil, rad.scenario, rad.forventet_dato));
            }
        }
    }
    assert_eq!(rader, plan.filer.len(), "fasiten og skanningen har ulikt antall filer");
    assert!(feil.is_empty(), "{} avvik fra fasiten:\n{}", feil.len(), feil.join("\n"));
}

#[test]
fn utfor_og_angre_sletter_ingenting() {
    let innboks = testdata().join("innboks");
    let mal = tempfile::tempdir().unwrap();
    let plan = lag_plan(&[innboks.clone()], mal.path(), &|_, _| {});
    let antall_kilder = bildesamler_kjerne::finn_filer(&[innboks.clone()], mal.path()).len();

    let res = utfor::utfor(&plan, &|_, _| {}).unwrap();
    assert!(res.feil.is_empty(), "{:?}", res.feil);
    assert_eq!(res.kopiert, plan.handlinger.len());
    for h in &plan.handlinger {
        assert!(mal.path().join(&h.til).exists(), "mangler {}", h.til.display());
    }

    // En ny kjøring skal ikke lage nye kopier.
    let igjen = utfor::utfor(&plan, &|_, _| {}).unwrap();
    assert_eq!(igjen.kopiert, 0);
    assert_eq!(igjen.allerede_der, plan.handlinger.len());

    // Angre den siste (tomme) kjøringen, så den første: kopiene flyttes til _Angret, ingenting forsvinner.
    assert_eq!(utfor::angre_siste(mal.path()).unwrap(), 0);
    assert_eq!(utfor::angre_siste(mal.path()).unwrap(), plan.handlinger.len());
    for h in &plan.handlinger {
        assert!(!mal.path().join(&h.til).exists());
    }
    let angret = walkdir::WalkDir::new(mal.path().join(utfor::ANGRET))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .count();
    assert_eq!(angret, plan.handlinger.len());
    assert_eq!(bildesamler_kjerne::finn_filer(&[innboks.clone()], mal.path()).len(), antall_kilder);
}
