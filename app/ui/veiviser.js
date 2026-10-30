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
  if (navn === "start") visHvor();
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
    fortsett();
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
  fortsett();
}

// --- OneDrive ---

$("vvOneDriveLoggInn").addEventListener("click", () =>
  travel($("vvOneDriveLoggInn"), "Venter på innlogging i nettleseren …", async () => {
    const navn = await invoke("onedrive_logg_inn");
    $("vvOneDriveTekst").textContent = `Logget inn som ${navn}.`;
    vv.kilder = [];
    vv.fra = "onedrive";
    fortsett();
  })
);

// --- Mappe ---

$("vvVelgMappe").addEventListener("click", async () => {
  const valgt = await open({ directory: true, title: "Velg mappen med bildene" });
  if (!valgt) return;
  vv.kilder = [valgt];
  vv.fra = "mappe";
  fortsett();
});

// --- Samling og skanning ---

// Første gang spør vi hvor samlingen skal ligge. Etterpå brukes samme sted, så det er ett steg mindre.
// Stedet vises på startsiden og kan byttes der.
function fortsett() {
  if (hent("mal", "")) return kjor();
  visSteg("samling");
  $("vvSamling").textContent = samling();
}

function visHvor() {
  const mal = hent("mal", "");
  $("vvHvor").hidden = !mal;
  $("vvHvorSti").textContent = mal;
  const sist = hent("sist", null);
  $("vvSist").hidden = !sist;
  if (sist) $("vvSist").textContent = `Sist: ${datoTekst(sist.tid)} ble ${antall(sist.kopiert, "bilde", "bilder")} kopiert fra ${sist.kilde}.`;
}

$("vvEndreSamling").addEventListener("click", async (e) => {
  e.preventDefault();
  const valgt = await open({ directory: true, title: "Velg hvor bildene skal samles" });
  if (!valgt) return;
  lagre("mal", valgt);
  $("mal").value = valgt;
  visHvor();
});

$("vvVelgSamling").addEventListener("click", async () => {
  const valgt = await open({ directory: true, title: "Velg hvor bildene skal samles" });
  if (!valgt) return;
  lagre("mal", valgt);
  $("mal").value = valgt;
  $("vvSamling").textContent = valgt;
});

$("vvBrukSamling").addEventListener("click", () => kjor());

async function kjor() {
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
}

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
  $("vvKopierTil").textContent = nytt ? `Kopiene legges i ${samling()}.` : "";
  $("vvSeLike").hidden = !oversikt.grupper.length;
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
    const res = await invoke("kopier", { kilde: kildeNavn() });
    lagre("sist", { tid: Date.now(), kopiert: res.kopiert, kilde: kildeNavn() });
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

function kildeNavn() {
  if (vv.fra === "google") return "Google Foto";
  if (vv.fra === "telefon") return vv.telefon?.navn ?? "mobilen";
  if (vv.fra === "onedrive") return "OneDrive";
  return vv.kilder[0] ?? "en mappe";
}

$("vvAngre").addEventListener("click", async (e) => {
  e.preventDefault();
  const ok = await sporBruker(
    "Angre kopieringen?\n\nKopiene som nettopp ble laget, flyttes til mappen «_Angret» i samlingen. Originalene røres ikke, og ingenting slettes.",
    { title: "Angre", kind: "warning", okLabel: "Angre", cancelLabel: "Nei, behold dem" }
  );
  if (!ok) return;
  try {
    const n = await invoke("angre", { mal: samling() });
    $("vvFerdigTekst").textContent = `Angret. ${antall(n, "fil", "filer")} er flyttet til «_Angret» i samlingen.`;
    lagre("sist", null);
  } catch (err) {
    vvFeil(err);
  }
});

// --- Like bilder side om side ---

// Én gruppe om gangen med store bilder. Enklere å se enn den lange listen i den avanserte visningen.
let likeNr = 0;

function storKort(f, merke, knapp) {
  return el("div", { class: "stort-kort" },
    el("div", { class: "merke" }, merke),
    el("img", { src: convertFileSrc(f.sti, "mini"), alt: f.navn, onerror: skjulBilde }),
    el("div", {}, el("b", {}, f.navn)),
    el("div", {}, `${f.bredde} × ${f.hoyde} punkter, ${storrelse(f.storrelse)}`),
    el("div", {}, f.dato ? `Tatt ${f.dato}` : "Ingen dato"),
    knapp
  );
}

function visLike() {
  const grupper = oversikt.grupper;
  likeNr = Math.max(0, Math.min(likeNr, grupper.length - 1));
  const g = grupper[likeNr];
  $("vvLikeTittel").textContent = `Like bilder (${likeNr + 1} av ${grupper.length})`;
  const [orig, ...andre] = g.filer;
  $("vvPar").replaceChildren(
    storKort(orig, "✔ Beholdes", null),
    ...andre.map((f) =>
      storKort(f, "Legges i «Duplikater»",
        el("button", { class: "stor-knapp", onclick: () => byttOriginal(g.indeks, f.indeks) }, "Behold heller denne"))
    )
  );
  $("vvForrige").disabled = likeNr === 0;
  $("vvNeste").disabled = likeNr >= grupper.length - 1;
}

async function byttOriginal(gruppe, fil) {
  try {
    oversikt = await invoke("sett_original", { gruppe, fil });
    visDuplikater();
    visLike();
  } catch (e) {
    vvFeil(e);
  }
}

$("vvSeLike").addEventListener("click", () => {
  likeNr = 0;
  visSteg("like");
  visLike();
});
$("vvForrige").addEventListener("click", () => { likeNr--; visLike(); });
$("vvNeste").addEventListener("click", () => { likeNr++; visLike(); });
$("vvTilOversikt").addEventListener("click", () => visSteg("klar"));

// --- Større tekst ---

function storTekst(ja) {
  document.body.classList.toggle("stor-skrift", ja);
  $("vvStorTekst").textContent = ja ? "Vanlig tekst" : "Større tekst";
  lagre("storTekst", ja);
}

$("vvStorTekst").addEventListener("click", (e) => {
  e.preventDefault();
  storTekst(!hent("storTekst", false));
});

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
  storTekst(hent("storTekst", false));
  visHvor();
  try {
    vv.status = await invoke("google_status");
  } catch {}
  try {
    $("vvOneDriveValg").hidden = !(await invoke("onedrive_status")).satt_opp;
  } catch {}
  if (!$("mal").value && vv.status?.standard_samling) $("mal").placeholder = vv.status.standard_samling;
})();
