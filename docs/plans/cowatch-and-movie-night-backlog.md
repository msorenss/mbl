# Senare: medtittande, NPU-kontext, undertexter, filmkväll och Arr-länkar

Önskemål från användaren 2026-11-19. Allt går till 1.4.0 (beslut 2026-11-21).
Klart 2026-11-21: punkt 1 (skärmdumpar, även för assistenten), 2 (NPU-kontext, provad på Hailo-10H; Hailo-8 och VLM återstår), 2b (undertextsökning; autosynk återstår), 3 (gäster på filmkväll), 4 (Arr-länkar), 5 (schemalagd filmkväll; notiser i öppen flik, webbpush återstår) 6 (databasunderhåll; föräldralösa cachefiler återstår) och 7 (saknade avsnitt och filmer; grå rader i säsongslistan återstår), samt redigerbar
meny (nytt önskemål). Se AGENTS.md.

Grund som redan finns (2026-11-19, ej committad vid skrivandet): chatt mellan
tittare och AI-sällskap, lägena på/bara svar/av, dialog från undertexter i
`live_context`, MCP `read_companion_chat`. Se `docs/book/src/ai-companion.md`.

## 1. Skärmdumpar till AI:n med inställbart intervall

- Servern tar en bildruta från den pågående sessionen vid aktuell position, med
  ett intervall som tittaren väljer (till exempel av, 30 s, 1 min, 5 min).
- Bilden ska kunna hämtas av sällskapet via MCP: en resurs eller ett verktyg som
  ger senaste bildrutan, nedskalad (cirka 512–768 px) som JPEG.
- Inga bildrutor efter aktuell position, samma spoilerregel som för dialogen.
- Valet ska ligga i chattpanelen bredvid kommentarsläget. Standard är av, eftersom
  bilden lämnar servern till en extern AI.
- Återanvänd FFmpeg-extraktionen från `MediaIntelligenceService::index_item_scenes`
  och trickplay. Trickplay-arken kan räcka som billig källa.
- Kostnad: en FFmpeg-sökning per intervall och session. Begränsa på Pi:n och
  hoppa över när kodaren är hårt belastad.

## 2. NPU-analys som kontext, verifierad på Hailo-8 och Hailo-10H

- Scenanalysen (taggar, objekt, ansikten, VLM-beskrivning) ska följa med i
  `live_context`, så att AI:n kan få en textbeskrivning i stället för en bild.
- Måste provas på riktig hårdvara: Hailo-10H finns på Pi:n. Hailo-8/8L har
  inte provats fysiskt; GenAI/VLM finns bara på 10H. Hailo-8 behöver en egen
  väg (vision-HEF, YOLO-taggar).
- Acceptans: indexera ett riktigt avsnitt på varje kort och kontrollera att
  beskrivningarna stämmer med bilden, inte bara att anropen lyckas. Frigate ska
  kunna köra samtidigt.
- Fallback utan NPU ska tydligt märkas som sämre (färgheuristik) och inte
  presenteras som riktig analys.

## 2b. Nedladdning av undertexter i MBL

- Sök och ladda ned undertexter (OpenSubtitles) från titelsidan och spelaren,
  med språkval och jobbstatus. Sparas som sidofil intill mediet och dyker upp
  som spår utan omskanning.
- Hör ihop med restlistans punkt 08 (sök/nedladdning, autosync, generering).
- Valfritt: autosync (`mbl-subsync`) efter nedladdning när tiderna inte stämmer.

## 3. Filmkväll oavsett vem som är inloggad

- Matchmaker kräver idag medlemskap per konto (migration 0009). Önskemål: vem
  som helst i hushållet kan delta och skriva sitt namn, till exempel i en önskan.
- Förslag: värden skapar en filmkväll och får en delningslänk eller kod. Gäster
  anger ett namn och röstar utan konto. Gäströsten binds till en token i länken,
  inte till ett självrapporterat namn (det var felet som 0009 rättade).
- Gäster ser bara titlar som värdens profil får se (ACL och åldersgräns).

## 4. Länkar till Sonarr, Radarr och Lidarr i gränssnittet

- Visa en länk "Öppna i Radarr/Sonarr/Lidarr" på förfrågningar och titlar, men
  bara för tjänster som är konfigurerade (`radarr_*`, `sonarr_*`, `lidarr_*` i
  `settings`).
- Länka direkt till titeln när `arr_id` finns, annars till tjänstens startsida.
- Bara för admin, eftersom länken avslöjar intern adress.

## 5. Schemalägga filmkväll med notis till alla

- Från webben (och senare appen): välj tid för filmkväll. Alla deltagare får en
  notis om att rösta.
- Notiskanaler att välja mellan: webbens pushnotiser (PWA finns), WebSocket i
  öppna flikar, Home Assistant/MQTT, webhooks. Android-appen när den finns.
- Röstningen stänger vid vald tid och vinnaren kan startas på vald spelare.

## 6. Schemalagt databasunderhåll och rensning av övergivet material

- Återkommande jobb (till exempel nattligt, tid och intervall går att välja) som kör
  `PRAGMA optimize`, `wal_checkpoint(TRUNCATE)`, `VACUUM` när fragmenteringen
  är stor, och en säkerhetskopia med gallring (backup finns redan via
  `create_backup`, men inte schemalagt).
- Rensning av poster utan mediafiler: objekt vars alla delar saknas sedan en
  viss tid, tomma serier och säsonger, bilder och trickplay-, undertext- och
  nedladdningscache som inte hör till något objekt längre, gamla jobb.
- Alltid med förhandsvisning (antal och exempel) och bekräftelse första gången,
  eller en inställning "rensa automatiskt efter N dagar". Saknade filer på
  frånkopplade diskar och nodbibliotek får inte tolkas som raderade. Kräv att
  biblioteksroten är nåbar innan något tas bort.
- Låsta metadatafält, tittarhistorik och betyg ska inte försvinna för objekt som
  tillfälligt saknas. Historiken ska kunna kopplas tillbaka om filen återkommer.
- MCP: förhandsvisning och körning med `dry_run`/`confirm`.

## 7. Saknade avsnitt och filmer

- Finns i dag: titelsidan för en serie visar hål inom en säsong
  (`GET /api/v1/requests/missing-episodes/{showId}`, `MissingEpisodes.tsx`):
  har man avsnitt 1–4 och 6–8 visas avsnitt 5 som saknat.
- Saknas:
  - avsnitt efter det sista man har, och hela säsonger. Kräver TMDB:s
    avsnittslista; framtida, ännu inte sända avsnitt räknas inte.
  - en översikt per bibliotek ("Saknas") i stället för bara per serie.
  - filmer i samlingar, till exempel del 2 av en trilogi. TMDB:s samlingar.
  - en knapp "Begär" som skickar saknade avsnitt eller filmer till Sonarr/Radarr
    via befintliga förfrågningar.
  - tydlig visning i säsongslistan: saknade avsnitt som grå rader med "Saknas".
- Specialavsnitt (säsong 0) och dubbelavsnitt (`S01E05-E06`) får inte ge
  falska hål.

## Långtidslistan

### Röstchatt med sällskapet i apparna

- Gäller Android, Android TV, Quest och iOS, efter att respektive app finns.
- Tittaren pratar i stället för att skriva: mikrofonåtkomst i appen,
  tal till text (STT) och svar som tal (TTS). Paus eller sänkt volym på filmen
  medan någon pratar.
- STT kan köras lokalt: Hailo-10H har Whisper (`Speech2Text` i `mbl-hailo`).
  Annars på enheten (Android SpeechRecognizer, Apple Speech) eller via servern.
- TTS på enheten i första hand. Röstsvar ska kunna stängas av separat från
  textchatten.
- Mikrofonen bara medan tittaren håller in en knapp eller har slagit på det
  uttryckligen. Aldrig ständig avlyssning, och tydlig indikator när den är på.
- Bygger på chatten och lägena från 2026-11-19 (`POST …/messages`,
  `read_companion_chat`).
