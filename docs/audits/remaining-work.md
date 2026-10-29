# Återstående arbetsområden efter kod- och gränssnittsgranskningen

Den tidigare uppskattningen ”ungefär 25” är här uppdelad i 25 namngivna
arbetsområden. En rad kan innehålla flera deluppgifter. Raden kryssas först när
hela flödet är implementerat och relevant verifiering är klar. M11 ingår inte.
Rättade fel i tidigare batcher finns i huvudrapportens statussektioner.

Status efter batch 24 (en ny lokal checkpoint samlar batch 20–24 ovanpå `fcdd83b`, som omfattar batch 1–19): 2 av dessa 25 områden färdigimplementerade, 23 fortfarande öppna (flera delvis åtgärdade). Batchernas riktade verifiering finns i granskningsrapporten. Slutverifieringen inför release nedan återstår.

- [ ] 01. Live TV: **Batch 18:** serverägt HLS-relä, avkodningsbar H.264/AAC, ägd 45 s lease med periodisk städning, gemensam encodergräns med VOD och tunerbokning med DVR. Kanalbyte/stopp/fel/cancel städar process och cache. **Batch 21:** aktuell inloggning/rating kontrolleras vid start, beredskap, förnyelse, medieanrop och underhåll. Åldersbegränsade profiler binds till ett godkänt program och väljer kanalen på nytt vid programslut; webbens buffert stängs också. Guide/MCP-metadata filtreras och råa källadresser döljs för icke-admin. Verklig tuner-/EPG-klocksynk, fysisk/browser-/WAN-acceptans och vidare liveprofiler återstår; liten rullande cache är inte beständig timeshift.
- [ ] 02. DVR: **Batch 19–20:** riktig inspelning och uppspelning via katalog/spelare, korrekt probe-duration, delad tunerbokning, profilmedveten schemaläggning och ägarstyrd CRUD. Återkommande 14-dagars seriematchning, paginering/deduplicering av guideimport, konflikt/Retry och verklig keepMax-/watch-retention är implementerade. Aktiva/pausade uppspelningar skyddas, fil-ID återanvänds inte efter avbokning, arkiv överlever tuner-/kanalborttagning och hanterade biblioteksroots skyddas. **Batch 22:** tidig EOF och encoderfel ger korrekt Completed/Partial/Failed efter probing; giltig del kan spelas och raderas med samma ACL. Schemalagd sluttid stoppar hängande källor, och behörigheter kontrolleras efter analysen. **Batch 23:** Retry behåller ägare/serieregel/retention och visar kanonisk REST/MCP-preview utan sidomutationer. Diskreserv, per-filgräns och bevakning före/under capture finns. **Batch 24:** DVB-bitmaptext och flera ljudspråk bevaras, radio blir spelbar Track med samma profil-/historik-/raderingsflöde och tydlig ljudvy. MPEG-2/AC-3, MP2-radio och HEVC/AAC verifieras. HLS anger rätt codec, och kopierat ADTS-AAC konverteras korrekt till fMP4. **Återstår:** bredare transport-/codecprofiler, generell SRT/ASS-konvertering, text-TV/CEA-text, källors interna paket-/tidsstämpelfel, helarkivskvot/prognoser, städning av Failed-filer och säker återställning av opublicerat material efter serveravbrott; fysisk tuner-/browser-/mottagaracceptans återstår.
- [ ] 03. Workers/noder: ersätt mockkatalog, verifiera verklig offload/handover/fallback.
- [x] 04. Trickplay: verkliga genererade artefakter i stället för platshållarvägar. **Batch 13:** riktiga nyckelbilder, idle-jobb efter skanning/vid cachemiss, säker cacheinvalidering och autentiserad tidslinjeförhandsvisning. Verklig video, korta klipp, bildarksgräns, jobbkö, ACL/rating och klientlivscykel verifierade; manuellt webbläsarprov ingår i slutkontrollen.
- [ ] 05. Musik: kö, minispelare, artist/album, now-playing och texter.
- [x] 06. Foton: tidslinje, lightbox och bildspel. **Batch 10:** API/UI och rating-/ACL-/pagination-/bildresursregressioner verifierade; manuellt webbläsar-/tillgänglighetsprov ingår i slutkontrollen. Originalformat används; ingen RAW-konvertering/fotominiatyrpipeline.
- [ ] 07. Metadata: redigering, fältlås, fix-match och bildval.
- [ ] 08. Undertexter: sök/nedladdning, autosync och generering med jobbstatus. **Batch 14:** befintliga text-/bildspår kan väljas i spelaren; korrekt WebVTT-konvertering och inbränning av inbyggd/extern text med root-kontroll verifierad. Skapande-/sök-/synkflöden återstår.
- [ ] 09. Spelare: kvalitetsbyte, ljudsynk, Up Next och verifierbar statistik. **Batch 14:** ljudspårsbyte under uppspelning, inbränd text/Off, spårbevarande vid kvalitet/Cast, paus under förhandling och HLS för ljudlös video. Fysisk PGS-/mottagaracceptans återstår. **Batch 15:** webbklienten väntar på läsbar direktkälla eller färdig första HLS-fragment/init innan källbyte. Startfel, timeout och avbrott städar kandidaten; positionen hämtas på nytt vid bytet och obekräftade kandidater skriver ingen historik. **Batch 16:** periodiskt HLS-underhåll är inkopplat i serverlivscykeln. Paus/buffring förnyar ägarens lease; övergivna sessioner frigör process/katalog, spelarlista och WAN-andel. Lokal timeout/eviction ger återupptagning vid samma position, och serverstopp spärrar nya starter innan barnprocesser städas. **Batch 17:** fatala nätfel får högst två appåterförsök per källa med backoff och tidsgräns. Permanenta HTTP-/avkodningsfel stoppar laddningen och släpper serversessionen. Manuell Retry bevarar position/spår/synk/kvalitet och väntar på städning; paus och gamla callbacks är skyddade. **Batch 19:** färdiga DVR-filer använder vanliga spelaren med probad mediemetadata, återupptagning och historik. **Batch 20:** profilmedveten DVR, fortsatt seriematchning och retention är implementerade med skydd för spelarens livscykel. **Batch 21:** Live TV:s profil-/programgränser och återkallade medielänkar är implementerade. **Batch 22:** tydliga, spelbara delinspelningar efter tidiga källavslut och hårt stopp vid schemalagt slut. **Batch 23:** Retry-policy/ägare och kanonisk preview är rättade; diskreserv och filstorlekstak skyddar capture. **Batch 24:** radio och DVB-undertexter går genom samma spelare; ljudvy, rätt audio-HLS/HEVC-deklarationer och ADTS-AAC till fMP4 är rättade. **Nästa core-steg:** vidare transport-/undertextprofiler samt Failed-städning/återställning och återstående arkiv-/lagringspolicy. Fysisk mottagaracceptans och bakgrunds-/sovande webbläsare återstår. **Delvis:** kvalitetsomförhandling från batch 6. **Batch 12:** lokal ljudfördröjning ersatt med signerad serverjustering −10..+10 s som följer kvalitetsbyte och Google Cast-förhandling. Verkligt FFmpeg-signaltest täcker båda riktningar och sökning. Verifierad avkodar-/bandbredds-/receivertelemetri och fysisk ljudsynkacceptans återstår. **Batch 11:** ägarbunden serverdiagnostik för nätklass, HLS-backend/process/omstarter; korrekt lokal buffert och upplösning; inga lokala värden presenteras som Cast-telemetri. **Batch 8:** Up Next och lokal opt-in-fortsättning finns; manuell nästa under Cast är inkopplad. **Batch 9:** opt-in-fortsättning på Google Cast är implementerad och regressionstestad; fysisk acceptans samt native receiverautomatik återstår. **Batch 7:** atomisk kapacitetskontroll och avvisning av kvalitetsbyte vid full kapacitet utan förhandsavstängning av gammal session.
- [ ] 10. Samlingar/spellistor: ordning, smarta filter, kö och automatisk samlingskoppling.
- [ ] 11. Home Assistant: inkopplad MQTT-livscykel och kommandon.
- [ ] 12. Admin: jobb/historik/dashboard och webhookadministration.
- [ ] 13. Companion: spelaroverlay och verklig kontext-/händelsekedja.
- [ ] 14. X-Ray/recap/sleep guard/klipp: sammanhängande webbflöden och jobb.
- [ ] 15. Matchmaker: lobby, anslutning, swipe, match och uppspelning.
- [ ] 16. Requests: TMDB-sökning, faktisk Arr-leverans och lagringsanalys.
- [ ] 17. Virtuella kanaler: biblioteksval och faktisk övergång mellan objekt.
- [ ] 18. Fjärrspelare: DLNA-/mottagarval och verifierad kommandokedja.
- [ ] 19. Fjärråtkomst: rekommendationswizard och direkt-/relästatus, tydliga installationsflöden.
- [ ] 20. Profilbyte: säker PIN-baserad sessionsväxling; administration av PIN finns.
- [ ] 21. Detaljvy: titellogga, full editions-/extrashantering.
- [ ] 22. Språk: täck hårdkodade produktsträngar i alla fem språk.
- [ ] 23. Tillgänglighet: dialogfokus, skärmläsare, mobil och tangentbordsnavigering.
- [ ] 24. Inställningar: korrekt kontobyte och verifierad tema-/preferenslagring.
- [ ] 25. Onlineproviders: verkliga adapterflöden; nuvarande demoetiketter räknas inte som integration.

## Git-kontrollpunkt 2026-10-29

Den samlade Rust-regressionen passerar: 371/371 tester, inga överhoppade.
`just web-check` passerar med 138 tester, TypeScript, produktionsbygge och
42,24 KB initial JavaScript gzip. Hela arbetsytans Clippy, fmt, OpenAPI,
design tokens och MCP-paritet 147/147 passerar också. Kontrollpunkten sparar arbetet i Git;
slutverifieringen inför release nedan upprepas efter fortsatta fixar.

## Prioritet

Enligt användarens senaste instruktion: färdigställ spelarrelaterade delar först (09, 01/02, 04, 08 och 18 där de berör uppspelning), därefter övriga områden.

## Slutverifiering inför release

- [ ] Full Rust- och webbregression samt fmt/Clippy/OpenAPI/tokens/MCP-paritet på den slutliga releasekandidaten. Kontrollpunktens gröna resultat ska upprepas efter fortsatta fixar.
- [ ] Renderade användarflöden med riktig server och testmedia.
- [ ] Storybook/axe och manuell responsiv-/tillgänglighetskontroll.
- [ ] Fysisk Cast-/native-mottagare och tuner/DVR; verklig WAN- och workerprovning.
- [ ] Rapport, specifikation och releasenoter överensstämmer med verifierad funktion.

Hailo-bildanalys är CPU-heuristik enligt rättad status. Verklig NPU-inferens
behöver separat modellimplementation och hårdvaruverifiering för att det tidigare
specifikationspåståendet ska kunna återinföras. Ingen sådan acceptans är utförd.
