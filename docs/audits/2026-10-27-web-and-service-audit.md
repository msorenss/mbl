# Kod- och gränssnittsgranskning 2026-10-27

## Batch 24 — DVB-undertexter, medieprofiler och radio

- Recorder tar med undertextspår i stället för att ovillkorligen kasta dem. Syntetisk DVB-bitmaptext och två AC-3-ljudspråk bevaras i MPEG-TS, inklusive språkmetadata. Den publicerade inspelningen använder spelarens vanliga spårval och bildtextinbränning; ett pixelprov skiljer synlig DVB-text från Off på svart video. HEVC/AAC bevaras vid capture och omkodas för Firefox. Ingen ny videokodning görs i inspelaren.
- Ljudradio avvisas inte längre för saknad video. Positiv duration och ett riktigt audio-/videospår krävs; även läsbara undertextfiler med verklig duration avvisas. Radio blir Track i det hanterade DVR-biblioteket och använder vanlig HLS/AAC, historik, profilkontroll och ägarstyrd radering. Completed/Partial följer samma publicering som video. Ljuduppspelning behöver ingen videokodarplats.
- Ny `PlaybackResponse.hasVideo` ger rätt information även för audio-HLS. Webben visar titel/ljudvy och Seek audio, döljer videokvalitetsval och undviker trickplay-förfrågningar. HLS master annonserar den verkliga kopierade videocodecen; audio-HLS får enbart ljudcodec och ljudbitrate. Radio/Safari-HEVC deklareras inte längre felaktigt som H.264. Övriga spelarflöden återanvänds med befintlig REST/MCP-styrning; ingen ny endpoint/verktygsräkning/migration.
- Ett varierande HEVC-startfel spårades med fångad FFmpeg-felutdata till `Malformed AAC bitstream`: transportens ADTS-AAC kopierades utan `aac_adtstoasc` till fMP4. Ny intern `AudioOp::CopyAac` väljs för kopierat AAC och konverterar pakethuvuden utan ljudomkodning. Andra codecs kopieras som tidigare. Regressionen täcker både H.264-omkodning med kopierat AAC och HEVC/AAC-remux för Safari-profilen, inklusive serverad master och verkliga fragment.
- HEVC-remux normaliseras även till `hvc1` i MP4-fragmenten via `VideoOp::CopyHevc`. De hade tidigare FFmpegs `hev1` medan masterlistan sade `hvc1`. Det riktiga fragmentets sample entry kontrolleras med ffprobe, inte enbart codec-namnet i API-svaret.
- Verifierat: 51 unika riktade Rust-tester (nya DVR-mediaflöden 4; källfel 6; planer/diskvakt 6; schemaläggning/retention 10; relä/DVR 12; uppstart 6; spår 2; transkodningspipeline 5). Verklig FFmpeg verifierar DVB-pixlar/Off, ljudspråk, radio Completed/Partial med profil/historik/radering, HEVC→H.264 och HEVC-remux med matchande hvc1 samt kopierat AAC. 152 webbtester, TypeScript/Vite och initial JS-budget 42,24 KB gzip passerar; slutlig kontrastjustering är också byggverifierad. Biome har 0 fel och tidigare 5 varningar/4 infos. Clippy för fem berörda crates med alla targets och -D warnings, fmt/diff-check och OpenAPI-check passerar. OpenAPI/TypeScript är regenererade; MCP-paritet 148/148 (74 verktyg, 52 undantagna paths). Full workspace-regression efter batch 20–24 ingick inte.
- Avgränsning: testad DVB-text i MPEG-TS innebär inte generell bevaring/konvertering av SRT/ASS, text-TV, inbäddad CEA-text eller alla transport-/codecvarianter. Källors interna paket-/tidsstämpelfel och fysisk tuner/browser/receiver/WAN/Windows-acceptans återstår. Nästa core-arbete omfattar kvarvarande transport-/undertextprofiler, Failed-städning/återställning och helarkivskvot/prognoser. Ingen ny commit/release/M11.

## Batch 23 — bevarad Retry-policy och verklig planering

- Retry pekar på en ägd Failed/Partial-rad via `retryOf`/`retry_of`. Ursprunglig ägare och överlevande serieregel behålls, inklusive effektiv padding och keepMax/watch-retention. Admin ändrar inte ägarskapet när den hjälper en annan profil. Ägarens aktuella behörigheter gäller; delade tittare kan inte Retry. Borttagen regel ger manuell inspelning med samma ägare. DB kontrollerar rad/regel/ägare igen i skrivtransaktionen och återanvänder redan bokade försök utan dubbel tunerbelastning.
- Ny read-only preview normaliserar samma request som faktisk schemaläggning. REST och MCP visar guidens riktiga metadata/tider, aktuellt behörighetsbeslut, padding, ägare, ärvd lagringspolicy, konflikt eller befintlig inspelning samt diskstatus. Inga biblioteksrader, kataloger, jobb eller tunerbokningar skapas. Webben visar raderingspolicyn innan Retry bekräftas och förklarar lågt diskutrymme/konflikt innan en ny rad skapas. Dubbla klick spärras och sena svar efter avmontering kan inte öppna bekräftelse eller schemalägga. Planen är en ögonblicksbild och omprövas vid schemaläggning/start.
- DVR skyddar en standardreserv på 512 MiB före start och under capture och stöder valfritt filstorlekstak. `-fs` begränsas också av startens lediga utrymme minus reserven; gränser gäller med paketgranularitet. En giltig fil efter gränsstopp blir spelbar Partial. Ingen automatisk utrensning av andra sparade filer görs. Config/TOML/env samt MCP get_settings.dvr beskriver den tillämpade policyn. Under capture avbryts inspektionen vid shutdown/sluttid; timeout eller policyfel städar barnet utan delpublicering.
- Verifierat: 41 unika riktade Rust-tester (DVR-planer 6, källfel 6, schemaläggning 10, M9 7, MCP-snapshots 3, config 8, disk 1). Preview/MCP använder verklig guide och lämnar bibliotek/katalog/jobb/tuner orörda; ägare/retention, idempotens/konflikt, indragen ACL, låg disk och spelbar storleksbegränsad Partial täcks. Ett policyfel under verklig FFmpeg-capture reap:ar barnet utan att publicera delmedia. 151 webbtester passerar; TypeScript/Vite och JS-budget 42,24 KB gzip passerar efter rättning av två typfel i testassertioner. Biome har 0 fel och tidigare 5 varningar/4 infos. Clippy för sju berörda crates med alla targets och -D warnings, fmt/diff-check och OpenAPI-check passerar. OpenAPI/TypeScript är regenererade. MCP-paritet 148/148 (74 verktyg, 52 undantagna paths). Full Rust-workspace-regression efter batch 20–23 ingick inte.
- Reservvakten är pollad och reserverar inte volymutrymme mot andra skribenter eller garanterar framtida plats. Helarkivskvot/prognoser, städ-/återställningspolicy för Failed-filer och DVR-format/undertexter/radio återstår. Windows-implementationen är tillagd men inte lokalt hårdvaruverifierad; fysisk tuner/EPG/browser/receiver/WAN-acceptans kvarstår. Ingen ny commit/release/M11.

## Batch 22 — ofullständiga inspelningar utan falsk framgång

- Nya capturer klassas med probad medieduration jämfört med hela det planerade fönstret. En ren FFmpeg-exit efter tidig EOF räcker inte längre för Completed. `partial` bevarar en giltig del med förklaring, även efter käll-/encoderfel; tomt eller oläsbart material blir Failed utan katalogpost. Toleransen är 5 procent av fönstret, minst 100 ms och högst 2 s, för packetgränser/startjitter. Durationkontrollen bevisar inte att en källa är fri från interna tappade paket eller felaktiga tidsstämplar.
- Hängande capturer stoppas vid schemalagt slut med monotont larm och väggklockskontroll. Den tidigare 15-sekundersmarginalen är borttagen; faktisk medielängd avgör resultatet. Serverstopp, avbokning och indragna rättigheter publicerar inte automatiskt delmedia. Rättigheter/guide revalideras efter analysen så sena ändringar inte publicerar gammal rating.
- Status/orsak/katalogpost skrivs atomiskt. Partial är spelbar via vanliga spelaren och följer samma ACL/rating/ägarskap. Katalogtitel/beskrivning varnar också för ofullständig inspelning. Webben erbjuder Partial-filter, Play partial, Retry och Delete; nytt försök behåller den gamla filen och får nytt ID. KeepMax/watch-retention omfattar alla sparade Completed/Partial-filer med samma aktiva-spelarvakt. MCP-listning/filter/raderingspreview och snapshots följer statusen och beskriver sparade filer.
- Verifierat: 31 riktade Rust-regressioner passerar: källfel/sluttid 6, DVR-planering/retention 10 (med Partial-filer), relä/DVR/publicering 12 och MCP-snapshots 3. Källfelsfallen är även omkörda med en tunerplats. 148 webbtester, TypeScript/Vite och JS-budget 42,24 KB gzip passerar; Biome har 0 fel och tidigare 5 varningar/4 infos. Clippy för sex berörda crates/alla targets med -D warnings, fmt/diff, OpenAPI-check och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar. OpenAPI/TypeScript är regenererade. Full workspace-regression från kontrollpunkten är inte omkörd efter batch 20–22.
- Äldre Completed-arkiv omklassas inte retroaktivt. Opublicerade filer från serverstopp/restart förblir Failed; säker återställningspolicy och lagringsbudget/städning återstår tillsammans med DVR-undertexter/radio/övriga format och normaliserad MCP-preview. Verklig tuner-/browser-/receiveracceptans kvarstår. Ingen ny commit/release/M11.

## Batch 21 — Live TV-profiler och säkra programövergångar

- Live-uppspelning kontrollerar nu aktuell inloggning och åldersgräns före start och efter beredskap. Begränsade profiler måste ha ett pågående tillåtet EPG-program; en saknad guide godkänner inte en rå kanal. Profiler utan åldersgräns behåller guidefri/fortlöpande visning.
- Live-leases binds till användare och token-hash, eftersom ett raderat SQLite-sessions-ID kan återanvändas. Varje medieanrop, förnyelse och underhåll revaliderar session/expiry/profil/program/källa. Utloggning, ändrade rättigheter, nekad rating eller ändrad källa städar encoder/cache/tuner. Kontrollen körs också efter långsam förberedelse.
- En begränsad lease gäller ett program-ID och dess sluttid. Förnyelse förlänger inte den gränsen; FFmpeg-output begränsas även för en snabb fil-/catch-up-källa. `accessEndsAt` stänger webbspelaren vid programslut och kräver nytt kanalval, även om nästa program är tillåtet. Det är ett medvetet begränsat flöde; automatisk fortsättning för dessa profiler är inte implementerad. Terminalt mediefel stänger HLS och tömmer buffrad källa.
- REST-kanaler/guide/nu/nästa och MCP-guidesökning använder profilfiltrering. Sökningen filtrerar före resultatgränsen genom keyset-sidor. Råa tuner-/kanaladresser och dolda kanaler lämnas bara till admin, och tunerupptäckt kräver admin. Virtuell EPG döljs för andra profiler tills bibliotekens ACL kan verkställas där. `canPlay` ger begripliga spärrade spelknappar. Guiden uppdateras periodiskt och gamla svar kan inte ersätta nyare fönster.
- Verifierat: 35 riktade Rust-regressioner passerar (Live-profiler 6, relä/DVR 12, DVR-planering/retention 10, äldre M9/guide/MCP 7). Nytt kanaluppdateringsfall är också omkört. 147 webbtester, TypeScript, Vite och JS-budget 42,24 KB gzip passerar; Biome har 0 fel och tidigare 5 varningar/4 infos. Clippy för sex berörda crates/alla targets med -D warnings, fmt/diff, OpenAPI-check och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar. OpenAPI/TypeScript är regenererade. Full workspace-regression från kontrollpunkten är inte omkörd efter batch 20–21. De nya fallen täcker guidefiltrering över 205 nekade träffar, råa källadresser, saknad guide, verklig snabb HTTP-källa med programgräns, rating-/profiländring under start/visning, medie-/renewal-/underhållsstädning och återkallad/utgången/utbytt inloggning. Webben verifierar spärrad start, tidsgräns, byte av timerägare, guideuppdatering, dolda adminåtgärder och tömd buffert utan 403-retry.
- Område 01 förblir öppet för verklig tuner-/EPG-klocksynk, browser/WAN/receiveracceptans, hårdvaruprofiler och beständig timeshift. Nästa core-arbete är återstående DVR-käll-/format-/lagringsfall samt MCP-preview. Ingen ny commit/release/M11.

## Batch 20 — profiler, återkommande DVR-regler och automatisk retention

- Schemaläggning har nu explicit ägare och DVR-bibliotek. Guide-ID styr faktisk kanal, titel, tid och rating. Profiler med åldersgräns spelar in en tillåten guidesändnings exakta tidsfönster utan marginaler; fria tider kräver en profil utan åldersgräns. Ägare ser sina tillåtna väntande/pågående inspelningar. Delade färdiga filer följer befintlig biblioteks-/ratingpolicy, medan Delete/Cancel kräver ägare eller admin. Främmande serieregler döljs också vid direkt detaljanrop.
- Rättigheter kontrolleras igen före claim och under capture. Återkallad biblioteksåtkomst, ändrad åldersgräns, programtitel, höjd guide-rating, förskjutna programtider eller borttagna ägare/källor stoppar felaktiga jobb. PG-filtreringen vid kataloglistning följer nu också Actor-regeln för okända ratingetiketter, som tidigare felaktigt kunde visas för PG-profiler. Borttagen tuner raderar inte arkivet. Bibliotekets automatiskt hanterade roots skyddas mot vanlig flytt/radering; namnbyte stöds.
- Seriematchningen är inkopplad i serverns bakgrundslivscykel var 15:e sekund, separat från inspelningsstarterna. Den använder ett rullande 14-dagarsfönster, kanalsäsong/new/episodfilter, keyset-sidor om 200 och deduplicering per profil/kanal/sändningsstart. Guideimport uppdaterar befintliga sändningar. Avbokade/raderade sändningar läggs inte tillbaka automatiskt, och explicit Retry kan boka dem med ett nytt monotont fil-ID. Schemakonflikter visas med orsak; de tappas inte tyst. Regelborttagning stoppar väntande jobb men behåller pågående/färdiga inspelningar.
- keepMax och autoDeleteWatched verkställs nu mot riktiga filer/katalogposter. Endast regelägarens watch state räknas, och spelarens gemensamma livscykellås skyddar pågående/pausad uppspelning, startförhandling, heartbeat och avslut. Stängning av HLS föregår shutdown-låset så ett fastnat startförsök kan avbrytas även när retention väntar. Radering är begränsad till 16 försök per pass och skjuts upp vid pågående probe. Dolda avbokningsrader rensas efter sändningsslut + ett dygn.
- Webb-DVR visar Retry, ägarstyrda raderingsknappar, ursprungskanal och fullständiga säsongs-/episod-/retentionsval. Automatisk filradering är opt-in, förklaras i formuläret och återställs till säkra standardval efter skapad regel. MCP använder samma backend och validerar numeriska intervall utan u32-trunkering; schema/annotations/confirmation visar att retention kan radera sparade filer.
- Verifierat: 140 webbtester, TypeScript/Vite och JS-budget 42,23 KB gzip; Biome har 0 fel och tidigare 5 varningar/4 infos. 45 riktade Rust-regressioner passerar. Fallen täcker 205 matchningar, återimport, avbokning/Retry, konflikt/idempotens, riktiga mediefiler och HLS, delad uppspelning, ägarens watch state, ACL/rating, regel-/tuner-/ägarborttagning och uppgradering av tidigare schema med bevarat arkiv. Clippy för sex berörda crates/alla targets med -D warnings och fmt/diff-check passerar. OpenAPI och TypeScript är regenererade; OpenAPI-check och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar. Full workspace-regression kördes vid kontrollpunkten, inte på nytt efter denna batch.
- Område 02 är fortfarande öppet för fortsatta käll-/format-/undertext-/lagringskontroller och fysisk tuner-/browser-/receiveracceptans. Nästa core-del är Live TV:s profil- och programövergångar samt återstående felvägar i inspelningskällor/MCP-preview. M11 avvaktar; ingen ny commit eller release är skapad efter kontrollpunkten.

## Samlad kontrollpunkt 2026-10-29

- Granskningsfixarna från batch 1–19 har verifierats tillsammans: hela Rust-arbetsytan passerar 371/371 tester utan överhoppade tester. Körningen använder två testtrådar och repots flaggor utan debug-symboler; tillfälliga lokala testservrar används för integrationerna.
- `just web-check` passerar: 138 webbtester, TypeScript, produktionsbygge och initial JavaScript-budget 42,24 KB gzip. Biome har 0 fel och tidigare 5 varningar/4 infos. Inga regressionsrättningar behövdes.
- Hela arbetsytans Clippy med alla targets och `-D warnings`, fmt/diff-check, OpenAPI-check, tokens-check och MCP-paritet 147/147 passerar.
- Användaren har godkänt en lokal Git-kontrollpunkt. Den fysiska acceptansen och de 23 återstående breda arbetsområdena ligger kvar i [restlistan](remaining-work.md); kontrollpunkten är ingen release.

## Batch 19 — färdiga DVR-filer i den vanliga spelaren

- Färdiga inspelningar publiceras med item/version/part/spår i ett särskilt hanterat bibliotek, **TV recordings**. Ny `Recording.itemId` öppnar samma Player som bibliotekstitlar: förhandling, startberedskap, återupptagning/historik, kvalitet och spårval återanvänds. Ingen separat rå fil-URL eller alternativ spelmotor införs.
- ffprobe kontrollerar läsbar video och positiv verklig medieduration efter att FFmpeg avslutats. Format, codec, upplösning, bitrate, ljudspår och probe/kapitel sparas. Wall-clock-duration tas bort; migration 0010 nollställer äldre felaktiga längder tills filerna har analyserats på nytt. Saknade/trasiga filer publiceras inte. Tidsgräns och ett delat lås begränsar metadataanalysen.
- EPG-rating och beskrivning sparas vid schemaläggning och används även om guiden senare rensas. DVR-listning/detalj/radering i REST och MCP filtreras genom katalogens ACL/rating; vanlig Playback/Stream gör sina befintliga kontroller. Begränsade profiler ser tills vidare endast publicerade inspelningar, eftersom väntande inspelningar ännu saknar ett fullständigt profilbeslut. Profilmedveten schemaläggning/pågående listning återstår; Live TV-profiler omfattas inte av denna ändring.
- Publicering och avbokning serialiseras; status/itemrelation/media skrivs i samma databastransaktion. Sparade filer måste ligga på inspelningens härledda plats under serverns konfigurerade DVR-root; symlänkar och externa databasvägar avvisas. Filens storlek/mtime/plats kontrolleras igen efter probing. Den allmänna biblioteksskannern hoppar över DVR-biblioteket och kan inte importera partiella filer.
- Giltiga äldre Completed-filer återställs till katalogen vid serverstart. Återställningen kör parallellt med schemaläggningen så ett stort arkiv inte gör att nya sändningar missas. Delete städar fil och katalogpost; pågående avbokning städas fortsatt av barnprocessens ägare. Klienten har tydlig filraderingsbekräftelse, visar verklig längd och Play endast för publicerade Completed-rader, uppdaterar den öppna DVR-fliken och väntar på live-release före VOD-start.
- Verifierat: 138 webbtester; TypeScript/Vite, Biome (0 fel; tidigare 5 varningar/4 infos) och JS-budget 42,24 KB gzip. 10 riktade Rust-regressioner passerar: DVR/delad tuner 5, MCP-snapshots 3 och äldre schema-/MCP-DVR-flöden 2. En verklig femminuters MPEG-2/AC-3-inspelning får korrekt probe-duration och spelbar HLS; position efter 61 s sparas och nästa start återupptar där. Idempotens, samtidig publicering/radering, EPG-rensning, scanner-gate, ACL/rating och avvisning av skadade/externa/symlink-filer täcks. Clippy för berörda fem crates med alla targets och `-D warnings` passerar. Fmt/diff-check och OpenAPI-check passerar; TypeScript är regenererat. MCP-paritet är fortsatt 147/147 (74 verktyg, 52 undantagna paths). Fysisk tuner-/webbläsar-/mottagaracceptans och full workspace-release-regression ingick inte i denna batch.
- Område 02 förblir öppet: automatisk retention, fortsatt serieregelmatchning, profilmedveten schemaläggning samt fysisk tuner-/browser-/receiveracceptans återstår. M11/Git/release avvaktar. Restlistan är fortsatt 2/25 färdiga breda områden och 23 öppna.

## Batch 18 — serverägd Live TV och gemensam DVR-bokning

- Tunerns råa URL ersätts med ett serverägt HLS-relä. FFmpeg producerar en verklig rullande playlist, init och fMP4-fragment före lyckad förhandling. Basprofil: software H.264 högst 1280×720/25 fps, bwdif för interlaced-bilder, AAC-stereo; inget nytt HW/HDR-stöd påstås. Input begränsas till HTTP/HTTPS med begränsad protokollkedja och read-timeout.
- Ny mediaroute `/api/v1/stream/live/{session}/{file}` använder en slumpad capability från en ägd 45 s lease. Endast genererad index/init/segment-syntax tillåts; ingen katalogtraversering/tempfil. Privat/no-store, CORS, HEAD och Range finns. Även start-/renewal-svaren är no-store, så leaseförnyelse alltid når servern. Reläets originaladress lämnas inte i Playback-responsen. Mediehämtning förnyar inte leasen; den autentiserade ägaren förnyar separat. Release, expiry, processfel, kvot, kanal-/tunerborttagning och serverstopp städar child, katalog och kapacitet.
- Live och VOD delar videoencoderbudget under samma admission-lås. Live och DVR delar atomisk tunerbokning. DVR använder stream copy och behöver ingen encoderplats. Aktiva reservationer preempteras inte; en inspelning utan ledig tuner misslyckas med tydlig orsak. Kapitelns tidigare påstående om konfigurerbar prioritet är borttaget.
- Barnprocessen publiceras under leaseslåset innan shutdown kan passera den. En avbruten/fallande start städar kandidat och kapacitet; befintlig registrering behålls tills städning slutförts. Idle-analys ser liveencoderreservationer, och trickplay-handlerns wiring använder nu den slutligt konfigurerade PlaybackService-proben.
- Webbens kanalbyte väntar på release av föregående encoder. Start kan avbrytas; sena svar släpps, gamla callbacks kan inte stänga ny kanal. Leaseförnyelse har timeout/överlappsskydd. Slutligt spelar-/källfel och naturligt EOF släpper bokningen. Hls.js använder batch 17:s begränsade återhämtning. Virtuella kanalobjekt avvisas som tunerkällor och deras ospelbara knappar spärras.
- DVR reserverar tuner för barnets livstid, släpper den vid stopp/avbrott och städar partiella filer även när avbokning tävlar med FFmpeg-avslut. Färdiga filer spelas ännu inte från gränssnittet; felaktig wall-clock-duration i metadata samt fortsatt serieregelmatchning/retention ligger kvar i område 02.
- Verifierat: 136 webbtester (12 nya liveflödesfall), TypeScript/Vite och initial JS-budget 42,24 KB gzip; 32 unika riktade Rust-regressioner (relä/DVR 9, HLS-sessioner 9, äldre live-/schematester 3, startup/livscykel/trickplay 11). En verifierat interlaced MPEG-2/AC-3-källa ger progressiv H.264/AAC i verkliga fMP4-fragment. Faktisk DVR-fil, delad kapacitet, full 45 s expiry, cache/release, Range-body, HEAD, CORS och traversalavvisning verifieras. Clippy för berörda crates, fmt och diff-check passerar.
- OpenAPI/TypeScript är regenererade och schema-check passerar. MCP-paritet 147/147, 74 verktyg och 52 undantagna paths. Mediaroute och ägda interaktiva leaseflöden är dokumenterade i EXCLUSIONS. TypeScript verifieras även efter slutlig generering.
- Område 01/02 förblir öppna för profil-/ratingkontroll, vidare liveprofiler, faktisk DVR-uppspelning/retention/serieregler och fysisk/browser-/WAN-/receiveracceptans. Livecachen är ett litet rullande fönster, ingen beständig timeshift. Nästa core-del är inspelningsuppspelning och korrekt mediemetadata. M11/Git/release avvaktar; restlistan är fortsatt 2/25 klara, 23 breda områden öppna.

## Batch 17 — begränsad HLS-återhämtning och manuell omstart

- `cargo clean` kördes före arbetet: 8087 filer och 3,4 GiB borttagna. Cirka 8 GB ledigt efter städningen. Denna batch ändrar webbklienten; Rust/REST/MCP är oförändrade och byggs inte om efter clean.
- Lokal hls.js-återhämtning har högst två appförsök per vald källa, efter 1 respektive 3 sekunder. En 30 s återhämtningsdeadline från första fatala nätfelet förhindrar tyst fastlåsning efter ett återstartsanrop. Fristående hls.js-hämtningar begränsas till 20 s första byte/25 s totalt, ett HTTP-felåterförsök och inga extra timeoutförsök. Detta är separata nivåer, inte ett påstående om högst två HTTP-anrop totalt.
- Master-manifestfel använder `loadSource` igen: SDK:ns PlaylistLoader.startLoad laddar inte om ett misslyckat manifest. Fragmentåterhämtning använder aktuell tidsposition. Dubbla fel under samma väntan köar inte fler försök. Framgångsrikt buffrat fragment eller faktisk tidsframflyttning avslutar deadline, så även pausade spelare kan återhämta dataladdningen. Manifest-/playing-händelse ensam gör det inte; försöksbudgeten återställs först med en ny källa.
- 404/410, 401/403, övriga permanenta 4xx (inklusive 429) och fatala avkodnings-/formatfel ger tydligt fel och manuell Retry i stället för en obegränsad startLoad/recoverMediaError-loop. Native/direct mediafel får samma manuella återstart. Första förhandlingen kan också prövas igen med bibehållen begärd start; separat återförsök utan sparad ljudsynk finns kvar.
- Terminalt lokalt fel pausar och stoppar laddningen, sparar positionen före eventuell elementreset och släpper serversessionen. Retry och kvalitetsbyte efter fel väntar på den föregående städningen, med 10 s timeout på stop-anrop. Varje misslyckad session stoppas en gång, även om nästa källa fallerar synkront. Ny förhandling bevarar del, position, kvalitet, ljudspår, undertext/Off och ljudsynk; förberedd källa lämnas pausad med uppmaning att trycka Play. Om stoppanropet inte når servern kan den gamla leasen behöva löpa ut innan kapacitet finns; API-felets verkliga orsak visas.
- Återförsöksknappen visar vänteläge och spärras under arbetet. Paus under manifeståterhämtning respekteras. SDK-handlers registreras före load/attach så synkrona startfel fångas; synkront misslyckad återladdning följs inte av startLoad. Gamla källors fel/timers får inte påverka ny källa, Cast eller en stängd spelare. Google Cast-styrning regressionstestas; receiverintern återhämtning och fysisk acceptans ingår inte i denna batch.
- Verifierat: 124 webbtester (17 nya renderade HLS-/felregressioner), Biome utan fel, TypeScript/Vite och initial JS-budget 42,23 KB gzip; Rust fmt och diff-check. Slutbygget och bundle-kontrollen passerar. Ingen Rust-omkompilering, Clippy-/OpenAPI-/MCP-omkörning behövs för denna frontendbatch; batch 16:s resultat gäller dess oförändrade serverkod. Full releaseverifiering återstår enligt restlistan.
- Område 09 förblir öppet för återstående telemetri och fysisk/native-/bakgrundsacceptans. Nästa core-batch går vidare till Live TV-/DVR-kedjan. Restlistan är fortsatt 2/25 klara, 23 breda områden öppna; M11 och Git/release avvaktar.

## Batch 16 — sessionsunderhåll, paus och återupptagning

- Serverns serve-/start_jobs-livscykel startar nu ett ägt underhållsjobb varje sekund. Befintlig timeout, ahead-throttling och diskkvot körs även utan nya uppspelningsstarter. Vid serverstopp avbryts loopen, nya starter spärras och FFmpeg-processer/kataloger städas innan databasen stängs. TCP-bindning sker före bakgrundsstart så en upptagen port inte lämnar startade jobb. Den gamla oanropade, oavbrytbara `spawn_maintenance_loop` är borttagen.
- Ägarvaliderad progress förnyar HLS-lease också vid paus/buffring, utan att flytta fragmentmarkören. Förnyelse och beslut om inaktivitet använder samma sessionslås. En stängd/utkastad session kan inte återupplivas. Diagnostik och en annan användares progress förnyar ingen lease. Shutdown stänger kapacitetsantagningen under dess lås och hanterar även samtidiga/pågående starter.
- HLS-sessioner som försvunnit och övergivna direktsessioner tas bort ur listan över uppspelningssessioner och WAN-fördelningen. Bekräftad uppspelning skrivs till historik en gång, obekräftade kandidater kasseras. Städning använder redan sparad progress och skriver inte tillbaka en äldre position över en nyare session av samma titel.
- Webbklienten skickar heartbeat var femte sekund under spelning, paus och buffring och vid återgång till synlig flik. Före verklig lokal `playing`-händelse skickas buffering, så enbart väntan/autoplay-blockering inte skapar historik. Anrop överlappar inte, har 10 s timeout och avbryts vid stängning.
- 404/410 för aktuell progress eller ett fatalt HLS-nätfel stoppar retry mot en städad session. Lokalt erbjuds **Resume playback**, som förhandlar vid aktuell position och behåller spår, ljudsynk, kvalitet och paus. Sena felsvar från ersatta sessioner ignoreras. Cast visar att mottagaren behöver kopplas från/anslutas igen; dess heartbeat använder mottagartid, även under paus.
- Stop-anrop behåller sessionen hos HLS-managern tills process- och katalogstädning är färdig. Ett avbrutet anrop tappar därför inte ägarskapet; underhåll kan slutföra en delvis stängd session. Ett låsstyrt avbrottstest verifierar att sessionen förblir upptäckbar och sedan städas.
- Verifierat: 107 webbtester, TypeScript/Vite och initial JS-budget 42,23 KB gzip; 27 riktade Rust-tester (playback/spår 7, startup/livscykel 9, HLS-sessioner/avbrott 9 och MCP 2 inklusive 10 paritetsfall). Kontrollerade testprocesser verifierar lease/avbrott; tidigare verkliga FFmpeg-regressioner för start/spår körs också. Clippy, fmt, OpenAPI och MCP-paritet 146/146 passerar. TypeScript verifieras även efter API-generering.
- Område 09 förblir öppet. Browser-/OS-sömn kan stoppa heartbeat längre än timeout (standard 60 s); återupptagning får då skapa en ny ström. Fysisk Cast-/native- och bakgrundsacceptans återstår. Övriga fatala HLS-fel använder fortfarande äldre återförsökslogik; nästa core-steg är begränsad återhämtning där samt Live TV-/DVR-uppspelning. De 23 breda kvarvarande områdena ligger kvar i restlistan.

## Batch 15 — verifierad uppspelningsstart och säkra källbyten

- Webbens alla playback-starter begär `waitForReady`. Servern kontrollerar att direktkällan går att öppna och läsa, eller väntar på första färdiga HLS-fragmentet och init för alla annonserade renditions. Ett gemensamt tidsfönster begränsar väntan (nuvarande standard 15 s); tidiga kända hårdvarufel använder befintlig fallbackkedja. Äldre API-anrop behåller tidigare antagningsbeteende om flaggan utelämnas. MCP-preview startar ingen process.
- Avvisad start, FFmpeg-fel, timeout eller avbruten startframtid städar kandidatens process, katalog och kapacitetsplats. Pågående start räknas som uppspelningsaktivitet så idle-analys viker undan. En stoppad session markeras stängd: ett sent segmentanrop via en kvarhållen Arc kan inte återstarta FFmpeg.
- Halvskrivna filer från en misslyckad process räknas inte som färdiga. Vid omstart tas gamla fragment från och med omstartspunkten bort, så nästa gamla fil inte felaktigt intygar att ett nytt fragment är färdigskrivet. Verklig FFmpeg-failure, tidig backend-fallback, stall/timeout och avbruten förhandling täcks av regressionerna.
- Den befintliga webbströmmen behålls under serverförhandlingen. Vid accepterat kvalitets-/ljud-/undertextbyte läses positionen på nytt, så väntetiden inte spolar tillbaka filmen. Samma positionsbevarande gäller Google Cast-spårbyte; explicita nya medier/Up Next använder sin begärda start. Förberedelse visas i gränssnittet och stängning av spelaren avbryter väntande startanrop; sena lyckade svar släpps.
- Beredskapskontrollerade kandidater börjar som buffering. Enbart förberedelse/buffring och efterföljande städning ändrar varken watch state eller historik; första playing/paused-progress bekräftar sessionen. Google Cast rapporterar också direkt efter accepterad laddning.
- Verifierat: 102 webbtester, TypeScript/Vite och initial JS-budget 42,23 KB gzip; 25 riktade Rust-tester (playback/start/spår 11, pipeline/session 12, MCP 2 inklusive 10 paritetsfall). Clippy, OpenAPI och MCP-paritet 146/146 passerar.
- Område 09 förblir öppet. Beredskap bevisar första serverartefakten, inte webbläsaravkodning, mottagarens nätåtkomst eller senare mediefel. **Nästa core-steg:** periodiskt sessionsunderhåll är inte inkopplat i produktionslivscykeln (`spawn_maintenance_loop` saknar anrop). Inkopplingen måste göras tillsammans med pausad heartbeat/återupptagning och kontrollerat serveravslut, så pausade spelare inte blir av med sin HLS-ström. Fysisk mottagaracceptans och de breda restpunkterna kvarstår.

## Batch 14 — ljud- och undertextspår

- Nytt ljudspårsreglage använder aktuella källans metadata och verkliga serverindex. Spårval, undertext/Off, kvalitet och ljudsynk bevaras genom omförhandling. Både lokal uppspelning och Google Cast respekterar en paus som görs under väntan på svaret. Avvisat byte behåller tidigare session/val och visar API-felets riktiga orsak.
- Undertextmenyn erbjuder bild-/inbränningsspår med tydlig konverteringsetikett. Inbränning/ersättning/Off förhandlar en ny videoström; sidolevererad text växlas direkt. Google Cast får inte längre ett externt caption-ID för en text som redan är inbränd i videon.
- ASS/SSA levereras enligt profilens target-format (WebVTT för webben). Inbyggd text översätts från absolut filindex till FFmpegs undertextordinal. Extern text använder sin egen validerade filsökväg. Verkliga ASS-/SRT-klipp verifierar synliga bildpixlar vid inbränning; filer utanför biblioteket nekas. ABR med burn-in returnerar valideringsfel eftersom dess graph annars tyst ignorerade texten; webbflödet använder en rendition.
- Tysta videor använder AudioOp::None i stället för att mappa ett obefintligt ljudspår. Master-playlisten beskriver verklig ljudcodec och korta videor får ingen påhittad minstalängd på sex sekunder.
- Fel streamtyp/saknat index ger 422 i REST/preview. MCP ger invalid-params och trunkerar inte överstora index till u32. Den befintliga 10-fallssviten för REST/MCP startar/stänger nu en session per fall så den respekterar verklig kapacitet.
- Cast-laddning tål fel i frivillig titelmetadata och visar aktuell titel efter mediebyte. Native Remote Playback-reglage som kräver ny källa spärras utan att föregående session stoppas.
- Verifierat: 99 webbtester, TypeScript/Vite och budget 42,23 KB initial gzip; 20 riktade Rust-tester (playback/spår 7, MCP 2 inklusive 10 paritetsfall, pipeline/session 11). Verklig HLS-avkodning bekräftar 880 Hz i valt ljudspår, WebVTT-text, synlig inbyggd/extern inbränning och ljudlös kortvideo. Clippy, OpenAPI och MCP-paritet 146/146 passerar.
- Ingen bred punkt stängs: sök/nedladdning/autosync/generering av nya undertexter, sena FFmpeg-startfel samt fysisk PGS-/mottagaracceptans ligger kvar i restlistan. Nästa core-förbättring är ett säkrare byte när FFmpeg fallerar efter att sessionen antagits.

## Batch 13 — verkliga sökförhandsbilder

- FFmpeg extraherar verkliga nyckelbilder. JPEG-ark byggs bara av befintliga, korrekt dimensionerade rutor. Misslyckad avkodning/timeout ger fel, inga färgade platshållare eller fabricerade framgångsmanifest. Fixad startnumrering, sista delintervall, mycket kort video och storleksgräns vid orimlig duration.
- Videoskanning köar idle-generering; cachemiss köar ett deduplicerat jobb och returnerar 202/Retry-After. Readyrouten kör inte längre tung FFmpeg-kod. Background-handler är registrerad och avbryter för senare återupptagning vid uppspelning/termisk belastning. Typed idle-yield bevarar räknaren för verkliga fel. Produktionssensorn aktiveras också utan testsysfs-root.
- Cache binds till verklig källfil, storlek, mtime, duration och generatorversion. Legacyplatshållare accepteras inte, saknade ark regenereras och ändrad källa får ny revision. Färdiga revisionskataloger publiceras före DB-index och gamla revisioner städas. Temporära filer och barnprocesser städas vid avbrott.
- Manifest och bilder använder samma bibliotek/rating/root-kontroll som mediet, inklusive redan cachade resurser. Bildsvar är privata och autentiserade. `TrickplayPreview` väljer rätt ruta vid pekar-/tangentbordssökning, återanvänder ark och städar blob-URL:er, anrop och polling. Sena svar från föregående mediedel ignoreras. Cast-statusbilden ligger under spelarreglagen.
- Fristående scanner-test hittade saknad `time/parsing`-feature som tidigare doldes av workspace-feature-unifiering; beroendet är rättat.
- Verifierat: 91 webbtester, TypeScript/Vite och initial JS-budget 42,24 KB gzip; 40 riktade Rust-tester (trickplay-analys 5, jobbkön 11, skanner 14, serverns M5/playback/trickplay 10). Verkliga färgklipp verifierar ruta/bildarksgräns och kort video; HTTP/jobbrunner verifierar kö, källa/cache, ACL/rating och privat JPEG. Clippy för berörda crates, OpenAPI och MCP-paritet 146/146 passerar.
- Område 04 är färdigimplementerat. Hårdvarudekodning/offload av analys och manuellt webbläsar-/receiverprov är inte verifierade av denna batch; CPU-sampling av nyckelbilder används. Övriga uppspelningsområden ligger kvar i restlistan.

## Batch 12 — signerad ljudsynk

- Servern hanterar −10000..+10000 ms i ljudströmmen: negativt tidigarelägger, positivt fördröjer. Kompatibel video kopieras. Ljudet kodas och dess tidsstämplar flyttas; trimningen utgår från den aktuella HLS-processens start vid sökning/omstart. Ett syntetiskt pulstest med riktig FFmpeg/HLS/avkodning mäter båda riktningar före och efter sökning (tolerans 80 ms).
- Samma accepterade värde returneras i playback/preview/diagnostik och MCP. Ogiltigt intervall ger 422 och ingen ny session. Videor utan ljud ignorerar sparad justering och visar 0 ms, så preferensen inte blockerar uppspelning.
- Webbreglaget omförhandlar vid aktuell position, bevarar pausläge och följer kvalitetsbyte. Google Cast får samma serverjustering och kan ändra den vid mottagarens position. Native Remote Playback använder befintlig källa och blockerar ändring under casting. Nattläget behåller EQ/kompressor; tidigare lokal DelayNode är borttagen för att undvika dubbeljustering.
- Om sparad ljudsynk hindrar första starten finns återförsök utan synk. Valet återuppta/starta om bevaras, serverfelets detalj visas och preferensen återställs först efter accepterad start.
- Avvisat byte behåller tidigare sessionskälla och sparad inställning. Google Cast behåller också den aktiva receiverregistreringen och slutlyssnaren när serverförhandlingen misslyckas före nytt medieanrop.
- Verifierat: 85 webbtester, TypeScript/Vite och budget 42,23 KB initial gzip; pipeline 5/5 (inklusive riktigt ljudpulstest), playback-API 5/5, MCP-tool-regression och tre schema-snapshots. Clippy för berörda Rust-crates, OpenAPI och MCP-paritet 146/146 passerar.
- Fysisk läppsynk/Cast-acceptans är inte verifierad av signal- och mocktesten. Hårdvarudekodare, uppmätt receiverbandbredd och mottagarrutt är fortsatt öppna i område 09; övriga uppspelningspunkter ligger kvar i restlistan.

## Batch 11 — spelarstatistik (core prioriteras)

- Ny sessionsbunden diagnostikroute och utökad MCP `explain_session` använder samma servermetod. Ägare/admin kan läsa, annan användare får 404, och sessionstopp tar bort åtkomsten.
- Backend, processstatus, ahead-paus, omstarter och senaste misslyckade exit läses från ett gemensamt HLS-lås. Direktuppspelning redovisar att servertranskodning inte används; saknad runtime markeras som saknad.
- Player hämtar diagnostik endast när statistikpanelen är öppen och avbryter vid byte/stängning. Cast läser inte den pausade lokala videons bildmått/buffert/dropped frames som receiverstatistik. Serveranropets nätklass skiljs från en faktiskt mätt receiver-/Tailscale-rutt.
- Lokal buffert räknas bara över intervallet som innehåller aktuell position. Upplösning uppdateras vid metadataändring. Saknade mätvärden och API-fel visas ärligt.
- Verifierat: web-check med 78 tester, TypeScript, Vite och budget 42,24 KB initial gzip; playback-API 4/4, MCP-regression, HLS-runtime-regression, Clippy, OpenAPI och MCP-paritet 146/146.
- **Core först enligt användaren:** full ljudsynk inklusive tidigareläggning av ljud är nästa delsteg. Verklig receiver-/avkodar-/bandbreddstelemetri kräver fortsatt arbete; denna batch är inte full hårdvaruacceptans.

## Batch 10 — foton

- `/photos/:id` erbjuder EXIF-/tillagd-datum per månad, stabil cursorbaserad hämtning, autentiserade originalbilder, bilddialog med pilar och bildspel. Vyn nås från fotobibliotekets detaljvy.
- Tidslinjen räknar nu bara bilder användaren får se. Sidlistan och originalströmmen använder samma biblioteks-/ålderskontroll. Icke-fotobibliotek/okända bibliotek nekas och originalbilder har korrekta MIME-typer.
- Bilder hämtas först när de behövs i vyn; blob-URL:er och anrop frigörs vid stängning/byte. Bildspel väntar på laddad bild, pausar på dold sida, stannar vid sista inlästa bild och stoppas vid bildfel.
- Verifierat: 74 webbtester, TypeScript, Vite och budget 42,24 KB initial gzip; HTTP-regression för rating/ACL, tidslinje/pagination vid lika datum samt autentisering/MIME för verklig PNG; server-Clippy, OpenAPI och MCP-paritet 145/145.
- Fotoflödets implementation är klar. Manuellt webbläsar-/tillgänglighetsprov ligger kvar i slutkontrollen. Webbläsaren måste stödja originalformatet; vyn implementerar ingen RAW-/HEIC-konvertering eller servergenerering av fotominiatyrer.

## Batch 9 — automatisk fortsättning på Google Cast

- SDK-mediets slutorsak måste vara IDLE/FINISHED efter observerad PLAYING-status. PAUSED/BUFFERING/ERROR/CANCELLED/INTERRUPTED, gamla laddningar och dubbla notifieringar utlöser inte nya starter.
- Aktuell laddning har eget ID och medielyssnare som städas vid nytt media/avslut. Slutposition bevaras även när SDK:ts tidsfält nollställs.
- Explicit opt-in till automatisk fortsättning fungerar på Google Cast. Avslutad serversession stängs och inväntas före nästa förhandling. Tillståndet kan invänta sent hämtad nästa-metadata utan att starta fler gånger.
- Verifierat: fyra nya regressioner, 70 webbtester totalt, TypeScript, produktionsbygge och budget 42,19 KB initial gzip passerar. Native Remote Playback-automatik och fysisk Cast-acceptans ingår inte i denna verifiering.
- Cast-guiden har uppdaterats till den aktuella implementationen och länkar officiella SDK-källor för slutorsaker och medielyssnare.

## Batch 8 — Up Next

- Spelaren hittar nästa numrerade avsnitt i samma eller nästa tillgängliga säsong via de autentiserade API:erna. Ordning avgörs av numrering, inte arrayordning. Tomma säsonger hoppas över och misslyckade API-anrop ger återförsök i stället för att tyst hoppa vidare.
- Up Next visas sista 30 sekunderna/vid slut. Manuell start är inkopplad i PlayerProvider. Lokal automatisk fortsättning kräver explicit opt-in och följer enbart aktuell uppspelningskedja.
- En redan pågående sessionsstängning inväntas före nästa lokala start. Sena uppslag avbryts/ignoreras efter titelbyte eller avmontering.
- Manuell nästa under casting använder befintlig receiverförhandling och ärver inte föregående avsnitts strömindex. Fysisk Cast-acceptans och automatisk fortsättning på receiver återstår.
- Spelarens globala tangentkommandon avstår från input/select/button/länk-fokus.
- Verifierat: sju nya tester, 66 webbtester totalt, TypeScript, Vite och budget (42,19 KB initial gzip) passerar.

## Batch 7 — säker antagning av transkodning

- Samtidiga starter reserverar kapacitet atomiskt genom en antagningslåsning fram till infogning/startfel.
- Ett kvalitetsbyte som inte får plats ger 503 och lämnar tidigare HLS-sessioner kvar. Servern avslutar inte längre tidigare uppspelning för att försöka skapa plats. Byte vid full kapacitet kräver frigjord kapacitet; gränsen överskrids inte tillfälligt.
- Saknad HLS-manager/FFmpeg returnerar fel i stället för genererad adress utan fungerande session.
- Sessionssviten 6/6 (inklusive samtidiga starter/spawn-fel) och samtliga playback-API-tester 3/3 passerar. Clippy för mbl-transcode/mbl-service/mbl-server --all-targets passerar.

## Batch 6 — kvalitetsval och ljudfördröjning

- Spelaren erbjuder ny serverförhandling med Auto/2/4/8/20 Mbps och aktuell position/medieversion. Lyckat svar ersätter sessionen, avvisat svar behåller klientens aktuella källa och återställer menyvalet. Sena svar städas vid stängning.
- Lokal ljudfördröjning på 0–5 s är kopplad till en WebAudio DelayNode, inklusive sparad preferens och nattlägeskedjan. Nattläge från sparade inställningar aktiveras vid start. Tidigareläggning av ljud återstår.
- Kvalitets-/ljudreglage är spärrade under casting. Cast-omförhandling återställer kvalitetsvisningen till automatisk.
- Verifierat: web-check med 59 tester, TypeScript, Vite och budget (42,18 KB initial gzip). Tre nya renderade regressioner för kvalitetsförhandling, pausbevarande, avvisat byte och verklig ljudnod.
- **Aktuell numrerad restlista:** [remaining-work.md](remaining-work.md). Spelarområdet inkluderar fortsatt Up Next, full HW-/nätstatistik, tidigareläggning av ljud ; kapacitetsfelet vid HLS-byte hanteras i batch 7.

## Batch 5 — tokennät, felhantering och språkval

- API-tokenformuläret har explicit LAN/Tailscale/WAN-val med säkra LAN/Tailscale-standarder och stoppar tomt val. Nätbegränsning visas i listan.
- Avvisad återkallning och läsfel visas. Misslyckade konfigurationsexempel raderar inte en skapad token eller påstår att skapandet misslyckats. Gamla snippets nollställs för ny token. Urklippsframgång visas först efter lyckad skrivning.
- Backend accepterar franska och spanska i sparade inställningar, utöver en/sv/de. Persistens och avvisat okänt språk testas.
- Verifierat: 56 webbtester, TypeScript, produktionsbygge och budget (42,18 KB gzip); två riktade Rust-regressioner och mbl-service Clippy passerar. Tre nya renderade tester täcker tokenflödena. Detta slutför inte rapportens allmänna i18n-/WCAG-acceptans.

## Batch 4 — säkra engångsinbjudningar

- Admin Users har skapa/lista/återkalla inbjudningar med bibliotek, rating och giltighet. Länken öppnar ett registreringsformulär med kontroll av utgången/förbrukad länk och skydd mot att ersätta ett inloggat konto.
- Konto, tilldelade rättigheter och förbrukning av länk görs atomiskt. Dubbla samtidiga registreringar kan inte skapa två konton. Misslyckat kontoskapande förbrukar inte länken. Koder har tagits bort ur loggar.
- UI kräver minst ett uttryckligt bibliotek för nya inbjudningar. Befintlig API-semantik för tom bibliotekstilldelning är oförändrad.
- 53 webbtester, TypeScript, produktionsbygge och budget passerar (42,18 KB gzip). Databasens samtidighets-/rollback-test samt Clippy för mbl-service/mbl-db passerar. PIN-baserad sessionsväxling återstår.

## Batch 3 — profilbehörigheter och PIN

- Admin Users har behörighetsdialog för bibliotek, hanterad profil och åldersgräns samt separata reglage för att sätta/ta bort PIN. Administratörers undantag och betydelsen av tom bibliotekstilldelning visas uttryckligen.
- Dialogen tillåter inte sparning efter läsfel, bevarar val vid sparfel och rapporterar framgång efter API-svar.
- API-deserialisering skiljer nu mellan utelämnad åldersgräns och explicit null; en befintlig gräns kan därför tas bort.
- Två nya renderade tester; web-check med 50 tester, TypeScript, produktionsbygge och budget passerar (42,13 KB gzip). Riktat Rust-regressionstest och mbl-service Clippy passerar. Inbjudningsflödet och riktig PIN-baserad sessionsväxling återstår.

## Batch 2 — webbflöden för förfrågningar och import

- `/requests` finns nu med validerat formulär, egen/all lista beroende på roll, godkännande och tillgängligmarkering. Desktop- och mobilnavigation är inkopplad. TMDB-sökning, Arr-leverans och lagringsanalys återstår.
- `/account` erbjuder import av Plex/Jellyfin-formatets JSON-filer med storleks-/formatkontroll, resultat och omatchade titlar. Direkt exporthämtning från källserver ingår inte.
- Tre nya renderade tester täcker fel och återförsök samt rollstyrda adminreglage. 48 webbtester passerar. Övriga ursprungliga fynd nedan kvarstår om de inte uttryckligen åtgärdats i statussektionerna.
- Git och buggfixrelease väntar tills samtliga överenskomna rättningar och kontroller är klara.

## 2026-10-28 — Implementation av granskningsrapporten (pågår)

Följande har implementerats i arbetskopian efter den ursprungliga granskningen:

- Ägar- och sessionsbundet spelarregister, begränsad WebSocket-kommandokanal, närvarotimeout och fel vid offline spelare. ADR 0007 beskriver leveransgarantierna.
- Separat konservativ Chromecast-profil, omförhandling efter mottagarval, MIME/part-ID, public_url, loopback-validering och externa textspår. Återgång till lokal spelare laddar rätt källa.
- Live TV-reservationer med slumpmässiga ID, autentiseringssession som ägare, 45 sekunders timeout och förnyelse från webben. Annan session kan inte förnya eller frigöra reservationen.
- Atomisk DVR-konfliktkontroll och bokning, även för serieregler. Serverns DVR-loop startar ägda FFmpeg-processer, sparar filer/status och hanterar avbokning, omstart och avstängning. Verifiering mot verklig tuner återstår.
- Companion kräver verklig behörig uppspelning. Matchmaker har slumpmässiga sessionslänkar, explicit medlemskap och röster bundna till användar-ID. Migration 0009 ogiltigförklarar gamla röster vars identitet inte kunde verifieras.
- WAN-klassificering vid uppspelningsstart, sparad budget vid omstart, sessionregistrering och gemensam pacing av direkta/HLS-mediesvar.
- Progressiv ljudström äger och avslutar FFmpeg vid frånkoppling; codec/bitrate/sökposition valideras.
- Bildanalys/benchmark rapporterar CPU-heuristik, inte påstådd NPU-inferens. Onlineproviders märks som demo och deras exempelströmmar kan inte startas som lyckad uppspelning.
- Biblioteksredigering/radering, felhantering för hemsidans layout, reaktiv förhandsvisning vid spår-/versionsval och periodisk worker-status.

Rapportens fynd nedan beskriver ursprungsläget. Denna statussektion anger senare åtgärder. **Hela rapporten är ännu inte färdigimplementerad.** Återstående större delar är bland annat Live TV-relay, fysisk Cast-/tuneracceptans, DVR-uppspelning/retention, nodernas mockkatalog, specialiserad musik/foto, metadata-/undertextverktyg, adminjobb/webhooks, MQTT, Companion-overlay och Matchmaker-UI. M11-klienterna ligger fortsatt utanför uppgiften.


Verifierat 2026-10-28: `just web-check` passerar (45 tester, TypeScript, Vite, 42,03 KB initial gzip). Workspace-Clippy passerar. MCP-paritet 144/144. Full Rust-körning: 327/328 passerade; enda felet var tunerfixturen i nya sessionsregressionen. Efter rättad fixtur passerar dess riktade körning 4/4. Ingen fysisk Cast-/tuneracceptans är genomförd.

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

## Fördjupad granskning av Chromecast-commit `d05a03f`

Commiten innehåller både tidigare granskningsrättningar och den nya Cast-integrationen. Plugin-av/på-flödet har ett relevant API-livscykeltest. `m12_chromecast_player_registry_integration` testar däremot HTTP-registrering/uppdatering/radering och anrop till kommandobussen; det ansluter inte en SDK-klient eller fysisk mottagare. Dokumentationens beskrivning av full Cast-/MQTT-/companionintegration gick längre än koden.

### Verifierade fel och rättningar

| Prioritet | Fynd i commiten | Åtgärd i denna granskning |
|---|---|---|
| P1 | `reportProgress` och stop läste `videoRef.currentTime` trots att den lokala videon pausats vid Cast-start | Progress, även vid fjärrpaus, och avslut använder mottagarens position. Lokala mediehändelser får inte skriva över Cast-tid/läge. Renderat test kontrollerar rapportering av 140000 ms trots att lokal video står vid noll. |
| P1 | En avbruten Cast-dialog lämnade lokal video pausad och kunde öppna en andra native-dialog | Föregående lokala spelläge återställs vid misslyckad/avbruten anslutning. Google-avbrott följs inte av en andra väljare. |
| P1 | `DELETE /players/{id}` fanns på servern men anropades aldrig av `cast.ts`; sena heartbeat kunde återregistrera frånkopplad spelare | SDK-/native-disconnect städar tillstånd, stoppar timer och köar avregistrering efter redan skickade registreringar. Spelarstängning avslutar den ägda sessionen; sena anslutningssvar ignoreras. |
| P1 | Ingen CORS på HLS-manifest/segment och undertext-/direktströmsrouterna | CORS/preflight begränsat till mediarouter. HTTP-test skickar extern Origin och signerad Range-begäran och verifierar 206, bytes, headers, 401 utan token samt avsaknad av CORS på kontorouten. |
| P2 | SDK-prenumeration använde `PLAYER_STATE_CHANGED`, som saknas i den dokumenterade `RemotePlayerEventType` | En dokumenterad `ANY_CHANGE`-prenumeration läser mottagarens samlade läge, tid, volym och mute. Testdubbeln accepterar enbart den dokumenterade händelsen. |
| P2 | `volumeLevel || 1` visade ljudlös mottagare som full volym | Nullish-standardvärde bevarar noll. Testat. |
| P2 | Play och pause var samma ovillkorliga toggle; mute och flera seek-/volymreglage ändrade lokal video | Idempotent play/pause mot SDK-läge; mute, tangentbord, tiasekundersknappar och intro-/credits-hopp går till aktiv mottagare. |
| P2 | Native-fallback markerades ansluten utan att följa dess tillstånd; kontrollmetoder använde fortfarande Google-controllern | Native-anslutnings-/mediehändelser följs och reglage styr native-videon. Disconnect-knappen öppnar webbläsarens väljare eftersom API:t inte erbjuder ett generellt disconnect-anrop. |
| P2 | Artwork använde `chrome.cast.media.Image` i stället för `chrome.cast.Image` | Rätt konstruktornamn och SDK-dubbel som verifierar metadataöverföringen. |
| P2 | Undertext-, hastighets- och nattlägesreglage såg aktiva ut men påverkade bara lokal video | Lokala kontroller avstängda under casting. Receiver-spårval återstår; det påstås inte vara färdigt. |

### Kvarstående Cast-hinder

- **P1: Mottagarprofil/MIME.** Samma uppspelningsbeslut och URL som för webbläsaren återanvänds. Chrome kan acceptera format som en viss Chromecast inte klarar; direktström antas fortfarande vara `video/mp4` om MIME saknas. En konservativ Cast-profil, korrekt MIME och omförhandling vid överlämning behövs.
- **P1: Verklig kommandotransport och isolering.** `PlayerRegistry::send_command` sänder till en intern broadcastkanal och ignorerar misslyckad sändning. Ingen produktionskonsument av `subscribe_commands()` finns i repot. Ett 204-svar bevisar därför inte att Cast-klienten fått kommandot. Registrering/avregistrering saknar också ägarisolering: andra inloggade användare kan skriva över eller radera en känd spelarpost. Utforma ägare/delning och en sessionsbunden transport innan MCP-fjärrkontroll markeras klar.
- **P1: URL från localhost/proxy och faktisk receiveråtkomst.** `toAbsoluteUrl` använder webbläsarens origin, inte serverns `public_url`. Localhost eller felvänd proxyadress kan inte hämtas av TV:n. CORS löser inte routning, certifikat, codecstöd eller en riktig överlämning.
- **P2: Undertexter, spår och receiverlivscykel.** Externa undertexter skickas inte till Cast och inget receiver-spårval finns. Slut på film, nätbortfall, återanslutning, app-/flikstängning och flera samtidiga kontroller behöver acceptansprov på hårdvara.
- **P2: Native-stöd varierar.** Förekomst av Remote Playback API innebär inte att alla Safari/AirPlay-/Miracastkombinationer fungerar. Ingen separat WebKit AirPlay-väg finns. Native-anslutningens väljare ägs av webbläsaren.

Commiten tog också bort versionsrubriken för `1.0.0` i `CHANGELOG.md`, så M10 hamnade under Unreleased. Rubriken har återställts.

MdBook-guiden har skrivits om så att den skiljer nuvarande funktioner från dessa begränsningar. Kontroll av SDK-beteende gjordes mot Googles primärkällor: [RemotePlayerController](https://developers.google.com/cast/docs/reference/web_sender/cast.framework.RemotePlayerController), [RemotePlayer](https://developers.google.com/cast/docs/reference/web_sender/cast.framework.RemotePlayer), [framework-händelser](https://developers.google.com/cast/docs/reference/web_sender/cast.framework), [Image](https://developers.google.com/cast/docs/reference/web_sender/chrome.cast.Image), [mediestöd/CORS](https://developers.google.com/cast/docs/media).

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
| M7.9/M7.10/M7.13 DLNA/Cast/fjärrkontroll | Delvis, med konkreta Cast-rättningar | Commit `d05a03f` har nu granskats separat; se fyndtabellen ovan. SDK-/spelarlivscykel och media-CORS har regressionstester. Verklig Cast-enhet, receiverprofil, DLNA-val och genomgående kommandoväg återstår. |
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

En maskinell ögonblicksbild finns i [`2026-10-27-api-ui-inventory.json`](2026-10-27-api-ui-inventory.json): 181 OpenAPI-operationer, varav 94 matchar bokstavliga API-klientanrop i produktionswebben vid inventeringen. Detta är **inte** en täckningsprocent: dynamiska anrop, `<video>`-/bild-URL:er och rå `fetch` behöver separat bedömning, och interna protokolloperationer behöver inte egna UI-reglage.

## Verifiering

Senaste verifieringen efter Cast-rättningarna: workspace-nextest **321/321**, workspace-Clippy med `-D warnings`, `just web-check` med **42 tester**, TypeScript/Vite/bundlebudget **42.03 KB gzip**, OpenAPI-check och tokens-check passerade. MCP-paritet **142/142 (100%)** efter dokumenterat undantag för klientens avregistrering. Full Rust-körning behövde tillåtelse att binda loopback-portar för befintliga mockservrar; första sandlådeförsöket stoppades av miljön. Ingen fysisk Cast-mottagare verifierades. Se även aktuell handoff i `AGENTS.md`. En grön enhetstestsvit ska inte användas som ersättning för saknade acceptansflöden i tabellen ovan. Nya klienter ligger fortsatt utanför omfattningen.
