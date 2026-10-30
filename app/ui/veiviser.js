// Den enkle visningen: ett spørsmål om gangen med store knapper. Bruker de samme kommandoene som
// den avanserte visningen i app.js, og kan hoppe dit med «Vis detaljer».

const vv = {
  kilder: [],
  // Hva brukeren holdt på med, så «Tilbake» og feilmeldinger havner riktig.
  fra: "start",
  status: null,
};

function visSteg(navn) {
  for (const s of document.querySelectorAll("#veiviser [data-steg]")) s.hidden = s.dataset.steg !== navn;
  $("vvFeil").hidden = true;
  if (navn === "google") oppdaterGoogle();
}

// Viser at noe skjer mens en knapp jobber, så ingen tror appen har hengt seg.
async function travel(knapp, tekst, arbeid) {
  const foer = knapp.textContent;
  knapp.disabled = true;
  knapp.textContent = tekst;
  try {
    return await arbeid();
  } catch (e) {
    vvFeil(e);
  } finally {
    knapp.disabled = false;
    knapp.textContent = foer;
  }
}

function vvFeil(melding) {
  $("vvFeil").textContent = String(melding);
  $("vvFeil").hidden = false;
}

for (const b of document.querySelectorAll("#veiviser [data-gaa]")) {
  b.addEventListener("click", () => {
    vv.fra = b.dataset.gaa;
    visSteg(b.dataset.gaa);
  });
}

function samling() {
  return hent("mal", "") || vv.status?.standard_samling || "";
}

function datoTekst(ms) {
  return new Date(ms).toLocaleDateString("nb-NO", { day: "numeric", month: "long" });
}

// --- Google Foto ---

async function oppdaterGoogle() {
  const bestilt = hent("googleBestilt", null);
  $("vvBestilt").hidden = !bestilt;
  if (bestilt) $("vvBestilt").textContent = `Du ba Google om bildene ${datoTekst(bestilt)}.`;
  $("vvGmail").hidden = !vv.status?.satt_opp;
  const filer = hent("googleFiler", []);
  $("vvNedlastet").textContent = filer.length
    ? `CloudJumper har tatt imot ${filer.length} ${filer.length === 1 ? "fil" : "filer"} fra Google.`
    : "";
  if (underveis.size) {
    $("vvNedlastet").textContent += ` Tar imot ${underveis.size === 1 ? "én fil" : underveis.size + " filer"} til nå. Vent til de er ferdige før du sorterer.`;
  }
  $("vvGoogleSorter").disabled = !filer.length || underveis.size > 0;
  // Bare steget brukeren er på nå er framhevet, så det er lett å se hva som er neste.
  const na = filer.length && !underveis.size ? 3 : bestilt || underveis.size ? 2 : 1;
  for (const t of document.querySelectorAll("#veiviser [data-trinn]")) {
    const nr = Number(t.dataset.trinn);
    t.classList.toggle("na", nr === na);
    t.classList.toggle("gjort", nr < na);
    t.querySelector(".stor-knapp").classList.toggle("hoved-knapp", nr === na);
  }
  $("vvGoogleNa").textContent = [
    "Start med steg 1.",
    underveis.size ? "Bildene lastes ned. La Google-vinduet stå åpent." : "Venter på e-post fra Google. Når den kommer, gå til steg 2.",
    "Bildene er hentet. Gå til steg 3.",
  ][na - 1];
}

$("vvBestill").addEventListener("click", () =>
  travel($("vvBestill"), "Åpner Google …", async () => {
    await invoke("google_vindu", { mal: samling(), side: "bestill" });
    lagre("googleBestilt", Date.now());
    oppdaterGoogle();
  })
);

$("vvHent").addEventListener("click", () =>
  travel($("vvHent"), "Åpner Google …", () => invoke("google_vindu", { mal: samling(), side: "nedlastinger" }))
);

$("vvGmail").addEventListener("click", () => travel($("vvGmail"), "Sjekker e-posten …", async () => {
  {
    if (!vv.status.innlogget) {
      await invoke("google_logg_inn");
      vv.status.innlogget = true;
    }
    const post = await invoke("google_hent", { mal: samling() });
    $("vvNedlastet").textContent = post
      ? `Fant e-posten fra Google (${datoTekst(post.mottatt_ms)}). Henter ${post.lenker.length} ${post.lenker.length === 1 ? "fil" : "filer"} …`
      : "Fant ingen e-post fra Google Takeout de siste to ukene. Har du bedt Google om bildene?";
  }
}));

// Filene Google-vinduet holder på å laste ned akkurat nå.
const underveis = new Set();

listen("google-nedlasting", ({ payload }) => {
  if (!payload.ferdig) {
    underveis.add(payload.fil);
  } else {
    underveis.delete(payload.fil);
    if (payload.ok) lagre("googleFiler", [...new Set([...hent("googleFiler", []), payload.fil])]);
  }
  oppdaterGoogle();
  if (!payload.ok) {
    $("vvNedlastet").textContent = "En nedlasting stoppet. Trykk «Hent bildene fra Google» og «Last ned alle» igjen.";
  }
});

$("vvGoogleSorter").addEventListener("click", async () => {
  try {
    const mappe = await invoke("google_nedlastingsmappe", { mal: samling() });
    vv.kilder = [mappe];
    vv.fra = "google";
    visSteg("samling");
    $("vvSamling").textContent = samling();
  } catch (e) {
    vvFeil(e);
  }
});

// --- Mobil via kabel ---

$("vvFinnTelefon").addEventListener("click", () =>
  travel($("vvFinnTelefon"), "Leter …", async () => {
    const liste = await invoke("telefoner");
    $("vvTelefonTekst").textContent = liste.length
      ? "Trykk på telefonen din."
      : "Fant ingen telefon. Sjekk at den er låst opp og at du har valgt «Filoverføring» eller «Stol på», og prøv igjen.";
    $("vvTelefoner").replaceChildren(
      ...liste.map((t) =>
        el("button", { class: "stor", onclick: () => velgTelefon(t) }, `📱 ${t.navn}`)
      )
    );
  })
);

function velgTelefon(t) {
  vv.telefon = t;
  vv.kilder = [];
  vv.fra = "telefon";
  visSteg("samling");
  $("vvSamling").textContent = samling();
}

// --- OneDrive ---

$("vvOneDriveLoggInn").addEventListener("click", () =>
  travel($("vvOneDriveLoggInn"), "Venter på innlogging i nettleseren …", async () => {
    const navn = await invoke("onedrive_logg_inn");
    $("vvOneDriveTekst").textContent = `Logget inn som ${navn}.`;
    vv.kilder = [];
    vv.fra = "onedrive";
    visSteg("samling");
    $("vvSamling").textContent = samling();
  })
);

// --- Mappe ---

$("vvVelgMappe").addEventListener("click", async () => {
  const valgt = await open({ directory: true, title: "Velg mappen med bildene" });
  if (!valgt) return;
  vv.kilder = [valgt];
  vv.fra = "mappe";
  visSteg("samling");
  $("vvSamling").textContent = samling();
});

// --- Samling og skanning ---

$("vvVelgSamling").addEventListener("click", async () => {
  const valgt = await open({ directory: true, title: "Velg hvor bildene skal samles" });
  if (!valgt) return;
  lagre("mal", valgt);
  $("mal").value = valgt;
  $("vvSamling").textContent = valgt;
});

$("vvBrukSamling").addEventListener("click", async () => {
  const mal = samling();
  if (!mal) return vvFeil("Velg hvor bildene skal samles.");
  lagre("mal", mal);
  $("mal").value = mal;
  $("apne").disabled = false;
  visSteg("jobber");
  if (vv.fra === "telefon" && vv.telefon) {
    $("vvJobberTittel").textContent = "Henter bildene fra telefonen …";
    $("vvJobberTekst").textContent = "La telefonen være koblet til og ulåst. Bilder som er hentet før, hentes ikke på nytt.";
    try {
      const h = await invoke("telefon_hent", { id: vv.telefon.id, navn: vv.telefon.navn, mal });
      vv.kilder = [h.mappe];
      vv.telefonFeil = h.feil;
    } catch (e) {
      visSteg("telefon");
      return vvFeil(e);
    }
  }
  if (vv.fra === "onedrive") {
    $("vvJobberTittel").textContent = "Henter bildene fra OneDrive …";
    $("vvJobberTekst").textContent = "Dette kan ta lang tid på et tregt nett. Bilder som er hentet før, hentes ikke på nytt.";
    try {
      const h = await invoke("onedrive_hent", { mal });
      vv.kilder = [h.mappe];
      vv.telefonFeil = h.feil;
    } catch (e) {
      visSteg("onedrive");
      return vvFeil(e);
    }
  }
  $("vvJobberTittel").textContent = "Leser bildene …";
  $("vvJobberTekst").textContent = "CloudJumper ser på hvert bilde for å finne riktig dato og like bilder. Ingenting kopieres ennå.";
  try {
    oversikt = await invoke("skann", { kilder: vv.kilder, mal });
    if ((vv.fra === "telefon" || vv.fra === "onedrive") && vv.telefonFeil?.length) {
      oversikt.varsler.push(`${vv.telefonFeil.length} filer kunne ikke hentes ${vv.fra === "telefon" ? "fra telefonen" : "fra OneDrive"}. Prøv igjen senere, så hentes bare de som mangler.`);
    }
    settKilder(vv.kilder);
    visteGrupper = SIDE;
    visResultat();
    visFunn();
  } catch (e) {
    visSteg(vv.fra);
    vvFeil(e);
  }
});

listen("fremdrift", ({ payload }) => {
  const { fase, ferdig, total } = payload;
  $("vvLinje").style.width = total ? `${(100 * ferdig) / total}%` : "0";
  if (fase === "onedrive") $("vvJobberTittel").textContent = `Henter bildene fra OneDrive … (${ferdig} av ${total})`;
  else if (fase === "telefon") $("vvJobberTittel").textContent = `Henter bildene fra telefonen … (${ferdig} av ${total})`;
  else if (fase === "pakker ut") $("vvJobberTittel").textContent = "Pakker opp filene fra Google …";
  else if (fase === "skanner") $("vvJobberTittel").textContent = `Leser bildene … (${ferdig} av ${total})`;
  else $("vvJobberTittel").textContent = `Kopierer … (${ferdig} av ${total})`;
});

function antall(n, en, flere) {
  return `${n.toLocaleString("nb-NO")} ${n === 1 ? en : flere}`;
}

function visFunn() {
  const r = oversikt.rapport;
  const linjer = [
    antall(r.bilder, "bilde", "bilder") + " sorteres etter år og måned",
    r.videoer ? antall(r.videoer, "video", "videoer") + " sorteres i en egen mappe" : null,
    r.duplikater ? `${antall(r.duplikater, "bilde", "bilder")} finnes fra før og legges til side i «Duplikater»` : null,
    r.uten_dato ? `${antall(r.uten_dato, "bilde", "bilder")} uten sikker dato legges i «Uten dato»` : null,
    r.ulesbare ? `${antall(r.ulesbare, "fil", "filer")} kunne ikke leses og hoppes over` : null,
    r.takeout_albumkopier
      ? `${antall(r.takeout_albumkopier, "bilde", "bilder")} lå i flere Google-album og kopieres bare én gang`
      : null,
    r.allerede_i_samlingen
      ? `${antall(r.allerede_i_samlingen, "bilde", "bilder")} er allerede i samlingen og hoppes over`
      : null,
  ].filter(Boolean);
  $("vvFunn").replaceChildren(...linjer.map((l) => el("li", {}, l)));
  const nytt = r.bilder + r.videoer + r.duplikater + r.uten_dato;
  if (!nytt) $("vvFunn").append(el("li", {}, "Ingenting nytt å kopiere. Alt er allerede i samlingen."));
  $("vvKopier").disabled = nytt === 0;
  visSteg("klar");
  const varsler = oversikt.varsler;
  $("vvVarsler").hidden = !varsler.length;
  $("vvVarsler").replaceChildren(
    varsler.length === 1 ? varsler[0] : el("ul", {}, varsler.map((v) => el("li", {}, v)))
  );
}

$("vvKopier").addEventListener("click", async () => {
  const ok = await sporBruker(
    `Kopiere bildene til ${samling()}?\n\nOriginalene blir liggende der de er.`,
    { title: "Kopier bildene", kind: "info", okLabel: "Kopier", cancelLabel: "Ikke nå" }
  );
  if (!ok) return;
  visSteg("jobber");
  $("vvJobberTittel").textContent = "Kopierer …";
  $("vvJobberTekst").textContent = "Du kan gå fra PC-en. CloudJumper sier fra når den er ferdig.";
  try {
    const res = await invoke("kopier");
    const deler = [`${antall(res.kopiert, "bilde", "bilder")} er kopiert til samlingen.`];
    if (res.allerede_der) deler.push(`${antall(res.allerede_der, "bilde", "bilder")} lå der fra før.`);
    if (res.feil.length) deler.push(`${antall(res.feil.length, "fil", "filer")} kunne ikke kopieres. Se «Avansert visning».`);
    $("vvFerdigTekst").textContent = deler.join(" ");
    if (vv.fra === "google") lagre("googleFiler", []);
    visSteg("ferdig");
  } catch (e) {
    visSteg("klar");
    vvFeil(e);
  }
});

$("vvApne").addEventListener("click", () => invoke("apne_mappe", { sti: samling() }).catch(vvFeil));

// --- Bytte mellom enkel og avansert visning ---

function visAvansert(ja) {
  $("veiviser").hidden = ja;
  $("avansert").hidden = !ja;
  lagre("avansert", ja);
}

$("vvAvansert").addEventListener("click", (e) => {
  e.preventDefault();
  visAvansert(true);
});
$("vvDetaljer").addEventListener("click", (e) => {
  e.preventDefault();
  visAvansert(true);
});
$("tilVeiviser").addEventListener("click", () => visAvansert(false));

// --- Oppstart ---

(async () => {
  visAvansert(hent("avansert", false));
  try {
    vv.status = await invoke("google_status");
  } catch {}
  try {
    $("vvOneDriveValg").hidden = !(await invoke("onedrive_status")).satt_opp;
  } catch {}
  if (!$("mal").value && vv.status?.standard_samling) $("mal").placeholder = vv.status.standard_samling;
})();
