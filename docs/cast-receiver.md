# MBL Custom Web Receiver — P3

Status 2026-11-13: receiver och senderkoppling implementerade lokalt. Registrerat
app-ID, registrerade testenheter, hosting och fysisk acceptans återstår.
Det här dokumentet beskriver ett testbygge, inte verifierad Cast-kompatibilitet.

## Innehåll och flöde

- `web/cast-receiver/index.html` laddar CAF v3 och en separat Vite-entry.
- `web/src/castReceiver.ts` startar CAF:s vanliga `cast-media-player`. CAF
  hanterar LOAD, paus, sökning, HLS/fMP4 och externa textspår. MBL skickar inte
  egna experimentella fMP4-hints och byter inte underliggande spelarmotor här.
- `web/src/utils/castReceiver.ts` svarar på P2:s nonce-bundna capability-fråga.
  Sju fasta upplösning/fps/nivå-kombinationer provas med Baseline, Main och High.
  Ingen modell gissas från enhetens namn. Proberna använder
  `canDisplayType("video/mp4", codec, width, height, framerate)`; ett rent
  MIME-besked räcker inte för att öppna högre kvalitetsnivåer.
- `getDeviceCapabilities().display_supported === false` identifierar audio-only.
  Okänd displaykapabilitet ger inga positiva videolägen och därmed fallback.
- AAC/MP3 provas separat. HLS/fMP4-transporten kommer från CAF, inte från
  en påstådd hårdvaruprofil. Ett positivt API-besked bevisar inte långvarig
  kodnings-/avkodningsprestanda eller felfri nätverksleverans.
- Rapporten går endast till frågande sender. Högst fyra frågor per sekund;
  inga köer eller bakgrundsprober. P2 kräver aktuell session, nonce och revision.
- Telemetri använder `urn:x-cast:com.mbl.telemetry`: ERROR, BUFFERING,
  BITRATE_CHANGED, PLAYING, PAUSE och PLAYER_LOAD_COMPLETE. Endast numeriska
  felkoder/bitrate, buffringsflagga och faktisk videogeometri om tillgänglig.
  Inga URL:er, request headers, token eller råa SDK-objekt skickas till loggen.
  Sendern visar detta som `receiver.caf.*` i befintlig Cast-diagnostik.

Receivern importerar inte MBL:s API-klient, inloggning eller admin-UI. Endast
redan signerade media-URL:er överlämnas i vanliga Cast LOAD. Ingen admin-token
behövs för receiversidan. Strömrutternas befintliga capability/ACL-kontroll och
CORS gäller även en egen receiver-origin; de har inte öppnats ytterligare.

## Bygg och paketera

Från repositoryroten:

```sh
XDG_RUNTIME_DIR=/tmp pnpm -C web build
node web/scripts/package-cast-receiver.mjs /tmp/mbl-cast-receiver
```

Paketet innehåller `/cast-receiver/index.html` och enbart dess beroenden under
`/assets/`. Servera paketets rot; flytta inte bara HTML-filen. Samma receiversida
ingår i MBL-serverns inbäddade webbbygge efter serverombyggnad. HTML ska använda
`Cache-Control: no-cache`; hashade assets kan vara immutable. Receiversidan
undantas från PWA:s app-shell-cache. Vite target `chrome80` undviker ny syntax
som äldre Cast-runtime inte kan parsa.

## Registrering och testhosting

1. Logga in i [Google Cast SDK Developer Console](https://cast.google.com/publish/).
   Kontoägaren ansvarar för registrering och eventuell registreringsavgift.
2. Registrera en **Custom Receiver** med den exakta receiver-URL:en. För
   registrerade utvecklingsenheter tillåter Google HTTP under utveckling.
   Ett möjligt LAN-testmål efter installation är
   `http://192.168.68.147:8420/cast-receiver/index.html`.
   Den gamla installerade binären innehåller inte automatiskt denna sida.
3. Registrera Bedroom display och Bedroom TV som testenheter enligt Googles
   anvisningar. Android TV använder Cast-programvarans serienummer. Spara inte
   serienummer eller kontouppgifter i Git. Följ konsolens instruktioner om
   omstart/aktivering innan första försöket.
4. För publik distribution krävs stabil HTTPS-hosting och betrott certifikat.
   Ett självsignerat certifikat på Pi:n blir inte betrott på TV:n genom att
   användaren accepterar det i sin vanliga webbläsare. HTTP-devhosting är
   endast för det lokala registrerade testet; inte publik produktion.
5. Kopiera app-ID:t till Administration → Serverinställningar → Cast receiver
   application (advanced). Detta är en lokal webbläsarinställning, inte en
   hårdvarumodell. Koppla från Cast före byte; knappen laddar om sidan.
   För leverans till alla användare bygg med `VITE_MBL_CAST_APP_ID=<app-ID>`.
6. Standardreceiver kan väljas uttryckligen som kompatibilitetsläge. Den ger
   inte MBL-rapporten och får 720p30-fallback. MBL gör inget dolt byte tillbaka
   om custom-appen inte kan startas. Ett appbyte kräver ny anslutning.

Registrering/publicering i Googles konsol utfördes inte av denna kodändring.
Ingen automatiskt vald hög profil kan fysiskt verifieras innan stegen ovan är klara.

## Fysisk acceptans (återstår)

Kör samma källfil på båda enheterna; spara separata loggar med enhetsnamn och
byggidentitet. Bekräfta först att `receiver.application` visar custom-app-ID,
sedan `receiver.profile` med `source: receiver-probe` före `server.negotiate`.

- Bedroom display: Auto ska stanna inom den rapporterade 720p-kombinationen.
- Bedroom TV: kompatibel högre kombination ska väljas utan modellval.
- Prova Auto och 2 Mbps från noll och sparad position. Kontrollera bildrörelse,
  ljud/bildsynk, undertexter, paus från mottagaren och nästa avsnitt.
- Byt mottagare under förberedelse och efter anslutning; tidigare profil/svar
  får inte användas på den nya. Prova nekad/utebliven capability-kanal.
- Logga demux-/decode-felkoder, faktisk bitrate och buffring. Positiv probing
  är inte acceptans av själva mediet. Auto-felet är inte löst förrän fysisk
  uppspelning och korrekt serverutdata verifierats.
- P4:s adaptiva flernivåstege för nya profiler är fortfarande separat arbete.

## Primärkällor

- [Google: Custom Receiver](https://developers.google.com/cast/docs/web_receiver/basic)
- [Google: CastReceiverContext och canDisplayType](https://developers.google.com/cast/docs/reference/web_receiver/cast.framework.CastReceiverContext)
- [Google: registrering och utvecklingsenheter](https://developers.google.com/cast/docs/registration)
- [Google: receiverhändelser](https://developers.google.com/cast/docs/reference/web_receiver/cast.framework.events)

## Lokal verifiering 2026-11-13

269 webbtester, TypeScript, produktionsbygge och bundle-budget 147,47 KB gzip
passerade. Sex serverintegrationstester passerade, inklusive receiversidans
HTML, oautentiserade åtkomst och no-cache-header. Receiverpaketet skapades med
tre assetberoenden. Inga fysiska mottagarresultat ingår i dessa kontroller.
