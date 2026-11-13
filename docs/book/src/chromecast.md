# Chromecast och Remote Playback

Webbspelaren använder Google Cast Web Sender SDK med Default Media Receiver
(`CC1AD845`) och webbläsarens Remote Playback API som fallback. MBL har ingen
egen receiver. För verifierade enheter, kända begränsningar och exakt
felsökningsflöde, se [Cast-driftguiden](../../audits/cast-runbook.md).

**Läge 2026-11-12:** Bedroom display spelar den testade filen vid 2 Mbps,
även efter textspårsbyte. Automatisk fungerar ännu inte enligt användaren.
Använd 2 Mbps som tillfällig inställning för denna fil; det är ingen garanti
för alla filer eller mottagare. Automatisk är inte samma sak som adaptiv HLS.

## Användning

Öppna MBL på en adress mottagaren kan nå, starta en titel och välj Cast-knappen.
Avbrutet mottagarval återställer tidigare lokalt spelläge. Play/pause, sökning,
volym och mute styr aktiv mottagare. Progress och avslut använder mottagartid.

Google Cast omförhandlar med serverns konservativa `chromecast`-profil.
Svar innehåller MIME-typ, medieversion och undertextlänkar. Externa textspår
skickas till SDK:t och kan väljas i spelaren. Ljudspår hanteras genom serverns
omförhandling. **Audio track** kan bytas under uppspelning på Google Cast och
bevaras vid nästa ljudsynk-/spårbyte. Inbrända undertexter omförhandlas i videon
och skickas inte som externa SDK-textspår. Ljudsynk mellan −10 och +10 sekunder justeras i serverns ljudström
(negativt tidigarelägger, positivt fördröjer) och följer med till Google Cast.
Ändring använder mottagarens position och bevarar pausläget; ljudet kodas om,
medan kompatibel video kopieras. Vid uttryckligt kapacitetsfel frigör den aktuella arbetskopian tidigare
session och provar en gång till. Misslyckad ersättning efter frigjord
kapacitet återställer lokal uppspelning vid sparad position. Om en sparad ljudsynk hindrar den första starten kan
du välja **Retry without audio sync** i felvyn; ditt val av återupptagning eller
start från början bevaras. Native Remote Playback använder befintlig källa och
reglaget för att ändra ljudsynk är därför spärrat under sådan uppspelning.
Lokala reglage för hastighet och nattläge är spärrade under casting.
Google Cast kan byta kvalitetsgräns genom ny serverförhandling.

Google Cast väntar också på serverns första färdiga HLS-data före laddningen.
Vid HLS-byte används serverns förberedda position.
Utan kapacitetsöverlämning kan förhandlingsfel behålla tidigare session; detta verifierar inte mottagarens
åtkomst till adressen eller dess avkodare.

Webbspelaren förnyar också en pausad Cast-session med mottagarens position.
Om webbläsaren eller enheten sover längre än serverns timeout kan sessionen
ändå städas. Då visas ett meddelande om att koppla från och ansluta mottagaren
igen. Fysisk bakgrunds-/sömnacceptans återstår.

## Ljudsynk

Ljudsynk kräver en ny serversession och kan ge en kort laddning vid bytet.
Inställningen sparas efter accepterat byte och följer även lokala kvalitetsbyten.
Videor utan ljud fortsätter spelas med effektiv ljudsynk 0 ms. Statistikpanelen
visar serverns accepterade värde. Lokal WebAudio används för nattläge och lägger
inte på ytterligare fördröjning.

Implementationen använder FFmpegs
[atrim och asetpts](https://ffmpeg.org/ffmpeg-filters.html#atrim) för att bevara
videons tidslinje också vid sökning och omstart av HLS-processen. Fysisk
läppsynk på mottagare återstår att verifiera.

## Up Next

Native Remote Playback använder befintlig källa och kan inte starta nästa
avsnitt genom serverförhandlingen. Nästa-reglaget är spärrat tills mottagaren
kopplats från; aktuell session stängs inte av ett sådant försök.

Under slutet av ett avsnitt visas nästa tillgängliga avsnitt, även över
säsongsgränser. **Play next episode** startar det på ansluten Google-mottagare.
Kryssa i **Automatically play next episode** för automatisk fortsättning i den
aktuella uppspelningskedjan. Standard är avstängt; valet är inte en global
kontoinställning och kräver att webbspelaren förblir öppen.

Automatisk fortsättning kräver att mottagarens aktuella medieladdning först
rapporterat uppspelning och sedan `IDLE` med orsaken `FINISHED`. Paus, buffring,
stopp, avbrott och fel utlöser inte nästa avsnitt. Sena händelser från tidigare
laddningar ignoreras. Den avslutade serversessionen stängs innan nästa startas,
så dess transkodningsplats kan återanvändas. Ett misslyckat byte kräver återförsök
från användaren; klienten gör ingen obegränsad automatisk retry.

Native Remote Playback har fortfarande manuell styrning och saknar automatisk
receiverfortsättning i MBL. Dess disconnect-knapp öppnar webbläsarens väljare.
Stöd för skärmtekniker beror på webbläsare/operativsystem.

## Nätverk och media

- Google Cast använder serverns `public_url` när den är konfigurerad, annars
  webbläsarens origin. Loopback-adresser avvisas eftersom de pekar på mottagaren.
- Serveradress, certifikat och codec måste stödjas av mottagaren. Konservativ
  profil garanterar inte kompatibilitet med varje modell.
- Mediarouter har CORS/preflight för direktström, HLS och textspår. Signerade
  medielänkar och svårgissade HLS-adresser används; CORS ändrar inte API-behörigheter.

## Spelarregister och kommandon

Närvaron är bunden till användare och autentiseringssession. Webbläsaren öppnar
en WebSocket-kommandokanal för sin registrerade spelare. Administratörer kan
kontrollera spelare; vanliga användare ser sina egna. Offline-spelare ger fel.
Ett accepterat kommando betyder köat till klienten, inte bekräftad effekt på TV:n.

Frånkoppling städar prenumerationer och köar avregistrering efter heartbeat.
Närvaro löper ut efter 60 sekunder utan uppdatering. MQTT-/webhookpublicering
är separata integrationspunkter som inte följer automatiskt av Cast-stödet.

## Verifiering

SDK-dubblar och renderade tester täcker bland annat slutorsaker, dubbla/sena
händelser, städning av medielyssnare, automatisk fortsättning, nollvolym,
avbrutna anslutningar och korrekt progress/stop. HTTP-regressioner täcker
ägarisolering, transport, kapacitet och media-CORS.

Vissa fysiska tester är bekräftade enligt Cast-driftguiden. Full acceptans
med representativa filer, seek/resume, undertexter, ljudspår, nätavbrott
och fortsättning över flera avsnitt återstår.
Se [granskningsrapporten](../../audits/2026-10-27-web-and-service-audit.md).

SDK-referenser: [Media och uppdateringslyssnare](https://developers.google.com/cast/docs/reference/web_sender/chrome.cast.media.Media),
[slutorsaker](https://developers.google.com/cast/docs/media/messages),
[mediestöd och CORS](https://developers.google.com/cast/docs/media).

## Cast-diagnostik i spelaren

Öppna **📊 → Cast-diagnostik** (eller tryck `s` för statistik) och välj
**Kopiera Cast-logg**. Om urklipp inte är tillgängligt kan textfältet markeras
och kopieras manuellt. Vyn finns även när mottagaren kopplats ned.

Loggen visar TV-val, serverförhandlingens HTTP-status, mediaformat, startposition,
LOAD-begäran och kvittens, mottagarens `isConnected`, `isMediaLoaded`,
`playerState`, position och längd, medieuppdateringar/slutorsaker, kommandon,
heartbeat och kommandokanalens anslutning. SDK-fel visar tillgängliga felkoder
utan att serialisera SDK-objekt. Ett försök som väntar i 20 sekunder får en
särskild väntemarkering; detta ändrar inte uppspelningsbeteendet.

Tiderna anges både som UTC och millisekunder från försökets början. En accepterad
LOAD är separat från mottagarens PLAYING; även PLAYING kan behöva jämföras med
om positionen faktiskt avancerar. Medielänkar maskerar query/fragment och HLS-/
live-sessionernas capability-ID. Inga cookies, bearer tokens, lösenord, titlar
loggas av diagnostiken. Mottagarens namn loggas i `receiver.selected`.

Loggen ligger endast i den aktuella sidans minne, utan uppladdning eller
beständig lagring. Den begränsas till 200 händelser: början bevaras tillsammans
med de senaste händelserna. Ett nytt Cast-försök ersätter det gamla; omladdning
av sidan tömmer loggen. **Kopiera därför efter felet och före nästa försök eller
omladdning.** Rapporten innehåller fortfarande tekniska uppgifter som serverns
värd/adress och item-ID; granska den innan du delar den.

Detta är klientdiagnostik, inte fullständig HTTP-/FFmpeg-/mottagartelemetri.
Serverns vanliga loggar och spelarstatistik kompletterar tidslinjen. MCP kan
inte läsa en privat webbläsares minneslogg; delning sker genom explicit kopiering.

## HLS med kopierad Cast-video

När Chromecast kan använda videocodecen men behöver ompaketering eller annat
ljud använder MBL FFmpegs verkliga segmentlista. Nyckelbilderna i originalfilen
avgör segmentlängderna; servern anger därför inte en uppskattad 3/6-sekunderslista
för det här flödet. Mottagarens init-fil och segmentens tidsstämplar behålls under
hela sessionen, även om mottagaren först hämtar början och sedan återupptar längre
in i avsnittet.

Servern förbereder segment fram till återupptagningspunkten med en liten marginal.
En sen start eller sökning kan därför ta längre tid och använda mer temporärt
utrymme: den kopierade videon och det konverterade ljudet före målpunkten behöver
indexeras. Förberedelsen begränsas till 60 sekunder och sessionerna omfattas av
den ordinarie cachekvoten. Vid förhandlingsfel behålls normalt befintlig källa, men inte om den redan
stängts för att frigöra transkodningskapacitet. Spelarens sökknappar förbereder en ny
Cast-källa vid behov. Direkt uppspelning och native Remote Playback använder
sina ordinarie sökflöden.

Det här gäller kopierad Cast-video. Full videoomkodning, den vanliga webbspelarens
HLS-flöde och en fullständig slumpmässigt sökbar segmentindexering har separata
flöden och kräver fortsatt verifiering. Fysisk TV-acceptans krävs utöver server-
och browsertester.
