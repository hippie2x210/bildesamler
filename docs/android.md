# CloudJumper på Android

CloudJumper finnes også som app for Android-telefoner (Android 7 eller nyere). Den gjør det samme som på PC:
leser bildene, finner riktig dato og like bilder, og **kopierer** dem til en samling sortert etter år og måned.
Den sletter aldri noe.

## Installere

1. Åpne GitHub på telefonen og logg inn (repoet er privat):
   **Releases → «CloudJumper for Android (siste testbygg)» → CloudJumper.apk**.
   Alternativt: last ned `CloudJumper-Android` fra siste kjøring under **Actions** på PC-en og flytt APK-en til telefonen.
2. Trykk på den nedlastede filen. Første gang spør telefonen om nettleseren får «installere ukjente apper». Si ja.
3. Google Play Protect kan si at appen er ukjent. Velg **Installer likevel** (appen er ikke lagt ut i Play-butikken).
4. Åpne CloudJumper. Den ber om **Tilgang til alle filer**. Trykk «Gi tilgang», slå på bryteren og trykk tilbake.

Oppdatering: last ned en nyere APK og installer over den gamle. Bildene og innstillingene blir liggende.

## Bruke appen

* **Bildene på telefonen**: velg en mappe, for eksempel `DCIM` (kameraet), `Pictures` eller `WhatsApp`, eller en USB-disk.
* **Google Foto**: knappene åpner Google Takeout i nettleseren. Filene havner i *Nedlastinger*, og «Sorter bildene i
  Nedlastinger» pakker dem opp og sorterer dem.
* **Hvor skal bildene samles?** Er en USB-disk eller et minnekort koblet til, foreslår CloudJumper den.
  Ellers `Pictures/CloudJumper` på telefonen. En ekstern disk er best, for samlingen tar like mye plass som bildene.
* Skjermen holdes på mens appen jobber, så telefonen ikke stopper kopieringen.

## Ekstern disk på telefonen

* Koble til med en USB-C-minnepinne eller en disk med OTG-adapter. Disken må være formatert som **exFAT** eller
  **FAT32**. NTFS (vanlig på Windows-disker) kan de fleste telefoner ikke skrive til.
* En vanlig harddisk uten egen strøm kan trekke mer strøm enn telefonen gir. Bruk en SSD, en minnepinne eller en disk med strømadapter.

## Hvorfor «Tilgang til alle filer»?

Android gir ellers bare apper tilgang til bilder én og én, og ikke til USB-disker som vanlige mapper. Med denne
tilgangen kan CloudJumper lese hele kameramappen og skrive til en ekstern disk, akkurat som på PC.
Appen sender ingenting noe sted (bortsett fra når du selv åpner Google Takeout i nettleseren).
Tillatelsen er grunnen til at appen ikke kan ligge i Play-butikken uten en egen godkjenning fra Google.

## Signering (for den som bygger)

GitHub Actions bygger APK-en i jobben «Android-app (APK)». Med hemmelighetene `ANDROID_KEYSTORE` (nøkkelfila som base64)
og `ANDROID_KEYSTORE_PASSORD` signeres hver versjon med samme nøkkel, så telefoner kan oppdatere. Uten dem signeres
APK-en med en tilfeldig testnøkkel: den kan installeres, men må avinstalleres før neste versjon kan installeres.
Lokalt: `cd app && npx tauri android build --apk` (krever Android SDK og NDK, se ci.yml).

## Ikke ferdig ennå på telefonen

* Gmail-sjekken for Takeout-e-posten (finnes på PC).
* Henting fra OneDrive og Dropbox (kommer i steg 2, samme som på PC).
* iPhone.
