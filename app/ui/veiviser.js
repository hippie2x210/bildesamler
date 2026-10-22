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
  $("vvGoogleSorter").disabled = !filer.length;
}

$("vvBestill").addEventListener("click", async () => {
  try {
    await invoke("google_vindu", { mal: samling(), side: "bestill" });
    lagre("googleBestilt", Date.now());
    oppdaterGoogle();
  } catch (e) {
    vvFeil(e);
  }
});

$("vvHent").addEventListener("click", async () => {
  try {
    await invoke("google_vindu", { mal: samling(), side: "nedlastinger" });
  } catch (e) {
    vvFeil(e);
  }
});

$("vvGmail").addEventListener("click", async () => {
  try {
    if (!vv.status.innlogget) {
      await invoke("google_logg_inn");
      vv.status.innlogget = true;
    }
    const post = await invoke("google_hent", { mal: samling() });
    $("vvNedlastet").textContent = post
      ? `Fant e-posten fra Google (${datoTekst(post.mottatt_ms)}). Henter ${post.lenker.length} ${post.lenker.length === 1 ? "fil" : "filer"} …`
      : "Fant ingen e-post fra Google Takeout de siste to ukene. Har du bedt Google om bildene?";
  } catch (e) {
    vvFeil(e);
  }
});

listen("google-nedlasting", ({ payload }) => {
  if (!payload.ferdig) {
    $("vvNedlastet").textContent = "Tar imot en fil fra Google …";
    return;
  }
  if (!payload.ok) {
    $("vvNedlastet").textContent = "En nedlasting stoppet. Trykk «Hent bildene fra Google» og «Last ned» igjen.";
    return;
  }
  lagre("googleFiler", [...new Set([...hent("googleFiler", []), payload.fil])]);
  oppdaterGoogle();
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
  $("vvJobberTittel").textContent = "Leser bildene …";
  $("vvJobberTekst").textContent = "CloudJumper ser på hvert bilde for å finne riktig dato og like bilder. Ingenting kopieres ennå.";
  try {
    oversikt = await invoke("skann", { kilder: vv.kilder, mal });
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
  if (fase === "pakker ut") $("vvJobberTittel").textContent = "Pakker opp filene fra Google …";
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
  ].filter(Boolean);
  $("vvFunn").replaceChildren(...linjer.map((l) => el("li", {}, l)));
  $("vvKopier").disabled = r.filer === 0;
  visSteg("klar");
  for (const v of oversikt.varsler) vvFeil(v);
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
  if (!$("mal").value && vv.status?.standard_samling) $("mal").placeholder = vv.status.standard_samling;
})();
