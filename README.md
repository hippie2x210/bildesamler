# CloudJumper

Samler bilder og videoer fra mange mapper på ett sted, sorterer dem i `År/Måned`, og legger duplikater til side. **CloudJumper sletter aldri noe**: originalene blir liggende, og alt kopieres.

## Slik blir samlingen

```
Samling/
  Bilder/2019/07/IMG_1234.JPG
  Videoer/2023/03/VID_20230317_084500.mp4
  Duplikater/2019/07/IMG-20190704-WA0001.jpg   (dårligere kopier av bilder som er beholdt)
  Uten dato/bilde1.jpg                          (ingen dato vi kan stole på)
  _Angret/…                                     (kopier som er angret, flyttes hit)
  .bildesamler/logg/…                           (logg over hver kopiering, brukes av «Angre»)
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

Originalen er den med sikrest dato, deretter kameradata, GPS, oppløsning og filstørrelse. Du kan velge en annen i fanen «Duplikater».

## Utvikling

| Mappe | Innhold |
| --- | --- |
| `kjerne/` | Rust-biblioteket som gjør alt arbeidet |
| `app/ui/` | Grensesnittet (HTML/JS med [XP.css](https://github.com/botoxparty/XP.css)) |
| `app/src-tauri/` | Tauri-skallet som kobler grensesnittet til kjernen |
| `testdata/` | Testbilder og `fasit.csv`, laget av `tools/lag_testbilder.py` |

```sh
cargo test -p bildesamler-kjerne --release   # test mot fasiten
cd app && npm ci && npx tauri dev            # kjør appen
cd app && npx tauri build                    # lag Windows-installasjon
```

GitHub Actions tester på Linux og Windows og legger installasjonsfilen ved hver kjøring under «Artifacts».
