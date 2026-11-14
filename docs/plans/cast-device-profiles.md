# Enhetsanpassad Google Cast och adaptiv Automatisk

Status: P1:s servergrund och geometrinormalisering implementerade lokalt 2026-11-13;
full omförhandlingsacceptans återstår. P2:s senderdel och P3:s receiverkod är implementerade; registrering/hosting och fysisk acceptans återstår. P4:s begränsade tvånivåväg är implementerad; fysisk ABR/WAN-acceptans återstår, se P4 nedan.
Checkpoint `e070192` är nu installerad på Pi:n. Kort fysisk Auto-uppspelning från noll på Bedroom display har bekräftats av användaren med bild och ljud (standardreceiver, säker 720p30-profil, en nivå). Full fysisk matris återstår; se driftguiden.
Prioritet: spelarens kärnfunktion före övriga klienter. Baslinje och incidenter:
[Cast-driftguiden](../audits/cast-runbook.md).

## Mål och definition av klart

Automatisk ska välja högsta rimliga kvalitet som både mottagaren, källan,
nätverket och servern klarar. Nest Hub får aldrig originalvideo över sin
verifierade gräns bara för att H.264-codecen stöds. Google TV ska inte sänkas
till Hub-kvalitet. Manuellt bitrateval får inte kringgå enhetens hårda gränser.

Klart innebär fysisk verifiering på Bedroom display och Bedroom TV, separat
verifiering på Kitchen display efter modellkontroll, maskinella regressioner,
begripliga GUI-besked samt loggar som förklarar varje kvalitetsbeslut.
En enda lyckad LOAD eller ett enstaka klipp räcker inte.

## Nuläge och tekniska begränsningar

- `DeviceProfile::builtin("chromecast")` återanvänder `generic-hls.toml`:
  H.264 nivå 4.1, 1920×1080, stereo och HLS fMP4. Alla modeller behandlas lika.
- Automatisk skickar ingen uttrycklig kvalitetsgräns. Den kan välja kopierad
  1920×960-video från testfilen. 2 Mbps ger omkodad 1280×640 och fungerar nu
  på Bedroom display. Auto-felets exakta orsak är fortfarande inte isolerad.
- Browser-codecprober får inte användas som Cast-mottagarens kapabiliteter.
- Sender SDK:s Receiver ger namn, label, receiverType och grova audio/video-
  kapabiliteter, inte en komplett decoderprofil. Label är knuten till
  browserprofil och origin; den är inte en global fysisk enhetsidentifierare.
- `canDisplayType(mime, codecs, width, height, framerate)` finns på receiver-
  sidan. Att anropa den i vår webbläsare är inte en mottagarprobe. Default
  Media Receiver ger oss inte automatiskt en egen kanal för dessa svar.

Beslut (uppdaterat efter användarens önskemål): automatisk kapabilitetsförhandling
är huvudvägen och ett krav för första leveransen. Bygg egen CAF-receiver tidigt.
Användaren ska inte behöva veta eller välja mottagarens modell. Vid okänt eller
uteblivet stöd väljs en försiktig fallback automatiskt med tydligt besked.
Receiverregistrering och HTTPS-hosting är därför faktiska leveransberoenden.
Ett eventuellt avancerat kompatibilitetsval får endast sänka verifierade gränser;
det får inte låta användaren välja en starkare modell och kringgå dem.

## Beslutsmodell

Skilj på fyra typer av uppgifter:

1. **Hårdvarustöd:** tillåtna codec/profile/level-kombinationer, dimensioner,
   fps, bitdjup, HDR, ljudformat/kanaler samt transport/container.
2. **Användarval:** önskad kvalitetsgräns och eventuell striktare profil.
3. **Nätverk:** LAN/WAN-policy, serverns bandbreddsbudget och uppmätt mottagar-
   genomströmning när sådan finns. Senderns nätverk är inte receiver-nätverket.
4. **Server:** tillgänglig encoder, samtidighet, faktisk kodningshastighet och
   cacheutrymme. Serverkapacitet får aldrig utvidga enhetens formatstöd.

Kandidat väljs ur skärningen av tillåtna kombinationer. Använd inte enbart
oberoende maxvärden: t.ex. 1080p30 och 720p60 är två olika tillåtna kombinationer.
Behåll originalet endast när både bild, ljud, container och övriga villkor
uppfylls. Ingen uppskalning. Bevara bildförhållande, beakta rotation/SAR och
använd encoderkompatibla jämna dimensioner. Begränsa både bredd och höjd,
inklusive stående och ovanliga format. Minska fps när nödvändigt; stöd för
fps-begränsning måste implementeras hela vägen till FFmpeg.

## Föreslaget datakontrakt

Inför versionssatt `CastCapabilityReport` och serverägd `CastProfileId`.
Första profilkatalog: `cast-safe-720`, `nest-hub`, `nest-hub-max`,
`chromecast-legacy`, `chromecast-gen3`, `chromecast-ultra`,
`google-tv-hd`, `google-tv-4k`, `google-tv-streamer`.
Exakta kombinationer verifieras mot modellens dokumentation; HD/4K får inte
sammanblandas. Otestade codecs aktiveras inte enbart för att modellen nämns.

Rapport/lagrat val innehåller:

- schemaVersion, profil-ID, källa (`manual`, `receiver-probe`, `fallback`),
  capability-revision och observationsdatum;
- tillåtna video-/ljudkombinationer och transportstöd;
- lokal mottagarnyckel, inte ett påstått globalt hårdvaru-ID;
- stödets säkerhet: verifierat, uppgivet eller okänt.

Servern validerar typer, storlek och intervall. Rapporten får aldrig styra
FFmpeg-argument direkt eller kringgå ACL, WAN-policy, kvoter eller sessionsägare.
Skilj kompatibilitetsprofil från resursbehörighet.

## Fas P0 — Baslinje och mätbara regressioner

- [ ] Spara installerad binärhash, commit, FFmpeg-version och hårdvarumodell.
- [ ] Fånga separat Auto och 2 Mbps från noll på samma fil och mottagare.
- [ ] Spara master/media-listor, init, första två fragment och ett resume-
  fragment utan att lägga tokens/mediefiler i Git.
- [ ] Kontrollera verklig upplösning, fps, codecprofil, EXTINF och tidsstämplar.
- [ ] Lägg integrationstest för de redan rättade Cast-felen: skalad masterhöjd
  och inga syntetiska 3+3-segment när FFmpeg skriver 6-sekunderssegment.

Acceptans: reproducerbar före/efter-baslinje. Markera Auto som olöst även om
profilbegränsningen senare verkar hjälpa; skilj profilfel från EVENT-problem.

## Fas P1 — Serverprofiler och säkert formatval

- [x] Inför profilkatalog med dokumenterade kombinationer och profilrevision.
- [x] Utöka playback-kontraktet med validerad Cast-profil; befintliga klienters
  `chromecast` fortsätter fungera. Välj uttrycklig migrationspolicy så att
  äldre Google TV-klienter inte oväntat försämras utan GUI-besked.
- [x] Uppdatera `decide.rs` för dimensions-/fps-gränser och korrekt skalfaktor.
- [ ] Respektera profilen i Direct Play, remux, transcode, resume, text-/ljud-
  byte, nästa avsnitt, kvalitet och optimerade medieversioner.
- [ ] Profil för okänd videomottagare börjar konservativt: H.264 SDR 8 bit,
  högst 720p30 och AAC-stereo. Detta är en försiktig fallback, ingen universell
  kompatibilitetsgaranti. Audio-only ska inte erbjudas video.
- [x] Master beskriver faktisk utdata: upplösning, CODECS och rimlig bandwidth
  inklusive ljud. Använd encoder-/probeinformation, inte fast avc1-sträng.

Berör: `mbl-playback` profile/decide, `mbl-core` där typer hör hemma,
`mbl-service::playback`, `mbl-transcode` och HLS-rendering.
Acceptans: Hub-profil kan aldrig få 1920×960 i Auto eller vid manuell 20 Mbps;
Google TV-profil kan behålla kompatibel originalvideo.

### P1 genomförande och avgränsningar (2026-11-12)

- Katalog revision 1 i `mbl-playback/src/cast_profiles.rs`, vald via befintligt
  `profile`-strängfält, exempelvis `nest-hub`. Inget nytt REST-endpoint eller
  nya serialiserade playback-schemafält behövdes. Klientrapporten i P3 är separat.
- `chromecast` är fortsatt bakåtkompatibel; nya modellnamn bevaras genom hela
  serverflödet. Klienten skickar fortfarande gamla namnet tills P3/P2 införts.
- Hårdvara begränsas till H.264 SDR/stereo i revision 1. Google TV HD använder
  säker 720p30 tills dess specifika profil verifierats. HEVC/AV1/HDR aktiveras
  inte på antagande. Detta är stödda delmängder, inte maximala modellbeskrivningar.
- Hög bitrate, full profil och overlay får inte utvidga en namngiven
  serverprofil. Saknade dimensioner ger fel vid Cast-HLS-start; okänd fps eller
  codecmetadata tillåter inte direkt kopiering med de nya profilerna.
- FFmpeg får fps-begränsning när den behövs, även för interlaced/okänd fps.
  Init läses efter förberedelse; master får faktiskt AVC-CODECS och dimensioner.
  Utdata över dimensionstaket avvisas och sessionsplatsen städas.
- Geometrinormalisering tillagd 2026-11-13: probe registrerar om pixlarna är
  kvadratiska och videon oroterad. Okänd äldre probe eller avvikande geometri
  förhindrar videokopiering för nya Cast-profiler. FFmpeg normaliserar SAR till
  1:1 efter autorotation och begränsar båda axlarna till profilens tak.
  Riktiga FFmpeg-tester verifierar anamorf 2:1 och 90 graders rotation samt
  60→30 fps. Testfilens rotation skrivs med `-display_rotation`; den äldre
  `rotate`-metadatataggen skapade ingen display matrix med lokal FFmpeg 7.1.5.
- Text-/ljudbyte och optimerade versioner använder samma serverprofil när
  klienten skickar den. Att P2/P3 alltid skickar rätt profil är ännu inte verifierat.
- Verifierat 2026-11-13: 25 riktade tester (9 playback, 7 probe, 2 HLS,
  7 pipeline) passerade, liksom `cargo clippy --workspace --all-targets -- -D warnings`.
  Geometritestet passerade även efter tillagd kontroll av testfilens metadata.
  Fysisk Cast har inte körts med dessa nya profiler.
- Följande kvarstår från P1-listan: audio-only receiverklassificering (kräver
  receiverinformationen från P3) och end-to-end-verifiering av samtliga
  omförhandlingar. P1 markeras därför inte fullständigt accepterad.
- Inget installerat eller releasat. P1 dokumenterades före P2. Se P2-anteckningen nedan för efterföljande klientarbete.

## Fas P2 — GUI för automatiskt vald kapabilitetsprofil

- [ ] Anslut till MBL:s receiver och invänta P3:s kapabilitetsrapport före
  serverförhandling. SDK-label används endast för lokal korrelation.
- [x] Välj effektiv profil automatiskt från verifierade formatkombinationer (sender; P3-rapport krävs i drift).
  Exakt modellnamn är inte nödvändigt: korrekt avkodningsstöd är målet.
- [ ] Ingen modellväljare i normalt uppspelningsflöde. Visa exempelvis
  “Automatisk · upp till 720p · mottagarens stöd” och faktiskt format i stats.
- [x] Saknad/otillräcklig rapport ger säker fallback och texten
  “Begränsad kompatibilitet – mottagarens stöd kunde inte verifieras”.
  Påstå inte att en modell har identifierats när vi bara använder fallback.
- [x] Cache är en prestandaoptimering, aldrig ensam auktoritet för att höja
  kvaliteten. Återvalidera på ny Cast-session och ändrad receiver-/protokollrevision.
  Sena svar från annan enhet får inte påverka aktuell profil. Sendern använder ingen cache.
- [x] Profilen följer alla omförhandlingar; byt atomärt när mottagaren byts (senderns gemensamma förhandlingsväg och sessionskontroller).
- [ ] Visa om effektiv kvalitet begränsas av enhet, bitrate, server eller nätverk.
- [ ] Ett avancerat kompatibilitetsläge kan vid behov sänka kvaliteten, men
  ska inte krävas för normal användning och får inte utvidga verifierat stöd.
- [ ] Mobiltesta laddning, timeout, fallback, återförsök och långa enhetsnamn.

Acceptans: Bedroom display och Bedroom TV får rätt effektiva gränser utan
modellval. Okänd enhet hanteras automatiskt och ärligt. Rensad lagring,
ny browser eller nytt konto får inte kräva att användaren kan hårdvarumodellen.

## Fas P3 — Egen receiver för verklig automatisk upptäckt

- [ ] Gör en separat teknisk prototyp med CAF. Registrera utvecklings-app-ID
  och testmottagare; dokumentera ansvar för konto, HTTPS-hosting och drift.
  Publicering/extern registrering hanteras som separat leveranssteg.
- [x] Receivern rapporterar begränsade `canDisplayType`-prober och tillgängliga
  kapabiliteter över ett versionssatt `urn:x-cast:`-namespace.
- [x] Testa utvalda formatkombinationer med explicita CAF-prober (fysisk acceptans kvar); undvik okontrollerad probing och att
  tolka ett positivt MIME-test som bevis på faktisk prestanda/HDR-ljudkedja.
- [x] Koppla rapport till aktuell Cast-session och request-nonce. Ignorera
  sena svar från tidigare enhet, för stora payloads och okända schemafält.
- [x] Ge handskakningen en kort timeout (initialt 3 s, mät och justera).
  Saknat/negativt besked använder säker fallback med synlig orsak.
  En äldre cachad rapport får inte övertrumfa ett aktuellt negativt besked.
- [x] Initiera rätt profil före POST playback. Kortlivade behörigheter gäller
  mediahämtning; skicka inte administratörstoken till receiver eller logg.
- [x] Behåll standardreceiver som explicit kompatibilitetsläge. Ett byte
  mellan receiverappar kan kräva ny anslutning och ska inte döljas.
- [x] Lägg begränsad receivertelemetri: demux/decode-fel, buffring, format och
  faktisk representation. Undvik råa URL:er, cookies och stora SDK-objekt.

Acceptans: två fysiskt olika mottagare får olika verifierade profiler utan
namngissning; utebliven capability-kanal fungerar med tydlig fallback.
P3 är ett leveranskrav för huvudflödet och genomförs före P2.
Standardreceivern får inte beskrivas som automatiskt modellmedveten.


P3-kod och registreringsguide: [Custom Receiver](../cast-receiver.md).
Lokalt implementerat 2026-11-13; registrering/hosting och fysisk acceptans återstår.

## Fas P4 — Adaptiv kvalitet inom enhetens gränser

- [x] Skapa högst två nivåer initialt, exempelvis upp till 720p och 480p för
  Hub. Bitrate bestäms av källan, mätning och budget, inte en fast garanti.
- [x] Filtrera varje nivå genom enhetens codec/fps/HDR-gränser. Nivåerna måste
  ha kompatibel init, synkroniserade nyckelbilder, ljud och segmentgränser.
- [x] Direct Play/remux med en nivå märks som original/en nivå, inte adaptiv.
- [x] Låt mottagarspelaren göra ABR när två nivåer finns. Ingen konkurrerande
  sender-loop som laddar om mediet vid varje tillfällig bandbreddssänkning.
- [x] Pi: aktivera inte två mjukvarukodningar bara för att klienten stöder ABR.
  Kräv hållbar uppmätt kodningskapacitet, annars en säker nivå med tydligt besked.
  Utvärdera förkodade versioner/worker som separat kapacitetslösning.
- [x] Med en nivå kan ett ihållande problem erbjuda lägre kvalitet eller en
  begränsad omförhandling. Bevara position, text, paus och ljud; använd cooldown
  och retry-tak så att uppspelningen inte pendlar mellan kvaliteter.
- [ ] Verifiera WAN-budget och faktisk receivertrafik, inte browserbandbredd.

Implementerat lokalt 2026-11-14:
- Namngivna Cast-profiler kan använda den befintliga tvånivåvägen för H.264 SDR,
  högst 720p och 30 fps, utan inbränd text. Varje FFmpeg-gren normaliserar SAR/rotation
  och begränsas separat; mastermetadata läses från respektive verklig init.
- Kräver självtest yngre än 24 timmar för samma FFmpeg och drivrutinsfingeravtryck,
  minst tre gånger källans fps samt två lediga kodningsplatser. Detta är konservativ
  inträdeskontroll, inte bevis på hållbar prestanda under verklig belastning.
  Pi/ARM behåller en nivå. Ingen namnheuristik eller ny receiverapp krävs.
- Varje rendition tar en plats i den gemensamma VOD/live-budgeten. Misslyckad
  inträdeskontroll eller förberedelse provar en nivå. GPU-ABR faller inte tyst vidare
  till två mjukvarukodningar.
- Statistikpanelen visar faktiskt antal nivåer och erbjuder rådet att sänka
  kvalitet manuellt vid kvarstående buffring. Befintligt spår-/paus-/positionsbevarande
  används; ingen automatisk omladdningsloop har lagts till.
- WAN:s befintliga förhandlingsbudget används. Browserns nätklass är fortfarande
  inte ett mätbevis för receiver-vägen. Fysisk bandbreddsbegränsning på mottagarens
  väg, längre belastningstest och verifierad WAN-klassificering återstår.

Acceptans: begränsat nät på receiver-vägen ger nivåbyte utan sessionsbyte när
ABR är aktivt; aldrig nivå över enhetstaket. En Pi utan kapacitet är stabil med
en nivå och visas korrekt. Ingen automatisk löfte om Netflix-lik ABR överallt.

## Fas P5 — Regressioner, fysisk acceptans och release

Maskinella tester:

- [ ] Beslutstabeller för codec/level, 720/1080/4K, 24/30/60 fps, HDR, Dolby-
  ljud, stående/SAR-video, okänd enhet och alla manuella kvalitetsgränser.
- [ ] Riktiga FFmpeg-fragment: ffprobe-format, avkodning, tidslinje, två nivåer,
  resume och init-stabilitet; API-ägarskap och validering.
- [ ] Browser: kapabilitetscache, byte av enhet/konto, timeout, felaktig/sen
  capability-rapport, fallback, avbrott och bevarade uppspelningsval.
- [ ] En transkodningsplats: lyckad ersättning och misslyckad ersättning med
  återställning på rätt position; ingen läckt session eller dubbelt lokalt ljud.

Fysisk matris per modell (logga binärhash och utfall):

| Scenario | Godkänt när |
|---|---|
| Auto från noll och sparad position | Rätt profil, PLAYING och stigande position samt synlig bild/ljud |
| 2/4/8/20 Mbps | Inget format över enhetstaket; tydlig effektiv kvalitet |
| Text Av/på/byte, bildtext, ljudspår | Rätt text/ljud och bibehållen A/V-synk |
| Paus 1 och 10 minuter, resume | Session förnyas och återupptas utan förlorad position |
| Seek fram/bak och nära slut | Rätt segment, stabil init, ingen felaktig completion |
| Minst 30 min sammanhängande uppspelning | Ingen cacheutslagning, drift eller oanmält stopp |
| Begränsat nät och kort avbrott | Rimlig anpassning/återhämtning utan retry-loop |
| Byte Hub ↔ Google TV, omstart av browser/server | Ingen kvarhängande profil/session; begripliga fel |

- [ ] OpenAPI: registrera nya typer/rutter, regenerera schema och TS, kör
  openapi-check; MCP-paritet eller motiverad EXCLUSIONS enligt AGENTS.md.
- [ ] Översätt nya GUI-strängar; kontrollera mobil och tillgänglighet.
- [ ] Kör relevanta Rust-/webbtester och slutlig bred verifiering sekventiellt
  med AGENTS-flaggor. På Pi: CARGO_BUILD_JOBS=1 när minnet är pressat.
- [ ] Uppdatera driftguide, changelog och IMPLEMENTATION; dokumentera vad som
  faktiskt testats. Separata commits per fas, release först efter acceptans.
- [ ] Inför rollback för receiverapp/profilrevision. Rollback av enhetsdetektion
  får inte återinföra felaktiga segmenttider eller masterdimensioner.

## Diagnostik och prioriterad ordning

Logga `receiver.profile_selected`, profilkälla/revision, tillåtna format,
`playback.decision` med orsak, verklig utdata, antal nivåer och fallbackorsak.
Korrelera session/attempt utan publika capability-token. Persistenta
mottagarnycklar ska inte förekomma i delbara exportloggar.

Ordning: P0 → P3-prototyp → P1 → färdig P3 → P2 → fysisk första acceptans
→ P4 → full P5. Fasnumren behålls för spårbarhet; ordningen ovan gäller.
Receiverprototypen ska tidigt visa att tillräcklig kapabilitetsinformation
kan hämtas på både Hub och Google TV innan kontrakt och GUI låses.
Om Auto fortsätter misslyckas trots korrekt <=720p-utdata: återgå till
EVENT/remux-analysen, inte ytterligare godtycklig sänkning av upplösningen.

## Primärkällor och kodankare

Kontrollerade 2026-11-12:

- [Receiver i Sender SDK](https://developers.google.com/cast/docs/reference/web_sender/chrome.cast.Receiver): label är origin-/browserbunden; capabilities är grova.
- [Receiver Context](https://developers.google.com/cast/docs/reference/web_receiver/cast.framework.CastReceiverContext): canDisplayType och device capabilities körs på mottagaren.
- [Googles mediestöd](https://developers.google.com/cast/docs/media): modellgränser.

Kod: `web/src/utils/cast.ts`, `Player.tsx`, `castDiagnostics.ts`,
`crates/mbl-playback/src/profile.rs`, `decide.rs`,
`crates/mbl-service/src/playback.rs`, `crates/mbl-transcode/src/args.rs`,
`session.rs`, `crates/mbl-stream/src/hls/` och API/OpenAPI.

## P2 senderimplementation — 2026-11-13

Klienten frågar nu den aktuella Cast-sessionen före varje serverförhandling.
`castProfile.ts` väljer en serverägd profil ur verifierade kombinationer; namnet
på mottagaren används enbart som visningstext. Ingen modellväljare eller
beständig kapabilitetscache införs. Profil-ID beskriver en kompatibel delmängd,
inte en identifierad hårdvarumodell.

Senderkontrakt för P3 (revision 1):

- Namespace: `urn:x-cast:com.mbl.capabilities`.
- Begäran: `{schemaVersion: 1, type: "probe", nonce: <UUID>}`.
- Svar: `{schemaVersion: 1, revision: 1, nonce, video, hlsFmp4, mp4,
  aacStereo, mp3Stereo, modes}`. Booleans ska rapportera faktiskt stöd.
- `modes`: högst 16 poster `{width, height, fps, avcLevel, sdr8Bit}`.
  Varje positiv post avser H.264 Baseline/Main/High upp till angiven nivå,
  SDR 8 bit, i både MP4 och HLS/fMP4. En hög upplösning/fps får inte sättas
  enbart efter positivt generellt MIME-test. P3 måste verifiera kombinationen.
- Högst 8192 tecken, exakt schema/revision och aktuellt nonce. Okända fält
  godtas inte. En annan sessions svar får aldrig ändra aktuell profil.
- Timeout 3 sekunder eller saknad kanal ger `cast-safe-720`, högst 720p30.
  Ett giltigt aktuellt `video: false` stoppar videoförsöket. Ett gammalt negativt
  svar ignoreras, precis som ett gammalt positivt svar.

Profilen överlämnas direkt till Players gemensamma Cast-förhandling, inklusive
nästa avsnitt, kvalitet, resume och spårbyten. Ny förhandling gör en ny kontroll;
misslyckad förberedelse behåller föregående profilvisning. Vid frånkoppling
rensas valet. UI visar upplösningstak, verifierat/reservläge och separat
begärd bitrate. Det påstår inte att nätverkets genomströmning är uppmätt.

**Leveransgräns efter P2 (P3-koden beskrivs ovan):** standardreceiver är fortfarande standard utan registrerat app-ID. Den levererar inte
MBL-rapporten och får därför reservprofilen även på Google TV. Registrerat
receiver-app-ID, hosting och fysisk verifiering av capability-prober saknas fortfarande.
Automatiskt högre kvalitet per fysisk mottagare är alltså inte färdiglevererad.
P2:s fysiska mobil-/mottagaracceptans är kvar; markera inte checklistan helt klar.

SDK-underlag: [CastSession message listeners och sendMessage](https://developers.google.com/cast/docs/reference/web_sender/cast.framework.CastSession).

Verifiering: hela webbsviten passerade med 258 tester. Efter de sista skydden
för byte under LOAD och råa online-URL:er passerade 65 riktade Cast-tester.
TypeScript och slutligt produktionsbygge passerade. Råa online-URL:er använder
inte serverförhandlingen och visas därför inte som begränsade av denna profil.
Mobillayout har radbrytning och begränsad textbredd; fysisk mobil-QA återstår.

P3-verifiering: 269 webbtester passerade, inklusive CAF-start, capability-kontrakt,
telemetrimaskering och kompatibilitetsläge. Produktionsbygge och initial JS-budget
147,47 KB gzip passerade. Fysisk mottagar-QA återstår.
