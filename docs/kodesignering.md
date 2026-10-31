# Kodesignering med SignPath Foundation (gratis)

Uten signatur viser Windows «Windows beskyttet PCen» når noen installerer CloudJumper. SignPath Foundation signerer
åpen kildekode gratis. Sertifikatet står i navnet «SignPath Foundation», ikke vårt navn.

## Hva SignPath krever, og status

| Krav | Status |
| --- | --- |
| OSI-godkjent lisens for all kode, ingen betalt dobbeltlisens | MIT (`LICENSE`). Avhengighetene er MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0 og Unicode. XP.css-skriftene med uklar lisens er fjernet. |
| Ingen skadelig eller «potensielt uønsket» programvare | CloudJumper sletter aldri noe og har ingen reklame eller telemetri. |
| Prosjektet er aktivt og allerede utgitt i den formen som skal signeres | Gjøres med første utgivelse `v0.1.0` (usignert) etter at repoet er offentlig. |
| Signerte filer bygges automatisk fra den offentlige kildekoden | `.github/workflows/utgivelse.yml` bygger på GitHub Actions og sender filen til SignPath. |
| «Code signing policy» på prosjektsiden og nedlastingssiden | Avsnittet `Code signing policy` i `README.md`, og lenke i hver utgivelse. |
| Roller: committere, kontrollører, godkjennere | Stian har alle tre. Det er lov for et enmannsprosjekt. |
| Tofaktorinnlogging for alle i teamet, både GitHub og SignPath | Stian må slå det på (se under). |
| Personvern: ingen data sendes uten at brukeren ber om det | `PERSONVERN.md` og avsnittet i README. |
| Omdømme som kan etterprøves | Den svake delen. Se «Risiko» under. |

## Risiko

SignPath sjekker at prosjektet har et visst omdømme. Ingen fast grense er publisert. Minst ett nytt prosjekt har fått
nei fordi det manglet synlighet utenfra (brukere, omtale, nedlastinger). CloudJumper er nytt og har én utvikler, så
et avslag er godt mulig første gang. Et avslag koster ingenting, og vi kan søke igjen senere. Reserveplanen er Certum
Open Source Code Signing til 49–69 euro i året (se forskningsrapporten).

Ting som øker sjansen: en ferdig utgivelse med installasjonsfil, en ryddig README på engelsk og norsk, og noen få ekte
brukere eller nedlastinger før søknaden.

## Det Stian må gjøre

1. **Slå på tofaktorinnlogging på GitHub:** <https://github.com/settings/security> → «Two-factor authentication».
2. **Gjøre repoet offentlig** (Claude spør først): Settings → General → Danger Zone → «Change visibility» → Public.
3. **Lage første utgivelse:** Claude lager taggen `v0.1.0`. Da bygges installasjonsfilen og legges ut usignert.
4. **Sende søknaden** på <https://signpath.org/apply> (Claude spør først). Teksten står under.
5. **Når SignPath sier ja:** lag konto på SignPath med tofaktor, og legg inn i GitHub (Settings → Secrets and variables → Actions):
   - Secret `SIGNPATH_API_TOKEN`: API-token fra en «CI user» i SignPath.
   - Variabel `SIGNPATH_ORGANIZATION_ID`: organisasjons-ID fra SignPath.
   - Variablene `SIGNPATH_PROJECT_SLUG` og `SIGNPATH_SIGNING_POLICY_SLUG` bare hvis SignPath bruker andre navn enn
     `cloudjumper` og `release-signing`.
6. **For hver ny versjon:** godkjenn signeringen i SignPath når e-posten kommer. Utgivelsen legges ut av seg selv etterpå.

Signeringsjobben står av til `SIGNPATH_ORGANIZATION_ID` finnes. Til da legges utgivelser ut usignert.

## Hva som signeres

Bare installasjonsfilen (`CloudJumper_<versjon>_x64-setup.exe`, NSIS). Det er den Windows SmartScreen sjekker ved
nedlasting. Programfilen inne i installasjonen får ikke SmartScreen-sjekk, fordi den ikke er lastet ned fra nettet.
Skal den også signeres senere, kan bygget deles i to: `tauri build --no-bundle`, signer `cloudjumper.exe`, så
`tauri bundle`, og signer installasjonsfilen. Det gir to godkjenninger per versjon.

I SignPath settes «artifact configuration» til en zip med én PE-fil (`*.exe`), siden GitHub-artefakter alltid er zip.

## Søknadstekst (engelsk)

Skjemaet på <https://signpath.org/apply> ber om disse opplysningene. Lim inn:

- **Project name:** CloudJumper
- **Repository URL:** https://github.com/hippie2x210/bildesamler
- **Homepage:** https://github.com/hippie2x210/bildesamler#readme
- **Download page:** https://github.com/hippie2x210/bildesamler/releases
- **License:** MIT
- **Code signing policy URL:** https://github.com/hippie2x210/bildesamler#code-signing-policy
- **Build system:** GitHub Actions (`.github/workflows/utgivelse.yml`), Tauri 2 / Rust, NSIS installer
- **Artifacts to sign:** `CloudJumper_<version>_x64-setup.exe` (NSIS installer for Windows 10 and 11)
- **Team:** hippie2x210 (committer, reviewer and approver). MFA is enabled on GitHub.
- **Description:**

  > CloudJumper is a free, open-source Windows app that helps non-technical people, mainly elderly family members,
  > gather all their photos and videos in one place. It imports from folders, USB-connected phones, Google Photos
  > (via Google Takeout), OneDrive and Dropbox, sorts everything into Year/Month folders, puts duplicates aside, and
  > flags files with unreliable dates. It never deletes or modifies the originals, and cloud access is read-only.
  > It has no telemetry and no server of its own.
  >
  > The installer is built only by GitHub Actions from the public source. We need code signing because the target
  > users are elderly people who are (rightly) frightened by the "Windows protected your PC" warning, and the app is
  > installed on many different family PCs.

- **Reputation / users:** Fyll inn ærlig: antall nedlastinger fra Releases, og hvor mange familier som bruker appen.
