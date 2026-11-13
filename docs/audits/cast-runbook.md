# Cast: drift, enhetsstöd och regressionsfelsökning

Kontrollerad 2026-11-12 mot arbetskopian efter v1.3.6. Dokumentet beskriver
lokala ändringar; en installerad v1.3.6-release behöver inte innehålla dem.
Detta är referensen för felsökning. [Användarguiden](../book/src/chromecast.md)
beskriver reglagen och [historiska granskningen](2026-10-30-cast-diagnostics.md)
innehåller tidigare fel. Releaseanteckningar är inte bevis för fysisk acceptans.

## Verifierat läge

| Mottagare | Identifiering | Resultat och åtgärd |
|---|---|---|
| Bedroom TV | Användaren anger Chromecast med Google TV | Tidigare fungerande remux/Automatisk; 12 november fungerade 2 Mbps. Testa varje ändring även här. |
| Bedroom display | Användaren anger Google Hub | 12 november bekräftad uppspelning vid 2 Mbps och byte till textspår 38. Automatisk misslyckas enligt användaren och är inte löst. Använd tills vidare 2 Mbps för den testade filen. |
| Kitchen display | Exakt modell inte fastställd | 12 november före senaste tidslinjefixen: samma IDLE-fel vid 2 Mbps. Måste testas igen med nya bygget; Bedroom-resultatet bevisar inget för Kitchen. |
| Andra modeller | Okänd | Kontrollera modell, firmware, codecgränser och fysisk uppspelning separat. |

Logg `4b83e14d-2498-4a5e-ba85-7bec62baaf30`, försök 3, UTC 17:23:13:
Bedroom display, medie-ID 3 (How to Build the Box), start 268593 ms,
maxBitrate 2000000, ljudspår 1, undertextspår 38. Första förhandling 503;
plats frigörs och andra förhandling ger 200. LOAD skickas 17:23:18.420,
`load.ready` 17:23:22.931, ny ström PLAYING 17:23:32.077 och position
avancerar 268–305 s. PLAYING före det nya LOAD avser gamla strömmen och ska
inte räknas som bevis för det nya undertextbytet. `tracks=8`,
`burnedSubtitleStreamIndex=null`: textspår skickades, inte inbränd text.
Loggen bevisar inte synlig/rätt synkroniserad text eller långvarig A/V-synk.
Serverexport: `8c6d64be-07d8-4947-b3f7-f1118dfb6f14`.

## Enhetskrav och Automatisk

Google anger H.264 High ≤4.1, 720p/60 för Nest Hub och 720p/30 för Hub Max.
Chromecast med Google TV har högre gränser. Se
[Googles mediestöd](https://developers.google.com/cast/docs/media), kontrollerat
2026-11-12. Det är modellkrav, inte bevis att vår aktuella ström accepteras.

MBL använder idag samma `chromecast`-profil för alla Google-mottagare:
`generic-hls.toml`, H.264 upp till 1920×1080, nivå 4.1, 8 bitar, inget HDR,
högst två ljudkanaler; HLS-mål H.264/AAC. **Den aktiva webbklienten har ännu ingen automatisk
Hub-specifik profil.** Serverarbetskopian har nu separata namngivna profiler
(revision 1), bland annat `nest-hub` och `nest-hub-max`; de används först när
klienten skickar respektive profilnamn. Se genomförandeplanens P1-status. Friendly name loggas för korrelation; namnet
används inte för att bestämma codecstöd. Webbläsarens codecprober beskriver
webbläsaren, inte mottagaren.

**Automatisk betyder inget uttryckligt kvalitetsbegränsande maxBitrate från
reglaget. Det betyder inte alltid adaptiv strömning eller automatisk
modellidentifiering.** Serverns profil/nätverk och källan avgör vägen:

| Beslut | Leverans | Viktig skillnad |
|---|---|---|
| Direct Play | Signerad direktlänk till kompatibel originalfil | Ingen videoomkodning; verklig modell måste stödja filen. |
| Kopierad video / remux, eventuellt nytt ljud | HLS fMP4 med FFmpegs verkliga EVENT-lista för Cast `waitForReady` | Originalets upplösning och nyckelbilder behålls. |
| Videoomkodning | H.264/AAC HLS fMP4, normalt sexsekunderssegment | Kan skala ned och korrigera codec/bitrate. |
| Adaptiv videoomkodning | Två synkroniserade HLS-nivåer när villkoren uppfylls | GPU eller begränsad x86-mjukvaruväg; inte generellt aktiverad på Pi. |

Källan i försöken är H.264 1920×960 med E-AC-3 5.1 i MKV. Automatisk kan
kopiera den videon medan ljudet blir stereo. Vid 2 Mbps tvingar källans högre
bitrate fram kodning; hämtade fragment visar H.264 High 3.1, 1280×640,
yuv420p, AAC-LC stereo 48 kHz. 4/8 Mbps är inte likvärdigt med 2 Mbps och
kan ge annan upplösning eller kopierad video.

**Öppet fel:** Automatisk på Bedroom display. Originalets upplösning över
Hub-gränsen och EVENT/remux-vägen är kandidater; den senaste bifogade
Cast-loggen gäller 2 Mbps, inte det misslyckade Auto-försöket. Fastställ inte
orsaken utan en separat Auto-logg och de faktiska segmenten.

## Flödet från Play till mottagare

1. Lokal webbläsaruppspelning förhandlar med webbläsarprofilen.
2. Cast-knappen öppnar Google Sender SDK:s väljare. Default Media Receiver
   har app-ID `CC1AD845`; MBL har ingen egen receiver. Native Remote Playback
   är en separat fallback som behåller webbläsarens mediaelement.
3. `receiver.selected` loggar namnet. `POST /api/v1/playback` använder
   `profile=chromecast`, hela millisekunder, ljud/textval, ljudoffset,
   kvalitetsgräns och `waitForReady`. Servern förbereder HLS-data.
4. Vid uttryckligt fel om maximalt antal videotranskodningar frigörs den
   tidigare sessionen och förhandlingen provas exakt en gång till. Detta
   gäller även byte under pågående Cast. Andra serverfel utlöser inte den
   destruktiva kapacitetsåtgärden.
5. Media-URL måste nås av mottagaren själv. Relativa länkar mot privat
   HTTPS-origin skrivs om till HTTP; 8443 blir 8420 (18443 blir 18421).
   Redan absoluta URL:er passerar befintlig URL-hantering utan denna omskrivning.
   Kontrollera alltid faktiska `load.send.url`, public_url, port och CORS.
   Att webbläsaren accepterat ett lokalt TLS-certifikat hjälper inte Cast.
6. LOAD använder BUFFERED, längd, startposition och autoplay. Muxad fMP4 får
   inte godtyckligt nya `hlsSegmentFormat`/`hlsVideoSegmentFormat`-hints:
   tidigare experiment gav legacy MPL-felet `Unable to derive timescale`.
7. LOAD-anropets svar har 30 s gräns. Därefter väntar klienten på rätt
   contentId och `isMediaLoaded=true`, för närvarande högst 60 s.
   `load.accepted` betyder bara kvittens. Kräv `load.ready`, PLAYING och
   stigande position efter det aktuella LOAD.
8. Lokal källa töms vid överlämning för att undvika dubbelt ljud. Om den
   måste stängas tidigare för kapacitet sparas positionen före tömning.
   Misslyckad ersättning efter frigjord kapacitet återställer lokal källa;
   tidigare Cast-session kan då inte garanteras finnas kvar.
9. Progress, paus och stopp använder mottagarposition. Även pausad session
   förnyas. Webbläsaren måste hållas aktiv för styrning/förnyelse; lång sömn,
   cachekvot och nätverksavbrott är separata felkällor.

## Undertexter, ljud och kvalitet

Externa textspår använder WebVTT och kan uppdateras via `editTracksInfo`.
Vid behov laddas ny serversession. Högst åtta textspår skickas; valt spår
måste finnas med. Bildtext kräver inbränning och videoomkodning. Ett
inbränt spår får inte skickas som ett externt receiver-track-ID.

Ljudspår, ljudsynk, kvalitet och vissa textbyten kan kräva ny session. Ny HLS
laddas vid serverns förberedda position; använd inte en senare receiverposition
för fragment som ännu inte förberetts. Pausläget ska följa bytet. En Pi med en
transkodningsplats kan inte skapa två kodningar samtidigt: ovanstående
kapacitetsöverlämning ger ett laddningsavbrott, inte sömlös övergång.

## Vad vi lärt oss och inte ska återinföra

- 2026-11-12: master annonserade 1280×960 trots verklig 1280×640.
  Enkel omkodning måste skala den annonserade höjden också.
- Samma fångst: första EXTINF var 3 s men video i första fragmentet 6,006 s.
  FFmpeg-argumenten satte 6 s; Cast får inte syntetiska 3+3-startsegment.
  Korrigeringen följdes av fungerande Bedroom display vid 2 Mbps. Två
  metadataändringar levererades tillsammans: isolerad kausalitet är inte bevisad.
- Kopierad video kan ha t.ex. 7,341 s och 7,465 s segment trots önskade 6 s.
  Använd verkliga FFmpeg-tider, inte uppskattade VOD-tider, för remux.
- Längre LOAD-bekräftelse (15→60 s) och helfilsläsning som i 1.2.0 löste inte
  display-felet. Strömmande segmentleverans är återställd. Content-Length
  finns fortfarande; HTTP 200 betyder inte att mottagaren avkodade kroppen.
- E-AC-3/six-channel/Matroska i förhandsbesked beskriver källan. Inspektera
  utdata innan ett sådant meddelande tolkas som fel codec på mottagaren.
- v1.2.0:s releaseanteckningar om progressiv MP4/faststart motsvarar inte
  taggens aktiva remuxväg, som använder HLS. Använd kod och fångad trafik.
- Överskriv inte sparad lokal position efter att mediaelementet tömts till 0.
- Ett fungerande Google TV-test verifierar inte Nest Hub, och tvärtom.

## Reproducerbar felsökning

1. Anteckna installerad binär/commit, serverplattform, FFmpeg-version,
   mottagarmodell/firmware, fil och item-ID. Versionsnumret ensamt räcker inte
   för lokala testbyggen. Spara binärens SHA-256 vid installation.
2. Börja med 2 Mbps, från noll, text Av. Exportera loggar direkt. Prova sedan
   samma fil/enhet med sparad position, text, paus och sist Automatisk.
   Ändra bara en faktor åt gången. För varje test: bild, ljud, synk och tid.
3. Kopiera **Statistik → Cast-diagnostik** och **Administration →
   Diagnostikloggar**. Korrelera UTC, försök och receiver.selected.
   Exporten är begränsad; tidiga försök kan redan ha fallit ur serverloggen.
4. Matcha serverns fulla logg på session, mottagar-IP och user agent.
   Fuchsia/CrKey och webbläsaren är olika klienter. Följ master → media-lista
   → init → segment 0 → målsegment → efterföljande segment.
5. Om bara init/segment 0 hämtas: spara exakt playlist, init och fragment
   medan sessionen lever. Jämför EXTINF, TARGETDURATION, RESOLUTION, CODECS,
   nyckelbilder och tidsstämplar med FFprobe. Redigera inte init efter
   att mottagaren hämtat den. Sessioner kan städas efter stopp.
6. Analysera lokalt, utan att publicera signerade URL:er:

   ```bash
   cat init.mp4 0.m4s > first.mp4
   ffprobe -v error -show_streams -show_format -of json first.mp4
   ffmpeg -v error -threads 1 -i first.mp4 -f null -
   ```

   Detta visar avkodbarhet i FFmpeg, inte acceptans på Google-hårdvara.
   Gör även ett separat prov av init + målfragment vid återupptagning.
7. Kräv fysisk acceptans efter kodändring; en SDK-mock kan inte testa
   mottagarens demuxer. Dokumentera negativa resultat, inte bara gröna tester.

| Symtom | Kontrollera först |
|---|---|
| 503 före LOAD | Transkodningsplats och om retry efter stopp lyckas. |
| LOAD accepterat, IDLE/loaded=false | Mottagarens mediahämtningar och faktisk ström; kvittens räcker inte. |
| 404 efter paus | Sessionens livstid, cachekvot, heartbeat och rätt sessions-ID. |
| Ljud på datorn | Lokal källa pausad/tömd; skilj native Remote Playback från Google Cast. |
| Bild/ljud ur synk | Tidsstämplar, verkliga segmenttider, ljudoffset, omstart och textbyte. |
| 2 Mbps fungerar, Auto misslyckas | Jämför remux mot omkodning, källupplösning, codecprofil och EVENT-lista. |

Genomförandeplan: [Enhetsanpassad Cast och adaptiv Automatisk](../plans/cast-device-profiles.md).

## Källkarta och återstående arbete

- `web/src/utils/cast.ts`: SDK, LOAD, mottagartillstånd, URL och textspår.
- `web/src/components/Player.tsx`: förhandling, kapacitetsöverlämning, position,
  kvalitet och återställning. Tester i `Player.cast.test.tsx`.
- `web/src/utils/castDiagnostics.ts`: maskering och begränsade minnesloggar.
- `crates/mbl-playback/src/profile.rs`, `profiles/generic-hls.toml`,
  `src/decide.rs`: profil, formatbeslut och breddval.
- `crates/mbl-service/src/playback.rs`: HLS-session, höjd, tidslinje och ABR.
- `crates/mbl-transcode/src/args.rs`, `session.rs`: FFmpeg och fragmentberedskap.
- `crates/mbl-stream/src/hls/`: spellisterendering och HTTP-kroppar.
- `crates/mbl-api/src/routes/stream.rs`: mediarouter och hämtloggar.

Nästa: separat Auto-fångst på Hub, modellriktig profil/kapat format utan att
förlita sig på friendly name, korrekt CODECS från utdata (master använde
avc1.640028 medan uppmätt kodning var High 3.1), Kitchen-verifiering och
längre tester av paus/sökning/A-V-synk. Full Rust-regression återstår efter
minnesbegränsad avbruten testkompilering. Fysiskt bekräftad fix får inte
förväxlas med färdig release eller fullständig acceptans på alla modeller.
