//! Windows Portable Devices: samme driver som Utforsker bruker for telefoner.

use crate::{allerede_hentet, er_media, interessant, trygt, Henting, Telefon};
use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use windows::core::{GUID, PCWSTR, PWSTR};
use windows::Win32::Devices::PortableDevices::*;
use windows::Win32::Foundation::{GENERIC_READ, PROPERTYKEY};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, IStream, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, STGM_READ,
};

fn com() {
    // Feiler bare hvis tråden allerede er satt opp på en annen måte, og da virker COM likevel.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

fn bred(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Leser og frigjør en streng Windows har laget.
unsafe fn ta(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = p.to_string().unwrap_or_default();
    CoTaskMemFree(Some(p.0 as *const _));
    s
}

/// Telefoner og andre bærbare enheter som er koblet til nå.
pub fn finn() -> Result<Vec<Telefon>> {
    com();
    unsafe {
        let mgr: IPortableDeviceManager = CoCreateInstance(&PortableDeviceManager, None, CLSCTX_INPROC_SERVER)?;
        let _ = mgr.RefreshDeviceList();
        let mut antall = 0u32;
        mgr.GetDevices(std::ptr::null_mut(), &mut antall)?;
        let mut ider = vec![PWSTR::null(); antall as usize];
        if antall > 0 {
            mgr.GetDevices(ider.as_mut_ptr(), &mut antall)?;
        }
        let mut ut = Vec::new();
        for p in ider.into_iter().take(antall as usize) {
            let id = ta(p);
            let idw = bred(&id);
            let mut lengde = 0u32;
            let _ = mgr.GetDeviceFriendlyName(PCWSTR(idw.as_ptr()), PWSTR::null(), &mut lengde);
            let mut buf = vec![0u16; lengde.max(1) as usize];
            let navn = if mgr.GetDeviceFriendlyName(PCWSTR(idw.as_ptr()), PWSTR(buf.as_mut_ptr()), &mut lengde).is_ok() {
                String::from_utf16_lossy(&buf).trim_end_matches('\0').to_string()
            } else {
                String::new()
            };
            ut.push(Telefon { navn: if navn.is_empty() { "Telefon".into() } else { navn }, id });
        }
        Ok(ut)
    }
}

struct Objekt {
    id: String,
    navn: String,
    mappe: bool,
    storrelse: Option<u64>,
}

struct Enhet {
    innhold: IPortableDeviceContent,
    egenskaper: IPortableDeviceProperties,
    ressurser: IPortableDeviceResources,
    nokler: IPortableDeviceKeyCollection,
}

impl Enhet {
    unsafe fn apne(id: &str) -> Result<Self> {
        let klient: IPortableDeviceValues = CoCreateInstance(&PortableDeviceValues, None, CLSCTX_INPROC_SERVER)?;
        let navn = bred("CloudJumper");
        klient.SetStringValue(&WPD_CLIENT_NAME, PCWSTR(navn.as_ptr()))?;
        klient.SetUnsignedIntegerValue(&WPD_CLIENT_MAJOR_VERSION, 0)?;
        // Bare lesetilgang: CloudJumper kan ikke endre eller slette noe på telefonen.
        klient.SetUnsignedIntegerValue(&WPD_CLIENT_DESIRED_ACCESS, GENERIC_READ.0)?;
        let enhet: IPortableDevice = CoCreateInstance(&PortableDeviceFTM, None, CLSCTX_INPROC_SERVER)?;
        let idw = bred(id);
        enhet
            .Open(PCWSTR(idw.as_ptr()), &klient)
            .context("Fikk ikke åpnet telefonen. Lås den opp og velg «Filoverføring» (Android) eller «Stol på» (iPhone)")?;
        let innhold = enhet.Content()?;
        let egenskaper = innhold.Properties()?;
        let ressurser = innhold.Transfer()?;
        let nokler: IPortableDeviceKeyCollection =
            CoCreateInstance(&PortableDeviceKeyCollection, None, CLSCTX_INPROC_SERVER)?;
        for k in [WPD_OBJECT_NAME, WPD_OBJECT_ORIGINAL_FILE_NAME, WPD_OBJECT_CONTENT_TYPE, WPD_OBJECT_SIZE] {
            nokler.Add(&k)?;
        }
        Ok(Enhet { innhold, egenskaper, ressurser, nokler })
    }

    unsafe fn barn(&self, forelder: &str) -> Result<Vec<Objekt>> {
        let fw = bred(forelder);
        let liste = self.innhold.EnumObjects(0, PCWSTR(fw.as_ptr()), None)?;
        let mut ider = Vec::new();
        loop {
            let mut buf = [PWSTR::null(); 64];
            let mut fikk = 0u32;
            let hr = liste.Next(&mut buf, &mut fikk);
            for p in buf.into_iter().take(fikk as usize) {
                ider.push(ta(p));
            }
            if hr.is_err() || fikk == 0 {
                break;
            }
        }
        let mut ut = Vec::with_capacity(ider.len());
        for id in ider {
            let iw = bred(&id);
            let Ok(v) = self.egenskaper.GetValues(PCWSTR(iw.as_ptr()), &self.nokler) else { continue };
            let streng = |k: &PROPERTYKEY| v.GetStringValue(k).ok().map(|p| ta(p)).filter(|s| !s.is_empty());
            let navn = streng(&WPD_OBJECT_ORIGINAL_FILE_NAME).or_else(|| streng(&WPD_OBJECT_NAME)).unwrap_or_default();
            let typ: GUID = v.GetGuidValue(&WPD_OBJECT_CONTENT_TYPE).unwrap_or_default();
            let mappe = typ == WPD_CONTENT_TYPE_FOLDER || typ == WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT;
            let storrelse = v.GetUnsignedLargeIntegerValue(&WPD_OBJECT_SIZE).ok();
            ut.push(Objekt { id, navn, mappe, storrelse });
        }
        Ok(ut)
    }

    unsafe fn kopier(&self, id: &str, til: &Path) -> Result<()> {
        let iw = bred(id);
        let mut optimal = 0u32;
        let mut strom: Option<IStream> = None;
        self.ressurser.GetStream(PCWSTR(iw.as_ptr()), &WPD_RESOURCE_DEFAULT, STGM_READ.0, &mut optimal, &mut strom)?;
        let strom = strom.context("telefonen ga ingen data")?;
        let tmp = til.with_extension("cloudjumper-del");
        let mut ut = std::io::BufWriter::new(fs::File::create(&tmp)?);
        let mut buf = vec![0u8; (optimal as usize).clamp(64 << 10, 4 << 20)];
        loop {
            let mut lest = 0u32;
            strom.Read(buf.as_mut_ptr() as *mut _, buf.len() as u32, Some(&mut lest)).ok()?;
            if lest == 0 {
                break;
            }
            ut.write_all(&buf[..lest as usize])?;
        }
        ut.flush()?;
        drop(ut);
        fs::rename(&tmp, til)?;
        Ok(())
    }
}

/// Teller bildene og videoene på telefonen uten å kopiere noe: (antall, bytes).
pub fn tell(telefon: &str) -> Result<(usize, u64)> {
    com();
    unsafe {
        let enhet = Enhet::apne(telefon)?;
        let mut feil = Vec::new();
        let filer = finn_filer(&enhet, Path::new(""), &mut feil)?;
        Ok((filer.len(), filer.iter().filter_map(|(o, _)| o.storrelse).sum()))
    }
}

unsafe fn finn_filer(enhet: &Enhet, mal: &Path, feil: &mut Vec<String>) -> Result<Vec<(Objekt, std::path::PathBuf)>> {
    // Lagringsområdene ligger under DEVICE.
    let mut filer = Vec::new();
    let mut ko: Vec<(String, Vec<String>, std::path::PathBuf)> = Vec::new();
    for lager in enhet.barn("DEVICE")? {
        ko.push((lager.id, Vec::new(), mal.join(trygt(&lager.navn))));
    }
    while let Some((id, sti, mappe)) = ko.pop() {
        let barn = match enhet.barn(&id) {
            Ok(b) => b,
            Err(e) => {
                feil.push(format!("{}: {e}", mappe.display()));
                continue;
            }
        };
        for o in barn {
            if o.mappe {
                let mut ny = sti.clone();
                ny.push(o.navn.to_lowercase());
                if interessant(&ny) {
                    ko.push((o.id, ny, mappe.join(trygt(&o.navn))));
                }
            } else if !sti.is_empty() && er_media(&o.navn) {
                filer.push((o, mappe.clone()));
            }
        }
    }
    Ok(filer)
}

/// Kopierer bildene og videoene fra telefonen til `mal`. Returnerer hvor mange som ble hentet.
pub fn hent(telefon: &str, mal: &Path, fremdrift: &dyn Fn(usize, usize)) -> Result<Henting> {
    com();
    let mut h = Henting { mappe: mal.to_path_buf(), ..Default::default() };
    unsafe {
        let enhet = Enhet::apne(telefon)?;
        // Først en liste over alt, så fremdriften blir riktig.
        let filer = finn_filer(&enhet, mal, &mut h.feil)?;
        let total = filer.len();
        for (n, (o, mappe)) in filer.into_iter().enumerate() {
            fremdrift(n, total);
            let til = mappe.join(trygt(&o.navn));
            if allerede_hentet(&til, o.storrelse) {
                h.fantes += 1;
                continue;
            }
            let res = fs::create_dir_all(&mappe).map_err(anyhow::Error::from).and_then(|_| enhet.kopier(&o.id, &til));
            match res {
                Ok(()) => h.hentet += 1,
                Err(e) => h.feil.push(format!("{}: {e:#}", til.display())),
            }
        }
        fremdrift(total, total);
    }
    Ok(h)
}
