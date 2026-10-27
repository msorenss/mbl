# Kod- och gränssnittsgranskning 2026-10-27

## Omfattning och slutsats

Granskning av befintlig Rust-server och SolidJS-webb mot `IMPLEMENTATION.md`, främst M2/M3/M6–M10. Native Android/TV/VR-klienter (M11) ingår inte. Dokumentationens `[x]` och testernas milstolpsnamn är inte bevis på fungerande produktflöden. Ingen fullständig säkerhetscertifiering eller hårdvaru-/nätverkstest görs i denna granskning.

Webben har en användbar grund för bibliotek och uppspelning, men **hela den specificerade webbfunktionaliteten finns inte**. Flera senare milstolpar består av API/datamodeller eller fristående hjälpfunktioner utan inkopplad användarresa. Det krävs fortsatt implementation och end-to-end-verifiering innan de kan beskrivas som färdiga.

## Åtgärdade fynd i denna arbetsomgång

| Fynd | Konsekvens före ändring | Ändring / verifieringspunkt |
|---|---|---|
| Samlingars och offentliga spellistors objekt lästes utan bibliotek-/åldersfilter | Begränsade användare kunde se skyddade objekts ID och antal; lägga till otillåtna ID i egen spellista | Gemensam SQL-filtrering via `ItemFilter`, filtrerade antal och kontroll vid tillägg. HTTP-regressioner i `crates/mbl-server/tests/audit.rs`. |
| Ny TOTP-setup återställde redan aktiverad 2FA | Setup-anrop stängde av faktorn och ersatte hemligheten | Avvisas med 409 när faktorn är aktiv. Test att gammal kod fortfarande fungerar och lösenord ensamt fortfarande nekas. |
| Inloggningen saknade TOTP-fält | Konton med aktiverad 2FA kunde inte logga in från webben | Kodfält och överföring till API. Renderat formulärtest. |
| Profilval ändrade ingen serversession; PIN `0000` kontrollerades lokalt | Användaren trodde att annan profil och andra rättigheter/tittarhistorik användes | Lokal PIN-stub borttagen. Annat konto kräver riktig inloggning; valt användarnamn förifylls. Ingen PIN-baserad sessionsväxling påstås vara implementerad. |
| Kontosäkerhet/sessioner saknade webbvy | Ingen väg att aktivera 2FA eller återkalla egna sessioner | `/account`: aktivera/verifiera/stäng av TOTP och lista/logga ut enheter. |
| Fjärråtkomst saknade webbvy | Befintliga serverpolicyer kunde inte hanteras från webben | `/admin/remote`: status, policy, WAN-gräns och befintlig diagnostik, med tydlig begränsning för diagnostiken. |
| Samlingar/spellistor saknade webbvy | CRUD-API gick inte använda från ordinarie gränssnitt | `/collections`: manuellt skapa, läsa, byta namn, radera, söka/lägga till/ta bort objekt. Ej full musik-kö, smarta filter eller omordning. |
| Ljud-/undertextval på detaljsidan ignorerades vid start | Fel språk/undertext trots valt alternativ | Val skickas till spelaren och API; explicit avstängd undertext hålls avstängd i webben. Versionsväljare väljer rätt part. |
| Direktuppspelning kunde väljas för annat inbäddat ljudspår | Webbläsaren spelade sitt standardspråk trots serverbeslutet | Välj DirectStream/remux vid avvikande ljudval; kopiera video när möjligt. Beslutsmotortest. |
| Detaljvy laddades bara vid montering | Navigation mellan titlar kunde visa föregående titel | Reaktiv omladdning på ID, nollställning, felmeddelande och skydd mot sena svar. |
| Starta om skickade inget startvärde | Serverns sparade tittarposition kunde återanvändas | Explicit `startMs: 0`; även små begärda startpositioner bevaras. Renderade spelartester. |
| Spelare kunde återanvändas vid titelbyte; sena sessionssvar läckte | Föregående session/ström kunde leva kvar | Keyed spelarmontering och stängning av session som anländer efter avmontering; endast en stoppsändning per normal avmontering. |
| Bibliotek stannade vid första 200 objekten | Större bibliotek såg ofullständiga ut | Cursor-baserad Hämta fler och återställning vid filter-/biblioteksbyte. Renderat regressionstest. |
| Sökresultat kunde skrivas över av äldre anrop | Resultat passade inte söksträngen | Versionskontroll för svar i söksidan och kommandopaletten. |
| Watch-state och flera Live TV-mutationer behandlade HTTP-fel som framgång | Felaktiga framgångsmeddelanden och stängda dialoger vid 403/409/422 | Kontroll av API-klientens `error` före framgångsmeddelanden. |
| Live TV-knapp öppnade ingen spelare och släppte ingen tuner | Ingen visning; efter upprepade klick blev tunern full | Spelare för webbläsarkompatibla HTTP/HLS-källor, stängning/kanalbyte släpper reservation; sena svar släpps också. MPEG-TS-relay saknas fortfarande. |
| EPG blandade sekunder i UI med millisekunder i program | Fel datum och program utanför rutnätet | Millisekunder för fönster, axel och formattering. Tids-/positionsregression. |
| DVR kontrollerade samma kanal, utan nya marginaler | Överbokning mellan kanaler på samma tuner och överlapp i marginaltid | Beräkna toppkonkurrens per tuner över alla kanaler, med marginaler; validera intervall och normalisera äldre sekundvärden. HTTP-regression. Samtidiga skrivningar behöver fortfarande transaktionell reservation. |
| WAN-fördelning tvingade minst 1,5 Mbps även under lägre budget | Summan kunde överskrida satt uppladdningsgräns; noll innebar inte obegränsat | Respektera låg budget, hantera noll som obegränsat och undvik multiplikationsoverflow. Enhetstest. |

Ytterligare rättningar: explicit `partId` måste tillhöra begärt `itemId` (HTTP-regression); worker-vyn hanterar HTTP-fel och använder genererade typer. Vissa typfel i parallellt tillkommen Cast-kod rättades för att hela webbygget ska kunna verifieras.

## Täckning mot specificerat gränssnitt

Status avser faktisk användarresa, inte bara förekomst av ett API eller en typ.

| Specifikation | Status i webben efter ändringarna | Evidens / återstående arbete |
|---|---|---|
| M2 setup, bibliotek, bläddring, sökning, tittarstatus | Finns, vissa delar verifierade med tester | `Setup.tsx`, `Libraries.tsx`, `LibraryDetail.tsx`, `ItemDetail.tsx`, `Search.tsx`. Biblioteksredigering/radering och jobbstatus behöver kompletteras. |
| M3.2 teman/accent | Finns | `ThemeContext.tsx`, `Layout.tsx`; inställningslagring mellan användarbyten och visuella teman behöver integrationstest. |
| M3.3 Storybook/axe | Ej komplett verifieringskedja | `web/package.json` saknar `storybook:test` och axe-körning trots acceptanskriteriet. |
| M3.5 responsiv navigering | Delvis | Desktop/mobile finns. TV-tangentkoden i `Layout.tsx` är inte full rumslig navigering. Mobilens nya sidor nås via inställningar/kommandopalett. |
| M3.6 hemsida och hubbordning | Finns delvis | `Home.tsx` har ordningsändring; felhantering vid sparning behöver förbättras. |
| M3.7 detaljvy | Delvis | Version/ljud/undertext och browserprofil kopplade. Titellogga och komplett versions-/extraspresentation återstår. Förhandsbeslutet uppdateras inte för varje valt spår/version. |
| M3.8 spelare | Delvis | Hastighet, undertexter, nattläge, markörer och statistik finns. Kvalitetsmeny, ljudoffsetkontroll, Up Next och full statistik/HW-/nätkoppling behöver kompletteras. |
| M3.10/M10.6 språk och tillgänglighet | Delvis | Nyckelparitet täcker inte hårdkodade strängar i sidor och nya vyer. Fem kompletta produktöversättningar och WCAG 2.2 AA kan inte styrkas av befintliga tester. Fokus, mobil och skärmläsare behöver verklig kontroll. |
| M6.1/M6.2 profiler, ACL, åldersgränser, PIN, inbjudningar | Delvis | Users-vyn skapar/raderar användare; full redigering av rättigheter, hanterad profil, PIN och inbjudningsflöde saknas i UI. Profilbyte kräver lösenord. |
| M6.2 API-token och enheter | Delvis | AI Agents hanterar token; Account visar egna enheter. Alla specificerade nätbegränsningskontroller behöver jämföras med tokenformuläret. |
| M6.3 2FA | Grundflöde tillagt | Account + Login; ingen automatisk QR-bild (manuell hemlighet fungerar). |
| M6.8/M6.11 fjärråtkomst | Delvis | Policy/statusvy tillagd. Full rekommendationswizard och indikator direkt/relä per session saknas. ACME/DDNS/UPnP-installation är inte en färdig webbresa. |
| M7.3 musik | Saknas som specialiserad klient | Album/artist kan förekomma i generisk hierarki, men kö, minispelare, ambient now-playing och synkade texter saknas. |
| M7.5 foton | Saknas som specialiserad klient | Timeline-API finns i `routes/photos.rs`; ingen fototidslinje/lightbox/bildspel i routade sidor. |
| M7.6 samlingar/spellistor/editioner | Delvis | Manuella samlingar/spellistor och versionsval tillagda. Automatisk TMDB-samling, smarta filter, ordningsredigering, köuppspelning och extras återstår. |
| M7.7 metadataredigering/lås/fix-match/artwork | Saknas i detalj-UI | Ingen färdig metadataeditor eller artwork-väljare; serverkapabiliteter måste också följas till körbar adapter. |
| M7.8/M7.17 undertexter | Delvis | Val/visning finns; sökning/nedladdning, autosync och Whisper-generation saknar sammanhängande UI/jobbintegration. |
| M7.9/M7.10/M7.13 DLNA/Cast/fjärrkontroll | Under integration, ej komplett verifierat | Chromecast-knapp och `web/src/utils/cast.ts` tillkom parallellt under granskningen. Typ-/API-fel rättades i slutkontrollen. Verklig Cast-enhet, nätåtkomst till signerad ström, DLNA-val och genomgående kommandoväg återstår att verifiera. |
| M7.11 webhooks, schemalagda jobb, dashboard | Saknas som full adminvy | Endpoints/tjänster finns men ingen samlad jobb-/historik-/statistik-/webhookadministration. |
| M7.15 Home Assistant MQTT | Ej verifierad runtime-integration | `mbl-remote/src/mqtt.rs` innehåller nyttolast-/kommandofunktioner; serverns uppstart kopplar inte in en MQTT-anslutningsslinga. |
| M7.16 AI Companion | Saknas i webbspelaren | API/tjänst finns; panel/overlay och livetranskript-/spelarhändelsekedjan behöver kopplas ihop. |
| M7.18 X-Ray, recap, sleep guard, klipp | Saknar komplett webbresa | Fristående analysfunktioner är inte samma sak som tillgängliga spelarreglage och bakgrundsjobb. |
| M7.19 Matchmaker | Saknas i webben | `routes/matchmaker.rs` finns; ingen lobby/QR/swipe/match/uppspelningsresa. Deltagaridentitet och behörigheter måste härdas före exponering. |
| M7.20 requests/Arr/storage | Saknas i webben | `routes/requests.rs` finns; ingen sök-/begär-/godkänn-/lagringsvy. Arr-tjänstens hjälpfunktion räcker inte för en komplett integration. |
| M8.8 workers/noder | Delvis | `admin/Workers.tsx` använder nu genererade API-typer och visar HTTP-fel i stället för falsk framgång vid wake/revoke/parning. Riktig offload och löpande status ska verifieras separat. |
| M9 guide/tuners/DVR | Delvis | Fyra flikar finns, tidsenheter/spelarstart/felsvar rättade. Ingen inkopplad faktisk inspelningsmotor hittad i `mbl-server/src/app.rs`. Schemalagd DB-rad är inte verifierad inspelning till fil. |
| M9.6 onlineplugins | Demonstrationsdata | `mbl-service/src/plugins.rs` returnerar `example.com`-strömmar. Kan inte beskrivas som verklig YouTube/Spotify/WebDrama-uppspelning. |
| M9.7 virtuella kanaler | Delvis | Playhead/EPG finns, men skapandedialogen skickar tom `itemIds`; full biblioteksval-/strömövergångsresa saknas. |
| M10.5 migrering Plex/Jellyfin | API finns, UI saknas | `routes/import.rs`; ingen importdialog och resultatöversikt. |

## Ytterligare kvarstående serverfynd / prioritering

1. **P1: Live tunerreservation behöver ägare, session-ID och timeout.** `resolve_live_stream`/`release_live_stream` räknar per tuner och släpper via kanal-ID; reservationer kan släppas av annan autentiserad klient och leva kvar vid nätverksbortfall. Denna ändring rättar webblivscykeln men inte protokollet.
2. **P1: DVR måste få faktisk inspelningsmotor och atomisk resursreservation.** Den rättade konfliktkontrollen kan fortfarande konkurrera med ett samtidigt schemaläggningsanrop. Serieregler skriver direkt via repository och behöver samma bokningsregler och felrapportering.
3. **P1: Granska session-/deltagarisolering innan Companion/Matchmaker exponeras mer.** Matchmaker använder inskickat `user_name` och tidsbaserat sessions-ID. Ingen färdig medlemskapsmodell finns i det granskade tjänsteflödet.
4. **P1: WAN-bandbredd är ännu inte en verifierad end-to-end-policy.** Playback skapar `DecisionContext` med `Network::Lan`; registrering/feedback och återläsning av sparad WAN-budget vid omstart behöver kopplas till faktisk uppspelning.
5. **P1: Påstådd Hailo-inferens ska särskiljas från heuristik.** `mbl-hailo::analyze_image` räknar färger och textnyckelord. Backendnamn eller en benchmark av denna funktion styrker inte verklig NPU-modellkörning.
6. **P2: Skilj demo/fallback från lyckad produktion.** Node-binären skickar mockbibliotek; trickplay innehåller platshållarväg; onlineplugins visar exempeldata. Exponera inget sådant som framgång utan tydlig status.
7. **P2: Full felhantering och tillgänglighet.** Flera vyer sväljer nät-/HTTP-fel eller visar tom vy. Läs-/sparfel, återförsök, formuläretiketter, dialogfokus och mobil navigering behöver en andra genomgående runda.
8. **P2: Öka testernas bevisvärde.** Flera äldre frontendtester anropar mockad API-klient direkt och verifierar inte komponenten. Nya regressionstester i denna omgång monterar faktiska komponenter; komplettera med verklig webbläsare, mediefiler och server för produktacceptans.

9. **P1: Progressiv ljudström måste äga FFmpeg-processens livscykel.** `routes/media_audio.rs` startar ett barn, tar stdout och returnerar en `ReaderStream`; barnhandtaget bevaras inte i svaret och `kill_on_drop` används inte. Avbruten klient och processuppstädning behöver ett livscykeltest och en ägande strömwrapper.

En maskinell ögonblicksbild finns i [`2026-10-27-api-ui-inventory.json`](2026-10-27-api-ui-inventory.json): 180 OpenAPI-operationer, varav 93 matchar bokstavliga API-klientanrop i produktionswebben vid inventeringen. Detta är **inte** en täckningsprocent: dynamiska anrop, `<video>`-/bild-URL:er och rå `fetch` behöver separat bedömning, och interna protokolloperationer behöver inte egna UI-reglage.

## Verifiering

Se aktuell handoff i `AGENTS.md` för slutliga körresultat. En grön enhetstestsvit ska inte användas som ersättning för saknade acceptansflöden i tabellen ovan. Nya klienter ligger fortsatt utanför omfattningen.
