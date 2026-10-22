// Tilpasninger for telefonen. Lastes før app.js og veiviser.js, og gjør ingenting på PC.
//
// Android har ingen mappedialog som gir en vanlig filsti, så dialog.open erstattes her av en enkel
// mappeliste. Telefonen selv (MainActivity.kt) svarer på tilgang, lagringsplasser og «åpne mappe».
(function () {
  const telefon = window.CloudJumperAndroid;
  if (!telefon) return;
  document.documentElement.classList.add("mobil");

  const tauri = window.__TAURI__;
  const kjerneInvoke = tauri.core.invoke;

  function lag(tag, klasse, tekst) {
    const e = document.createElement(tag);
    if (klasse) e.className = klasse;
    if (tekst != null) e.textContent = tekst;
    return e;
  }

  function gb(b) {
    return b >= 0 ? ` (${(b / 1e9).toLocaleString("nb-NO", { maximumFractionDigits: 1 })} GB ledig)` : "";
  }

  function lagringsplasser() {
    try {
      return JSON.parse(telefon.lagringsplasser());
    } catch {
      return [{ navn: "Telefonen", sti: "/storage/emulated/0", ekstern: false, ledig: -1 }];
    }
  }

  // --- Tilgang til bildene ---

  // Uten «Tilgang til alle filer» kan CloudJumper verken lese bildene eller kopiere dem til en disk.
  const tilgang = lag("div", "mobil-tilgang");
  tilgang.hidden = true;
  tilgang.append(
    lag("h1", null, "CloudJumper trenger lov til å se bildene"),
    lag("p", "stor-tekst", "Trykk knappen under. Da åpnes innstillingene. Slå på bryteren ved CloudJumper, og trykk tilbake."),
    lag("p", null, "CloudJumper lager bare kopier. Ingenting slettes eller sendes noe sted."),
  );
  const giTilgang = lag("button", "stor-knapp hoved-knapp", "Gi tilgang");
  giTilgang.addEventListener("click", () => telefon.beOmTilgang());
  tilgang.append(giTilgang);

  function sjekkTilgang() {
    tilgang.hidden = telefon.harTilgang();
  }
  window.addEventListener("cloudjumper-tilbake", sjekkTilgang);
  document.addEventListener("DOMContentLoaded", () => {
    document.body.append(tilgang);
    sjekkTilgang();
    pcTekster();
  });

  // På telefonen åpnes Google i nettleseren, og filene havner i Nedlastinger.
  function pcTekster() {
    const tekst = (id, t) => {
      const e = document.getElementById(id);
      if (e) e.textContent = t;
    };
    const hent = document.querySelector('#veiviser [data-trinn="2"] p');
    if (hent) {
      hent.textContent =
        "Når e-posten fra Google har kommet: trykk her. Trykk «Last ned» ved hver fil på Google-siden, og vent til nettleseren er ferdig. Kom så tilbake hit.";
    }
    tekst("vvGoogleSorter", "Sorter bildene i Nedlastinger");
    const start = document.querySelector('#veiviser [data-steg="start"] [data-gaa="mappe"]');
    if (start) {
      start.replaceChildren("📱 Bildene på telefonen", lag("small", null, "Kamera, WhatsApp, nedlastinger eller en USB-disk"));
    }
    const mappe = document.querySelector('#veiviser [data-steg="mappe"] p');
    if (mappe) mappe.textContent = "Velg mappen med bildene, for eksempel DCIM (kameraet) eller en USB-disk.";
  }

  // --- Mappevelger ---

  // Viser en mappeliste over hele skjermen. Svarer med stien, en liste med stier (multiple) eller null.
  function velg(valg = {}) {
    const zip = (valg.filters || []).flatMap((f) => f.extensions || []);
    const filmodus = !valg.directory;
    return new Promise((ferdig) => {
      const boks = lag("div", "mobil-velger");
      const tittel = lag("h1", null, valg.title || (filmodus ? "Velg fil" : "Velg mappe"));
      const her = lag("p", "mobil-her");
      const liste = lag("div", "mobil-liste");
      const knapper = lag("div", "mobil-knapper");
      const avbryt = lag("button", "stor-knapp", "Avbryt");
      const brukDenne = lag("button", "stor-knapp hoved-knapp", "Bruk denne mappen");
      const nyMappe = lag("button", "stor-knapp", "Ny mappe her");
      nyMappe.hidden = true; // Mappen lages av CloudJumper ved kopiering, så det holder å skrive navnet.
      knapper.append(avbryt);
      if (!filmodus) knapper.append(brukDenne);
      boks.append(tittel, her, liste, knapper);
      document.body.append(boks);

      let gjeldende = null;
      const slutt = (svar) => {
        boks.remove();
        ferdig(svar == null ? null : valg.multiple ? [svar] : svar);
      };
      avbryt.addEventListener("click", () => slutt(null));
      brukDenne.addEventListener("click", () => gjeldende && slutt(gjeldende));

      function rad(tekst, klasse, handling) {
        const b = lag("button", "mobil-rad " + klasse, tekst);
        b.addEventListener("click", handling);
        liste.append(b);
      }

      function visRot() {
        gjeldende = null;
        brukDenne.disabled = true;
        her.textContent = "Velg telefonen eller en disk:";
        liste.replaceChildren();
        for (const p of lagringsplasser()) {
          rad((p.ekstern ? "💾 " : "📱 ") + p.navn + gb(p.ledig), "plass", () => visMappe(p.sti));
        }
      }

      async function visMappe(sti) {
        let innhold;
        try {
          innhold = await kjerneInvoke("mappeliste", { sti, endelser: zip });
        } catch (e) {
          her.textContent = String(e);
          return;
        }
        gjeldende = sti;
        brukDenne.disabled = false;
        her.textContent = sti.replace(/^\/storage\/emulated\/0/, "Telefonen");
        liste.replaceChildren();
        const rot = lagringsplasser().some((p) => p.sti === sti);
        rad("⬆ Tilbake", "opp", () => (rot ? visRot() : visMappe(innhold.forelder)));
        for (const m of innhold.mapper) rad("📁 " + m.navn, "mappe", () => visMappe(m.sti));
        if (filmodus) for (const f of innhold.filer) rad("🗜️ " + f.navn, "fil", () => slutt(f.sti));
        if (!innhold.mapper.length && !(filmodus && innhold.filer.length)) liste.append(lag("p", "liten", "Mappen er tom."));
      }

      visRot();
    });
  }

  tauri.dialog.open = velg;

  // «Se bildene» åpner telefonens filbehandler i stedet for Utforsker.
  tauri.core.invoke = (kommando, argumenter, ...rest) => {
    if (kommando === "apne_mappe") {
      telefon.apneMappe(argumenter.sti);
      return Promise.resolve();
    }
    return kjerneInvoke(kommando, argumenter, ...rest);
  };
})();
