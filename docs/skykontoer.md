# OneDrive og Dropbox: engangsoppsett

CloudJumper trenger en gratis «app-ID» fra Microsoft og en gratis «app-nøkkel» fra Dropbox. Begge settes opp én gang,
av den som bygger appen, ikke av hver bruker. Ingen av dem er hemmelige, og ingen av dem koster noe.
CloudJumper ber bare om lesetilgang: appen kan ikke endre eller slette noe i skyen.

Når ID-ene finnes, vises knappene «OneDrive» og «Dropbox» på startsiden. Uten dem er knappene skjult.

## OneDrive (Microsoft Azure)

1. Gå til <https://portal.azure.com> og logg inn med en vanlig Microsoft-konto (for eksempel Outlook/Hotmail).
   Første gang må du kanskje opprette en gratis Azure-konto. Den krever ikke betalingskort for app-registrering.
2. Søk etter **App registrations** (Appregistreringer) og trykk **New registration**.
3. Navn: `CloudJumper`.
4. Supported account types: **Personal Microsoft accounts only**.
5. Redirect URI: velg **Public client/native (mobile & desktop)** og skriv `http://localhost`.
6. Trykk **Register**. Kopier **Application (client) ID**.
7. Under **Authentication**: sett **Allow public client flows** til **Yes**, og lagre.
8. Under **API permissions**: sjekk at bare `Files.Read` og `offline_access` (Microsoft Graph, Delegated) står der.

## Dropbox

1. Gå til <https://www.dropbox.com/developers/apps> og logg inn med en Dropbox-konto.
2. Trykk **Create app**. Velg **Scoped access** og **Full Dropbox**. Navn: for eksempel `CloudJumper-<ditt navn>`.
3. Under **Permissions**: kryss av for `account_info.read`, `files.metadata.read` og `files.content.read`. Ingen andre. Trykk **Submit**.
4. Under **Settings** → **OAuth 2** → **Redirect URIs**: legg til disse tre, én om gangen:
   - `http://localhost:53682/`
   - `http://localhost:53683/`
   - `http://localhost:53684/`
5. **Allow public clients (Implicit Grant & PKCE)**: **Allow**.
6. Kopier **App key**. App secret trengs ikke og skal ikke brukes.
7. En ny Dropbox-app er i «Development»-modus og tillater opptil 500 brukere. Det holder for familie og venner.

## Legge ID-ene inn i appen

Enklest: legg dem inn som GitHub-secrets i repoet, så bygges de inn i installasjonsfilen:

- `CLOUDJUMPER_ONEDRIVE_KLIENT` = Application (client) ID fra Azure
- `CLOUDJUMPER_DROPBOX_NOKKEL` = App key fra Dropbox

For å teste uten nytt bygg kan ID-en i stedet legges i en tekstfil ved siden av `cloudjumper.exe`
(`%LOCALAPPDATA%\CloudJumper\`): `onedrive-klient.txt` eller `dropbox-nokkel.txt`, med bare ID-en i filen.
