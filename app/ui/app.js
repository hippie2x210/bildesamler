// Grensesnittet for CloudJumper. Alt arbeid skjer i Rust-kjernen; her vises bare valg og resultat.
const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open, ask } = window.__TAURI__.dialog;

const $ = (id) => document.getElementById(id);
const SIDE = 20;
let oversikt = null;
let visteGrupper = SIDE;

// Lager elementer uten innerHTML, så filnavn aldri tolkes som HTML.
function el(tag, attrs = {}, ...barn) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else e.setAttribute(k, v);
  }
  for (const b of barn.flat()) {
    if (b != null) e.append(b instanceof Node ? b : document.createTextNode(String(b)));
  }
  return e;
}

function lagre(navn, verdi) {
  try { localStorage.setItem(navn, JSON.stringify(verdi)); } catch {}
}
function hent(navn, standard) {
  try { return JSON.parse(localStorage.getItem(navn)) ?? standard; } catch { return standard; }
}

function storrelse(b) {
  if (b > 1e9) return (b / 1e9).toFixed(1) + " GB";
  if (b > 1e6) return (b / 1e6).toFixed(1) + " MB";
  return Math.max(1, Math.round(b / 1e3)) + " kB";
}

function status(tekst) { $("status").textContent = tekst; }

function visFeil(melding) {
  $("varsler").replaceChildren(el("div", { class: "feil" }, String(melding)));
}

// --- Mapper ---

function kilder() { return [...$("kilder").options].map((o) => o.value); }

function settKilder(liste) {
  $("kilder").replaceChildren(...liste.map((k) => el("option", { value: k }, k)));
  lagre("kilder", liste);
}

$("leggTilKilde").addEventListener("click", async () => {
  const valgt = await open({ directory: true, multiple: true, title: "Velg mapper med bilder" });
  if (!valgt) return;
  const nye = Array.isArray(valgt) ? valgt : [valgt];
  settKilder([...new Set([...kilder(), ...nye])]);
});

$("fjernKilde").addEventListener("click", () => {
  const valgt = new Set([...$("kilder").selectedOptions].map((o) => o.value));
  settKilder(kilder().filter((k) => !valgt.has(k)));
});

$("velgMal").addEventListener("click", async () => {
  const valgt = await open({ directory: true, title: "Velg hvor bildene skal samles" });
  if (!valgt) return;
  $("mal").value = valgt;
  lagre("mal", valgt);
  $("apne").disabled = false;
});

// --- Fremdrift ---

listen("fremdrift", ({ payload }) => {
  const { fase, ferdig, total } = payload;
  $("fremdrift").hidden = false;
  const tekst = fase === "skanner" ? "Leser filer" : "Kopierer";
  $("fremdriftTekst").textContent = `${tekst}: ${ferdig} av ${total}`;
  $("fremdriftLinje").style.width = total ? `${(100 * ferdig) / total}%` : "0";
});

function opptatt(ja) {
  for (const id of ["skann", "kopier", "angre", "leggTilKilde", "fjernKilde", "velgMal"]) {
    $(id).disabled = ja || (id === "kopier" && !oversikt);
  }
}

// --- Handlinger ---

$("skann").addEventListener("click", async () => {
  $("varsler").replaceChildren();
  opptatt(true);
  status("Skanner…");
  try {
    oversikt = await invoke("skann", { kilder: kilder(), mal: $("mal").value });
    visteGrupper = SIDE;
    visResultat();
    status("Skanning ferdig. Se over resultatet før du kopierer.");
  } catch (e) {
    oversikt = null;
    visFeil(e);
    status("Skanningen stoppet");
  } finally {
    opptatt(false);
  }
});

$("kopier").addEventListener("click", async () => {
  const r = oversikt.rapport;
  const ok = await ask(
    `Kopiere ${r.filer} filer til ${$("mal").value}?\n\n` +
      `${r.duplikater} duplikater legges i «Duplikater». Originalene blir liggende der de er.`,
    { title: "Kopier til samlingen", kind: "info", okLabel: "Kopier", cancelLabel: "Avbryt" }
  );
  if (!ok) return;
  opptatt(true);
  status("Kopierer…");
  try {
    const res = await invoke("kopier");
    const deler = [`${res.kopiert} filer kopiert`];
    if (res.allerede_der) deler.push(`${res.allerede_der} lå der fra før`);
    if (res.feil.length) deler.push(`${res.feil.length} feilet`);
    status(deler.join(", "));
    if (res.feil.length) {
      visFeil(`Noen filer kunne ikke kopieres:\n${res.feil.map(([s, e]) => `${s}: ${e}`).join("\n")}`);
    }
  } catch (e) {
    visFeil(e);
  } finally {
    opptatt(false);
  }
});

$("angre").addEventListener("click", async () => {
  const mal = $("mal").value;
  if (!mal) return visFeil("Velg målmappen du vil angre i.");
  const ok = await ask("Angre siste kopiering? Kopiene flyttes til mappen «_Angret» i samlingen. Ingenting slettes.", {
    title: "Angre",
    kind: "warning",
    okLabel: "Angre",
    cancelLabel: "Avbryt",
  });
  if (!ok) return;
  try {
    const antall = await invoke("angre", { mal });
    status(`${antall} filer flyttet til _Angret`);
  } catch (e) {
    visFeil(e);
  }
});

$("apne").addEventListener("click", () => invoke("apne_mappe", { sti: $("mal").value }).catch(visFeil));

// --- Faner ---

for (const fane of document.querySelectorAll('[role="tab"]')) {
  fane.addEventListener("click", (e) => {
    for (const f of document.querySelectorAll('[role="tab"]')) f.setAttribute("aria-selected", f === fane);
    for (const p of document.querySelectorAll("[data-panel]")) p.hidden = p.dataset.panel !== fane.dataset.fane;
  });
}

// --- Visning ---

function visResultat() {
  const o = oversikt;
  $("resultat").hidden = false;
  $("kopier").disabled = false;

  const varsler = o.varsler.map((v) => el("div", { class: "varsel" }, v));
  if (o.ulesbare.length) {
    varsler.push(el("div", { class: "varsel" }, `${o.ulesbare.length} filer kunne ikke leses og blir ikke kopiert.`));
  }
  $("varsler").replaceChildren(...varsler);

  const r = o.rapport;
  const rad = (t, v) => el("tr", {}, el("td", {}, t), el("td", {}, v));
  $("oversikt").replaceChildren(
    el("table", { class: "tall" },
      rad("Filer funnet", r.filer),
      rad("Bilder som sorteres i Bilder/År/Måned", r.bilder),
      rad("Videoer som sorteres i Videoer/År/Måned", r.videoer),
      rad("Duplikater (legges i «Duplikater»)", `${r.duplikater} (${storrelse(r.duplikat_bytes)})`),
      rad("Uten sikker dato (legges i «Uten dato»)", r.uten_dato),
      rad("Filer med advarsel", r.med_advarsel),
      rad("Kunne ikke leses", r.ulesbare)
    ),
    el("p", { class: "hint" }, "Ingenting er kopiert ennå. Se gjennom fanene, og trykk «Kopier til samlingen» når du er fornøyd.")
  );

  visDuplikater();
  $("utenDato").replaceChildren(filTabell(o.uten_dato, "Alle filer har en sikker dato."));
  $("advarsler").replaceChildren(filTabell(o.advarsler, "Ingen advarsler."));
}

function kort(f, gruppe, erOriginal) {
  return el("div", { class: "kort" + (erOriginal ? " original" : "") },
    el("img", { src: convertFileSrc(f.sti), loading: "lazy", alt: f.navn }),
    el("div", { class: "info" },
      el("b", {}, f.navn), el("br"),
      `${f.bredde}×${f.hoyde}, ${storrelse(f.storrelse)}`, el("br"),
      f.dato ? `${f.dato} (${f.kilde})` : "Ingen dato", el("br"),
      f.kamera ?? "Ukjent kamera", el("br"),
      erOriginal
        ? el("b", {}, "✔ Beholdes")
        : el("button", { onclick: () => velgOriginal(gruppe, f.indeks) }, "Behold denne")
    )
  );
}

function visDuplikater() {
  const grupper = oversikt.grupper;
  if (!grupper.length) {
    $("duplikater").replaceChildren(el("p", {}, "Fant ingen duplikater."));
    $("flereDuplikater").hidden = true;
    return;
  }
  $("duplikater").replaceChildren(
    ...grupper.slice(0, visteGrupper).map((g) =>
      el("div", { class: "gruppe" },
        el("div", { class: "gruppe-tittel" }, g.eksakt ? "Helt like filer" : "Samme bilde i ulik kvalitet"),
        el("div", { class: "kort-rad" }, g.filer.map((f, i) => kort(f, g.indeks, i === 0)))
      )
    )
  );
  $("flereDuplikater").hidden = visteGrupper >= grupper.length;
}

$("flereDuplikater").addEventListener("click", () => {
  visteGrupper += SIDE;
  visDuplikater();
});

async function velgOriginal(gruppe, fil) {
  try {
    oversikt = await invoke("sett_original", { gruppe, fil });
    visDuplikater();
  } catch (e) {
    visFeil(e);
  }
}

function filTabell(filer, tom) {
  if (!filer.length) return el("p", {}, tom);
  return el("table", { class: "liste" },
    el("tr", {}, el("th", {}, "Fil"), el("th", {}, "Dato"), el("th", {}, "Merknad")),
    filer.map((f) =>
      el("tr", {},
        el("td", {}, f.sti),
        el("td", {}, f.dato ?? "–"),
        el("td", {}, f.advarsler.join(" "))
      )
    )
  );
}

// --- Oppstart ---

settKilder(hent("kilder", []));
$("mal").value = hent("mal", "");
$("apne").disabled = !$("mal").value;
