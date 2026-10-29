# Live TV, EPG och DVR

Administratörer kan upptäcka HDHomeRun-enheter och importera M3U-kanaler i
fliken **Tuners & Sources**. XMLTV-text kopplar program till kanalernas EPG-ID.
Guiden visar tidsfönster, nu/nästa och programsökning. Från ett program kan du
välja direktvisning eller schemalägga en inspelning med start-/slutmarginal.

## Direktvisning

Välj spelknappen vid en vanlig tunerkanal. **Preparing live playback…** visas
medan servern förbereder den första färdiga videobiten. **Cancel** avbryter
väntan. Käll- och kapacitetsfel visar serverns felorsak.

Servern hämtar HTTP/HTTPS-källan och skapar HLS med riktiga fMP4-fragment.
Webbläsaren får en adress på MBL-servern och behöver ingen egen åtkomst till
tunerns adress, inloggning eller CORS-inställningar. Basprofilen använder
mjukvarukodad H.264, högst 1280 × 720 vid 25 fps, deinterlacing av märkta
interlaced-bilder och AAC-stereo. Kodningen delar serverns videoencodergräns
med vanlig VOD-uppspelning. Hårdvaruacceleration/HDR för liveflödet återstår.

Live-spelaren har vanliga videoreglage och begränsad nätåterhämtning. Vid
bestående fel släpps reservationen och du kan välja kanalen igen. Stängning,
kanalbyte eller naturligt slut släpper processen. Kanalbyte väntar på den
gamla processen så att dess encoderplats kan återanvändas.

Klienten förnyar sin ägda reservation var 15:e sekund. Utan förnyelse löper den
ut efter 45 sekunder, oberoende av mediehämtningar. Serverns periodiska städning
stänger processen och tar bort cachen. Medieadressen är en kortlivad hemlig
capability-länk; dela den inte. Diskkvot och serverstopp kan också stänga flödet.
HLS-cachen är ett litet rullande fönster, inte beständig timeshift/inspelning.
Vid lång paus eller browser-/OS-sömn kan kanalen behöva väljas igen.

## Profiler och programgränser

En profil med åldersgräns får bara starta ett pågående tillåtet program i
TV-guiden. Utan aktuellt guideprogram nekas start. Okänd rating följer den
befintliga ratingpolicyn; PG eller högre kan tillåta program utan rating.
Guiden döljer otillåtna program och spärrar spelknappen när aktuell sändning
inte är tillgänglig. Öppen guide uppdateras var 30:e sekund.

Begränsade profiler spelar ett program i taget. Vid guidens programslut stängs
både bokningen och webbspelaren. Välj kanalen på nytt för nästa tillåtna
program. Detta gäller också när nästa program har samma tillåtna rating.
Profiler utan åldersgräns kan fortsätta över programgränser.

Inloggning och aktuell ratingpolicy kontrolleras också vid förnyelse,
mediehämtning och serverunderhåll. Utloggning eller indragna rättigheter
stänger flödet. Icke-administratörer får serverreläets adress vid uppspelning;
råa kanal-/tuneradresser visas bara för administratörer. Kontrollen följer
EPG-tider och rating från den konfigurerade källan.

## Inspelning och tunergränser

DVR använder samma tunerbokning som livevisning. En upptagen tuner överbokas
inte. Aktiva reservationer behåller sin plats; en inspelning som inte kan få
plats markeras **Failed** med orsaken. Det finns ännu ingen inställning för
att automatiskt avbryta en tittares livevisning till förmån för DVR.

Schemat och status sparas i databasen. En bakgrundsprocess hämtar kanalen med
FFmpeg och skriver MPEG-TS under serverns datakatalog `recordings/`. Avbokning
stoppar processen och städar dess partiella fil. Vid serverstopp/restart
markeras avbrutna inspelningar som misslyckade; en missad inspelningsperiod
markeras också som misslyckad.

## Spela en sparad inspelning

När filen är färdig kontrollerar servern den med ffprobe och sparar verklig
medielängd, bild-/ljudspår och format. Inspelningen läggs automatiskt i
biblioteket **TV recordings**. På fliken **DVR Recordings** väljer du **Play**.
Den vanliga spelaren används, med återupptagning, kvalitet, ljudspår och
uppspelningshistorik. Pågående och misslyckade inspelningar går inte att spela.
Listan uppdateras var tionde sekund medan DVR-fliken är öppen.

Bibliotekets behörigheter och inspelningens åldersgräns gäller även här.
Rating och beskrivning sparas vid schemaläggning, så de finns kvar när guiden
rensas. Färdiga inspelningar delas genom bibliotekets behörigheter och ratingpolicy.
Väntande och pågående inspelningar visas för ägaren och administratörer. Bara
ägaren eller en administratör kan avboka eller radera en inspelning. Material
med okänd rating följer serverns vanliga profilpolicy.

Äldre färdiga filer under samma inspelningskatalog katalogiseras vid
serverstart. Saknade, oläsbara eller felplacerade filer får ingen spelknapp.
En vanlig biblioteksskanning importerar inte DVR-katalogens partiella filer;
inspelningstjänsten sköter dess innehåll. **Delete** tar bort både sparad fil
och katalogpost efter bekräftelse. **Cancel** avbryter en väntande/pågående
inspelning.

## Inspelningsformat, undertexter och radio

Inspelaren sparar MPEG-TS med kopierade bild-, ljud- och undertextspår. Flera
ljudspråk och DVB-bitmaptext finns kvar som val i den vanliga spelaren.
Webbläsare får DVB-text inbränd när spåret väljs; **Off** visar bilden utan
text. MPEG-2/AC-3 och HEVC/AAC har verifierats med syntetiska källor. Spelaren
anpassar leveransen till klienten, till exempel H.264/AAC i HLS.

Radio med enbart ljud kan också spelas in. Inspelningen visas som ett
ljudspår i samma DVR-bibliotek och öppnas med **Play** eller **Play partial**.
Spelaren visar en ljudvy med titel, tidslinje, volym och ljudspår. Videokvalitet
och bildförhandsvisningar visas inte för ljud. Bibliotekets profilregler,
historik och ägarstyrd radering gäller även radio. Kortare inspelningar
följer spelarens vanliga gränser för att spara återupptagningsposition.

DVB-stödet innebär inte att alla undertextformat kan sparas i MPEG-TS.
Text-TV, inbäddad CEA-text och generell SRT/ASS-/formatkonvertering är ännu
inte verifierade DVR-flöden. Fysisk tuner-/browser-/mottagaracceptans återstår.

## Profiler och schemaläggning

Biblioteket skapas vid den första schemaläggningen av en inspelning eller
serieregel. Administratören kan sedan ge hanterade profiler tillgång via
användarinställningarna. En profil med åldersgräns måste välja ett tillåtet
program från guiden. Servern använder guidens kanal, titel, tid och rating;
extra start-/slutmarginal ignoreras för dessa profiler så att inspelningen
inte omfattar ett annat program. Fria tider kan användas av profiler utan
åldersgräns. Avslutade program går inte att schemalägga.

Behörigheterna kontrolleras på nytt innan inspelningen startar och under
pågående inspelning. Återkallad åtkomst eller borttagen ägare stoppar jobbet.
Det automatiskt hanterade inspelningsbiblioteket kan döpas om, men dess
sökväg eller själva biblioteket ändras inte via vanliga biblioteksreglage.
Använd DVR-fliken för att ta bort sparade filer. Färdiga inspelningar finns
kvar även när deras tuner/kanal tas bort; väntande jobb får ett källfel.

## Serieregler och lagringspolicy

**New Series Rule** kan begränsas till kanal, säsong, episodiska program och
guidens markering för nya sändningar. Servern matchar de kommande 14 dagarna
var 15:e sekund, även efter nya guideimporter. En tidigare schemalagd sändning
behåller sin regel; överlappande regler gör inte flera kopior för samma
profil. Återimport av guiden gör inte heller dubbletter.

En schemakonflikt visas som **Failed** med orsaken. När kapacitet finns kan
ägaren välja **Retry** före programmets slut. Om en sändning avbokas läggs den
inte tillbaka av serieregeln. Ett uttryckligt nytt inspelningsförsök är
möjligt från guiden. **Delete Rule** avbokar väntande jobb men behåller redan
pågående och färdiga inspelningar.

**Keep latest saved recordings** tar bort äldre sparade Completed/Partial-filer för
regeln. Tomt värde behåller alla. **Delete after my profile watches** använder
regelägarens tittarhistorik; en annan profil som tittar utlöser inte radering.
Båda valen raderar sparade filer automatiskt och är avstängda från början.
Aktiva och pausade spelarstarter skyddas under raderingen. Stora arkiv kan
kräva flera bakgrundskörningar; högst 16 raderingar provas per körning.


## Ofullständiga inspelningar

**Partial** betyder att en giltig del sparades men att hela fönstret inte
kunde spelas in, eller att källan/encodern rapporterade fel. Orsaken och den
verkliga längden visas i DVR. **Play partial** öppnar den vanliga spelaren.
Bibliotekets titel och beskrivning märker också att inspelningen är ofullständig.
En tom eller oläsbar fil blir **Failed** och får ingen spelbar katalogpost.

Servern jämför probad medielängd med hela den schemalagda tiden. Den tillåter
5 procent av fönstret för paketgränser/startjitter, minst 100 ms och högst 2 s.
Detta upptäcker korta källavslut men garanterar inte att en ström saknar interna
paketförluster eller felaktiga tidsstämplar. Hängande källor stoppas vid
schemalagt slut, även om deras medieklocka står stilla.

**Retry** kan boka ett nytt försök så länge programmet fortfarande pågår.
Den gamla delen behålls och det nya försöket får ett nytt fil-ID; delarna
sammanfogas inte automatiskt. Nytt försök behåller ägaren, marginalerna och
en kvarvarande serieregel med dess lagringspolicy. Har regeln tagits bort blir
försöket manuellt med samma ägare. Vid ärvd automatisk radering visar webben
policyn för bekräftelse. Admin-retry följer också ägarens aktuella rättigheter. Keep-latest och delete-after-watched gäller
både Completed och Partial, med skydd för pågående/pausad uppspelning.

Serverstopp och indragna rättigheter publicerar inte automatiskt delmedia.
Filer som ännu inte publicerats efter ett serveravbrott visas fortsatt som
Failed. Äldre Completed-arkiv omklassas inte retroaktivt.

## Förhandsvisning och diskgränser

Retry hämtar en kanonisk plan innan något skapas. Samma plan finns i
`POST /api/v1/livetv/recordings/preview` och MCP `schedule_recording` med
`dry_run: true`. Guidens titel och tider används när ett program-ID anges;
klientens egna värden kan inte ersätta dem. `retryOf` (REST) eller `retry_of`
(MCP) väljer ett tidigare misslyckat/ofullständigt försök och bevarar dess
ägare och kvarvarande regel. Preview visar effektiv padding, retention,
konflikt/befintlig bokning och diskstatus. Ett null libraryId betyder att
biblioteket behöver skapas vid faktisk schemaläggning. Planen skapar inga
kataloger eller bokningar och är ingen garanti för framtida kapacitet.

I serverns TOML-fil kan DVR-gränser anges:

```toml
[dvr]
min_free_mb = 512
# Valfritt tak per inspelning; utelämna för inget separat filstorlekstak:
# max_recording_mb = 8192
```

Värdena anges i MiB. Motsvarande miljövariabler är
`MBL_DVR__MIN_FREE_MB` och `MBL_DVR__MAX_RECORDING_MB`. Starta om servern efter
konfigurationsändring. Noll min_free_mb stänger av reserven; noll filstorlekstak
är ogiltigt. MCP `get_settings` visar den tillämpade policyn och aktuell ledig
yta under `dvr`.

Diskvakten kontrollerar före start och varje sekund under inspelning.
Gränsstopp kan lämna en spelbar Partial med tydlig orsak. En ny inspelning som
inte kan starta får Failed och kan prövas igen efter att plats frigjorts.
Framtida schemaläggning är fortfarande möjlig när disken är låg; den reserverar
inte framtida utrymme. FFmpeg får också ett byte-tak baserat på startens lediga
yta och eventuellt konfigurerat tak. Paketgränser kan ge ett mindre överskridande.

Inga sparade filer raderas automatiskt för att lösa låg disk, utöver redan
valda seriereglers retention. Vakten pollar och reserverar inte yta mot andra
samtidiga skribenter. Den ersätter inte en helarkivskvot eller en prognos över
alla framtida inspelningar.

## Virtuella kanaler och fortsatt verifiering

Virtuella kanalobjekt och deras beräknade playhead/EPG kan hanteras. Faktisk
loopande uppspelning och övergång mellan bibliotekstitlar återstår;
spelknapparna är spärrade tills den kedjan finns.

Lokala syntetiska MPEG-TS-källor verifierar relä, fragment, kapacitet och DVR.
Fysisk HDHomeRun/IPTV-, EPG-klocksynk-, browser-, WAN- och receiveracceptans återstår i
[restlistan](../../audits/remaining-work.md).
