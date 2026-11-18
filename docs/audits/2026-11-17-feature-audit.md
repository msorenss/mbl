# Funktionsgranskning före apparna (2026-11-17)

Syfte: hitta funktioner som finns i API, inställningar eller dokumentation men inte fungerar från början till slut. Metod: jämförelse av OpenAPI mot webbens anrop, sökning efter funktioner som aldrig anropas, och läsning av tjänstekoden. Ersätter inte fysisk acceptans.

## Fungerar inte alls (koden finns men är inte inkopplad)

| # | Funktion | Fynd |
|---|---|---|
| A1 | Webhooks | `WebhookService::dispatch_event` anropas ingenstans. Webhooks kan skapas via API men skickas aldrig. Ingen admin-vy. |
| A2 | Radarr/Sonarr | `RequestService::forward_to_arr` anropas aldrig och det finns inga Arr-inställningar. Godkänd förfrågan leder inte till nedladdning. Sonarr kräver dessutom TVDB-ID, inte TMDB-ID. |
| A3 | DLNA/UPnP | Inställningen `dlna_enabled` sparas (standard *på*), men ingen SSDP-annonsering eller ContentDirectory startas. `mbl-remote::dlna` används bara av spelarregistret. |
| A4 | Companion-reaktioner | Reaktioner sänds på en broadcast-kanal som ingen lyssnar på; spelaren visar dem aldrig. `current_subtitle_cue` och `current_chapter` är alltid tomma. |
| A5 | Intro-/eftertextanalys | Ljudfingeravtryck för intro och svartbild/tystnad för eftertexter körs bara via ett manuellt API-anrop utan UI. Inget jobb efter skanning; "Skip Intro" visas bara när filen har kapitel. |

## Backend finns, ingen användarväg i webben

| # | Funktion | Saknas |
|---|---|---|
| B1 | Quick Connect (TV/app-inloggning) | Ingen sida där en inloggad användare godkänner koden. Blockerar app-/TV-inloggning. |
| B2 | Profilbyte | Profilsidan skickar till inloggning; PIN-verifiering (`/users/{id}/pin/verify`) används inte. |
| B3 | Fjärrkontroll | `/players/{id}/command` används inte; ingen vy för att styra en annan spelare. |
| B4 | Musik | Spår spelas i videospelaren. Ingen kö, album-/artistuppspelning, minispelare för ljud eller sångtexter (`/items/{id}/lyrics`). |
| B5 | Metadata | Inga API:er eller vyer för att redigera, låsa fält, matcha om eller välja bild, trots fältlås i databasen. |
| B6 | Matchmaker | Hela flödet (skapa, gå med, rösta) saknar webbvy. |
| B7 | AI-funktioner | Scensökning, indexering, sammanfattning (recap) och benchmark saknar webbvy. |
| B8 | Nedladdningar | Offline-API:t (för apparna) saknar webbvy; acceptabelt tills Android-klienten finns. |
| B9 | Requests | Saknade avsnitt och hardlink-analys saknar vy. |
| B10 | Live TV | Kanalredigering (`PATCH/DELETE /livetv/channels/{id}`) saknar vy. |

## Delvis

- Onlineproviders: YouTube och Spotify är märkta som demo och kan inte spela. WebTV/WebDrama fungerar.
- Virtuella kanaler: kvar enligt restlistan (biblioteksval, övergång mellan objekt).
- Restlistans punkter 22–24 (språk, tillgänglighet, inställningar) är oförändrade.

## Inaktuellt i `remaining-work.md`

- 03 Workers/noder: riktig offload och lagringsnoder implementerade 2026-11-17.
- 11 Home Assistant: MQTT-livscykel och kommandon implementerade 2026-11-01.
- 16 Requests: TMDB-sökning finns; Arr-leverans saknas (A2).
- 19 Fjärråtkomst: diagnostik med verkliga kontroller 2026-11-17.

## Åtgärdat 2026-11-17

- B1 Quick Connect: godkännandesida `/connect`, lookup-steg, engångstoken, ingen session före claim, tak för väntande koder.
- B2 Profilbyte: PIN-baserat byte till hanterade profiler, gammal session avslutas, PIN-spärr; `hasPin` var alltid `false` i profillistan.
- A5 Intro/eftertexter: bakgrundsjobb efter skanning med `marker_analysis` (migration 0018); FFmpeg avbryts när jobbet avbryts.
- A1 Webhooks: händelser för uppspelning och skanning, leverans med signatur och omförsök, testanrop och admin-vy.

## Åtgärdat 2026-11-18

- A2 Radarr/Sonarr: inställningar med anslutningstest (rotmapp och kvalitetsprofil hämtas från tjänsten), godkännande lägger till titeln och startar sökning, Sonarr slås upp via `tmdb:`, redan tillagd titel läggs inte till igen, fel lämnar förfrågan väntande med orsak, förfrågan blir tillgänglig när titeln finns i ett bibliotek, avslå-knapp. Den gamla `forward_to_arr` skickade fel fält (`searchForMissingMovie`, TMDB-ID till Sonarr) och anropades aldrig; borttagen.
- Lidarr: förfrågningar för artist och album (migration 0019: `musicbrainz_id`, `artist`), musiksökning via Lidarrs lookup, tillägg via API v1 med metadataprofil. Album läggs till med artisten omonitorerad så att bara albumet hämtas. Tillgänglig matchas på namn eftersom skannern bara sparar MusicBrainz-ID för spår.
- Profilbyte: PIN fungerar även för vanliga konton (inte administratörer) som fått en PIN. Tidigare gick sådana profiler till inloggningssidan.
- Översättningar: alla synliga texter (cirka 1 220 plus mallar med värden) är översatta till 22 språk; ord-för-ord-översättning ger inte längre halvöversatta meningar. `web/src/i18n/coverage.test.ts` extraherar texterna ur källkoden (`web/scripts/i18n-extract.mjs`) och fallerar vid saknad översättning.

## Åtgärdat 2026-11-18 (kväll)

- A3 DLNA: SSDP på UDP 1900 (startar och stoppar med inställningen), enhets- och tjänstebeskrivningar, ContentDirectory Browse med sidindelning, ConnectionManager, GENA-prenumerationer som accepteras, media med Range och DLNA-huvuden. Admin väljer bibliotek och åldersgräns; av som standard; bara LAN-klienter (peer-adress). Ingen omkodning, omslagsbild eller undertext ännu. Inte provat mot fysisk TV.

- B5 Metadata: redigering av fält med lås (låsen gäller även genrer och vald bild vid automatisk matchning), ommatchning mot vald TMDB-post och val av affisch/bakgrund bland TMDB:s bilder. Musikens metadata redigeras inte separat.
- B4 delvis: sångtexter i spelaren (synkade följer uppspelningen). Kö, album-/artistuppspelning och minispelare för ljud återstår.

## Föreslagen ordning

1. B1 Quick Connect och B2 profilbyte (krävs för apparna).
2. A5 intro/eftertexter som bakgrundsjobb.
3. A1 webhooks och A2 Radarr/Sonarr (klart).
4. B4 musik och B5 metadata.
5. A3 DLNA (eller dölj inställningen tills den finns), A4 Companion, B3 fjärrkontroll.
6. B6/B7 Matchmaker och AI-vyer.
