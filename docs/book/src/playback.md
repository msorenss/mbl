# Webbspelaren och förhandsbilder

Starta en titel från dess detaljvy. På tidslinjen kan du peka med musen eller
fokusera sökreglaget med tangentbordet för att visa en förhandsbild. Pilarna
ändrar positionen när reglaget har fokus. Bilden följer aktuell medieversion,
även om servern har valt en optimerad fil.

Förhandsbilder genereras från videons verkliga nyckelbilder, med en ruta per
10 sekunder, i JPEG-ark med 10 × 10 rutor. De är grova sökförhandsbilder;
nyckelbildernas avstånd avgör precisionen. Korta klipp får också en bild.

En skanning med nya eller ändrade videor köar generering som bakgrundsarbete.
Saknas förhandsbilder när du öppnar tidslinjen köas de då. Spelaren visar att
bilderna väntar på ledig server och provar igen en begränsad tid. Uppspelningen
kan fortsätta oberoende av förhandsbilderna. Arbetet använder en CPU-tråd och
avbryts för att återupptas senare om uppspelning eller termisk belastning kräver
det. Sådana pauser förbrukar inte jobbens felåterförsök.

Misslyckad avkodning ger ingen platshållarbild. Vid fel kan du prova igen genom att lämna och åter peka på eller fokusera
sökreglaget; efter ett permanent jobbfel väntar servern fem minuter innan
ett nytt försök köas. Ändrade källfiler och saknade bildark ogiltigförklarar
cachen. Förhandsbilder kräver samma biblioteks- och åldersbehörighet som videon.

Kvalitetsval och ljudsynk beskrivs även i
[Chromecast och Remote Playback](chromecast.md). Genereringens sampling bygger
på FFmpegs [fps-filter](https://ffmpeg.org/ffmpeg-filters.html#fps).

## Start och byte av ström

När spelaren visar **Preparing playback…** väntar servern på en läsbar, icke-tom
direktkälla eller första färdiga HLS-videofragmentet med init. Väntan är
begränsad till serverns konfigurerade tidsfönster (standard 15 sekunder för HLS).
Tidiga kända hårdvarufel kan prova nästa backend. Vid startfel visas felorsaken.

Vid byte av kvalitet, ljudsynk eller spår ligger den tidigare sessionen kvar
under förhandlingen. Misslyckas förberedelsen behålls den. Vid lyckat byte
används den senaste positionen och pausläget, även om du har spelat vidare
medan svaret dröjde. Misslyckade eller avbrutna kandidater städas och skriver
ingen historik innan uppspelning har bekräftats. Stängning av spelaren
avbryter dess väntande startanrop.

Kontrollen bekräftar serverns första data. Den garanterar inte att varje
webbläsare eller mottagare kan avkoda filmen, eller att resten av filen är
felfri. Återstående medie- och mottagarfall följs upp i
[restlistan](../../audits/remaining-work.md).

## Fel och återförsök

Vid ett tillfälligt lokalt HLS-nätfel visas **Connection interrupted. Retrying**.
Spelaren gör högst två egna återförsök för den aktuella strömmen. Hämtningar och
återhämtning har tidsgränser. Pausar du under väntan respekteras det även när
manifestet laddas igen. Bestående åtkomst-, format- och avkodningsfel stoppar
laddningen och visar en felorsak.

**Retry playback** förbereder en ny session vid den sparade positionen, med
samma ljud-/undertextspår, ljudsynk och kvalitetsval. Den misslyckade sessionen
släpps först så att dess transkodningsplats kan återanvändas. Knappen visar
vänteläge medan arbetet pågår. Den förberedda strömmen förblir pausad; tryck
Play för att fortsätta. Ett formatfel kan kräva att du väljer en annan kvalitet.

Retry finns också när första förhandlingen misslyckas eller en direktström inte
kan spelas. Om servern inte kan nås kan städning och ny start misslyckas; felorsaken
visas och du kan försöka igen senare. Återförsöken upphör när spelaren stängs
eller byter källa.

## Paus och sessionens livstid

Så länge webbspelaren är öppen skickar den heartbeat var femte sekund, även
under paus och buffring. Servern förnyar sessionen utan att flytta sökpositionen.
Övergivna HLS-sessioner städas efter 60 sekunder utan heartbeat eller medieanrop;
övergivna direktsessioner försvinner också från spelarlistan. Servern kontrollerar
dessutom diskkvot och pausar en FFmpeg-process som arbetar för långt före klienten.
Vid serverstopp stängs processerna och deras temporära kataloger tas bort.

Operativsystemet eller webbläsaren kan stoppa timers när enheten sover eller en
flik ligger länge i bakgrunden. Har sessionen då hunnit städas visar spelaren
**Resume playback**. Knappen förbereder en ny ström vid den aktuella positionen
med samma spår, kvalitet och ljudsynk. En pausad film förblir pausad. Disktryck
kan också kasta ut en HLS-session; samma återupptagning kan då användas när
utrymme finns. Kontrollen sparar ingen tittarhistorik enbart för en väntande
eller autoplay-blockerad start.

## Ljud- och undertextspår

Välj **Audio track** under uppspelning för att byta språk eller ljudspår.
Servern väljer spåret ur den aktuella mediefilen och omförhandlar vid aktuell
position. Vald kvalitet, ljudsynk och undertext bevaras. Spelaren respekterar
även en paus som görs medan bytet väntar på svar. Vid avvisat byte ligger den
befintliga sessionen och det tidigare valet kvar; serverns felorsak visas.

**Subtitle tracks** visar text- och bildspår. Text som profilen kan visa externt
växlas utan ny videoström. ASS/SSA konverteras till WebVTT för webbprofilerna.
Spår märkta **video conversion** kräver ny serverförhandling för inbränning;
detsamma gäller att stänga av eller ersätta en redan inbränd undertext.
**Off** bevaras efter följande kvalitets- eller ljudspårsbyte. Inbyggda
textundertexter räknas som undertextspår i FFmpegs
[subtitles-filter](https://ffmpeg.org/ffmpeg-filters.html#subtitles-1), och
externa filer får bara läsas inom bibliotekets rötter.

Google Cast använder samma spårval vid omförhandling. Inbränd text skickas inte
som ett obefintligt externt SDK-textspår. En misslyckad hämtning av titelmetadata
blockerar inte en giltig Cast-förhandling. Native Remote Playback använder
befintlig källa; ljudbyte, nytt media och ny inbränning kräver frånkoppling.
Nästa avsnitt är därför spärrat medan sådan mottagare används.

Videor utan ljud kan också omkodas till HLS. Korta klipps verkliga längd bevaras;
servern annonserar inte ett extra sexsekundersintervall för dem. Sökning,
nedladdning, autosynk och generering av nya undertexter återstår i restlistan.
