# Chromecast och Remote Playback

Webbspelaren använder Google Cast Web Sender SDK med Default Media Receiver
(`CC1AD845`) och webbläsarens Remote Playback API som fallback. MBL har ingen
egen receiver. Fysisk mottagaracceptans återstår.

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
medan kompatibel video kopieras. Saknas ledig HLS-kapacitet behålls tidigare
session och inställning. Om en sparad ljudsynk hindrar den första starten kan
du välja **Retry without audio sync** i felvyn; ditt val av återupptagning eller
start från början bevaras. Native Remote Playback använder befintlig källa och
reglaget för att ändra ljudsynk är därför spärrat under sådan uppspelning.
Lokala reglage för hastighet, nattläge och kvalitet är spärrade under casting.

Google Cast väntar också på serverns första färdiga HLS-data före laddningen.
Vid spår- och synkbyte används mottagarens senaste position efter väntan.
Startfel behåller den tidigare sessionen; detta verifierar inte mottagarens
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

Fysisk acceptans med representativa filer, seek/resume, undertexter,
ljudspår, nätavbrott och fortsättning över flera avsnitt återstår.
Se [granskningsrapporten](../../audits/2026-10-27-web-and-service-audit.md).

SDK-referenser: [Media och uppdateringslyssnare](https://developers.google.com/cast/docs/reference/web_sender/chrome.cast.media.Media),
[slutorsaker](https://developers.google.com/cast/docs/media/messages),
[mediestöd och CORS](https://developers.google.com/cast/docs/media).
