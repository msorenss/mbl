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
rensas. Profiler med biblioteks- eller ratingbegränsningar ser endast
publicerade inspelningar som de har tillgång till. Material med okänd rating
följer serverns vanliga profilpolicy.

Äldre färdiga filer under samma inspelningskatalog katalogiseras vid
serverstart. Saknade, oläsbara eller felplacerade filer får ingen spelknapp.
En vanlig biblioteksskanning importerar inte DVR-katalogens partiella filer;
inspelningstjänsten sköter dess innehåll. **Delete** tar bort både sparad fil
och katalogpost efter bekräftelse. **Cancel** avbryter en väntande/pågående
inspelning.

Serieregler sparar villkor för kanal, säsong och nya avsnitt och planerar
matchningar i den aktuella guiden. Fortsatt regelmatchning efter senare
EPG-importer och automatisk retention återstår.

## Virtuella kanaler och fortsatt verifiering

Virtuella kanalobjekt och deras beräknade playhead/EPG kan hanteras. Faktisk
loopande uppspelning och övergång mellan bibliotekstitlar återstår;
spelknapparna är spärrade tills den kedjan finns.

Lokala syntetiska MPEG-TS-källor verifierar relä, fragment, kapacitet och DVR.
Fysisk HDHomeRun/IPTV-, browser-, WAN- och receiveracceptans samt
profil-/ratingkontroll för Live TV återstår i
[restlistan](../../audits/remaining-work.md).
