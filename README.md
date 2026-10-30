# CloudJumper

Samler bilder og videoer fra mange mapper på ett sted, sorterer dem i `År/Måned`, og legger duplikater til side. **CloudJumper sletter aldri noe**: originalene blir liggende, og alt kopieres.

*English:* CloudJumper is a free, open-source Windows app that gathers photos and videos from folders, phones, Google Photos (via Takeout), OneDrive and Dropbox into one collection sorted by year and month, with duplicates set aside. It never deletes anything. Licensed under the [MIT License](LICENSE).

## Last ned

Den nyeste installasjonsfilen ligger under [Releases](../../releases/latest). Dobbeltklikk på `CloudJumper_…_x64-setup.exe` for å installere. Se også [Code signing policy](#code-signing-policy) og [Personvern](PERSONVERN.md).

## Slik blir samlingen

```
Samling/
  Bilder/2019/07/IMG_1234.JPG
  Videoer/2023/03/VID_20230317_084500.mp4
  Duplikater/2019/07/IMG-20190704-WA0001.jpg   (dårligere kopier av bilder som er beholdt)
  Uten dato/bilde1.jpg                          (ingen dato vi kan stole på)
  _Angret/…                                     (kopier som er angret, flyttes hit)
  .cloudjumper/logg/…                          (logg over hver kopiering, brukes av «Angre»)
  .cloudjumper/takeout/…                       (Google Takeout pakket ut; sorterte bilder flyttes herfra, zip-filen røres ikke)
```

## Hvordan datoen velges

1. Opptaksdato i EXIF (bilder) eller i videofilen. Umulige datoer avvises: fremtiden, før 1990, og nullstilte kameraklokker som 2000-01-01 00:00.
2. Dato i filnavnet, for eksempel `IMG_20190704_120000`, `IMG-20190704-WA0001` eller `Screenshot_20200310-143005`.
3. Ellers havner filen i `Uten dato`. Fildatoen brukes aldri automatisk, fordi den ofte er nedlastingsdatoen.

Er EXIF og filnavn uenige med mer enn et døgn, vises en advarsel.

## Hvordan duplikater finnes

- **Helt like filer** gjenkjennes på SHA-256.
- **Samme bilde i annen størrelse eller kvalitet** (for eksempel fra WhatsApp) gjenkjennes med en perseptuell hash.
- Serieopptak med ulik opptakstid og beskårne versjoner regnes ikke som duplikater.
- Bildene som allerede ligger i samlingen fra en tidligere kjøring, er med i sammenligningen. Et bilde som finnes fra før, havner i `Duplikater` (eller hoppes over hvis det er helt likt), ikke i `Bilder` en gang til. Det som er lest, huskes i `.cloudjumper/samling.json`.

Originalen er den med sikrest dato, deretter kameradata, GPS, oppløsning og filstørrelse. Du kan velge en annen i fanen «Duplikater».

## Utvikling

| Mappe | Innhold |
| --- | --- |
| `kjerne/` | Rust-biblioteket som gjør alt arbeidet |
| `app/ui/` | Grensesnittet (HTML/JS med [XP.css](https://github.com/botoxparty/XP.css)) |
| `app/src-tauri/` | Tauri-skallet som kobler grensesnittet til kjernen |
| `testdata/` | Testbilder og `fasit.csv`, laget av `tools/lag_testbilder.py` |

```sh
cargo test -p cloudjumper-kjerne --release   # test mot fasiten
cd app && npm ci && npx tauri dev            # kjør appen
cd app && npx tauri build                    # lag Windows-installasjon
```

GitHub Actions tester på Linux og Windows og legger installasjonsfilen ved hver kjøring under «Artifacts».

Når en tagg som `v0.1.0` skyves, bygger `.github/workflows/utgivelse.yml` installasjonsfilen, får den signert og legger den ut under «Releases». Se [docs/kodesignering.md](docs/kodesignering.md).

## Personvern

CloudJumper sender ingen informasjon til andre maskiner på nettet, med mindre du selv ber om det, for eksempel ved å logge inn på Google Foto, OneDrive eller Dropbox for å hente dine egne bilder. Les mer i [PERSONVERN.md](PERSONVERN.md).

## Code signing policy

Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org).

- Only the Windows installer built by GitHub Actions from this repository's source code ([`.github/workflows/utgivelse.yml`](.github/workflows/utgivelse.yml)) is signed. Nothing built elsewhere is signed.
- Committers and reviewers: [hippie2x210](https://github.com/hippie2x210). Changes from anyone else are reviewed by a committer before they are merged.
- Approvers: [hippie2x210](https://github.com/hippie2x210). Every signing request is approved by hand.
- All team members use multi-factor authentication on GitHub and SignPath.
- Privacy: This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it. See [PERSONVERN.md](PERSONVERN.md).

## Lisens

[MIT](LICENSE). Tredjepartskode: [XP.css](https://github.com/botoxparty/XP.css) (MIT, se `app/ui/vendor/XP.css.LICENSE`), Tauri og Rust-biblioteker under MIT, Apache-2.0 og lignende åpne lisenser. XP.css sine skrifttyper er tatt ut; appen bruker Windows' egen Tahoma.
