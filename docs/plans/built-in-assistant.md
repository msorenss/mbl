# Inbyggd AI-assistent och medtittare

Status: plan (2026-11-19). Beslut från användaren: alternativ 1, MBL driver själv agenten. Den ska fungera med en API-nyckel hos de flesta leverantörer, och även kunna använda ett agentramverk (Hermes, Claude Code). Externa MCP-agenter förblir ett tillval.

## Mål

1. **Medtittarchatten svarar utan att någon extern agent kör.** När tittaren skriver, trycker på "Kommentera nu" eller när en schemalagd kommentar är dags anropar MBL själv en modell.
2. **Global chatt överallt i appen**, även utan uppspelning. Exempel: "Finns det någon serie jag borde se utifrån min historik?" och "Jag gillar skådespelare X, finns det mer med hen i biblioteket?".
3. **Fungerar med de flesta leverantörer**, också lokala modeller, så att inget behöver lämna hemnätet om användaren inte vill det.

## Vad som redan finns

- `CompanionService::wait_for_events` ger händelser (tittarmeddelande, `requested`, `scheduled`) med `live_context`. Den inbyggda agenten använder samma källa som externa agenter.
- `mbl_mcp::tools::all_tools()` och `tools::execute()` kör alla 80 MCP-verktyg i processen med en `Caller` (`Actor` + scopes). ACL, åldersgränser och spoilerfilter gäller redan där.
- `reqwest` finns i arbetsytan. Hemligheter (Radarr/Sonarr m.fl.) ligger i `settings` i klartext, enligt beslutet 2026-11-16.
- Det saknas en verktygsfunktion för "titlar med person X". `item_people`/`people` finns i databasen, men det finns ingen sökväg från person till titlar.

## Arkitektur

Ny crate `mbl-agent` (beror på `mbl-service`, `mbl-mcp`, `reqwest`). Den kopplas i `mbl-server/src/app.rs`.

```
           +-------------------+        +-----------------------+
 webb ---> | REST /assistant/* | -----> | AgentRuntime          |
           +-------------------+        |  - konversationsloop  |
 companion wake (wait_for_events) ----> |  - verktygsanrop      | --> mbl_mcp::tools::execute
                                        |  - budget/gränser     |     (tittarens Actor)
                                        +-----------+-----------+
                                                    |
                                     +--------------+---------------+
                                     | Provider (trait)             |
                                     |  OpenAiCompatible            |
                                     |  Anthropic                   |
                                     |  CliAgent (Claude Code etc.) |
                                     +------------------------------+
```

### Leverantörer

Två wire-protokoll täcker nästan alla leverantörer:

| Adapter | Täcker |
|---|---|
| **OpenAI-kompatibel** (`/v1/chat/completions` med `tools`) | OpenAI, Google Gemini (OpenAI-kompatibel slutpunkt), OpenRouter, Groq, Mistral, DeepSeek, Together, xAI, Azure OpenAI, samt lokala: Ollama, LM Studio, vLLM, llama.cpp-server och `hailo-ollama` på Pi:n |
| **Anthropic Messages** (`/v1/messages`) | Claude |
| **CLI-agent** (fas A5) | Claude Code (`claude -p` med genererad MCP-konfig), Hermes eller annat ramverk som kan ta en prompt och en MCP-server |

- Inställningar: `assistant_enabled`, `assistant_provider` (`openai_compatible` / `anthropic` / `cli`), `assistant_base_url`, `assistant_api_key`, `assistant_model`, `assistant_cli_command`, samt gränser (se nedan).
- Adminvyn har färdiga val (OpenAI, Anthropic, Gemini, OpenRouter, Groq, Mistral, Ollama lokalt, LM Studio, Hailo-ollama). Ett val fyller i bas-URL och modellförslag, och knappen "Testa anslutning" gör ett kort anrop och visar svar och latens.
- Modeller utan verktygsstöd: agenten får då färdig kontext (scen, historik, sökträffar) i prompten i stället för verktyg. Svaren blir enklare men fungerar fortfarande. Adaptern känner av verktygsstöd vid anslutningstestet.
- Strömmande svar (SSE) i båda adaptrarna, så att den globala chatten visar text medan den skrivs.

### Verktyg och behörighet

- Agenten kör alltid som **tittaren som frågar** (dess `Actor`), med scopes `read` + `playback`. Aldrig `admin` eller `library` från början. ACL, åldersgräns och bibliotekstilldelning följer därmed automatiskt.
- En tillåtelselista med verktyg skickas till modellen. Medtittaren får: `explain_scene`, `get_item`, `get_children`, `search_media`, `get_watch_history`, `get_continue_watching`, `get_spoiler_free_recap`, `search_scenes` samt det nya `find_by_person`. Den globala chatten får samma utom scenverktygen, plus `get_hubs` och `browse_items`.
- Nytt MCP-verktyg och REST-anrop `find_by_person` (namn → titlar i biblioteket med roll och karaktär, behörighetsfiltrerat). Det nyttjas även av externa agenter.
- Inga muterande verktyg i första versionen. "Spela upp", "lägg i kö" och liknande är förslag i svaret med en knapp i webben (fas A4), inte något agenten gör själv.
- Metadata och tittarens text är data, inte instruktioner. Systemprompten säger det, och verktygssvar märks som otrodda.

### Spoilerskydd

- Medtittaren får bara dialog fram till positionen (`live_context` garanterar det redan) och bara `get_spoiler_free_recap`, inte råa sammanfattningar av senare avsnitt.
- Systemprompten förbjuder att avslöja handling efter aktuell position. Ett test skickar ett frestande verktygssvar och kontrollerar att prompten innehåller spärren. Det går inte att bevisa att modellen lyder; det dokumenteras.

### Gränser och integritet

- Per användare: max antal anrop per timme och max verktygssteg per svar (standard 6). Totalt dagligt tak i tokens, som admin kan sätta. Svaren loggas i det befintliga MCP-auditflödet med `client = "mbl-assistant"`.
- Hanterade profiler (barn) har assistenten av som standard. Admin kan slå på den per användare.
- Adminvyn säger tydligt att titlar, historik och dialog skickas till vald leverantör, och föreslår lokal modell för den som inte vill det.
- Nyckeln visas maskerad och följer med i säkerhetskopior, precis som Arr-nycklarna.

## Faser

| Fas | Innehåll | Klart när |
|---|---|---|
| **A0** Leverantörslager | `mbl-agent` med `Provider`-trait, OpenAI-kompatibel och Anthropic-adapter, strömning, inställningar, adminvy med förval och anslutningstest | Testat mot falsk server (båda protokollen, verktygsanrop, fel, timeout) och mot minst en riktig leverantör eller lokal Ollama |
| **A1** Agentloop | Verktygsloop med tillåtelselista, tittarens `Actor`, stegtak, budget, audit, `find_by_person` | Integrationstest: fråga → verktygsanrop → svar, behörighet (annan användares bibliotek syns inte), stegtak |
| **A2** Inbyggd medtittare | Bakgrundsuppgift per aktiv session där chatten är på: går med som "MBL" och svarar på tittarmeddelanden, `requested` och `scheduled` via `wait_for_events`. Av när assistenten är avslagen; samexisterar med externa agenter | Riktig session: meddelande → svar inom sekunder, schemalagd kommentar enligt frekvens, läget `off` respekteras |
| **A3** Global chatt | Tabell `assistant_conversations`/`assistant_messages` (migration), `POST /api/v1/assistant/conversations/{id}/messages` med SSE, chattlåda som nås från toppfältet och kommandopaletten (`Ctrl+K`). **Laddas latent**, eftersom JS-budgeten bara har cirka 1,3 KB kvar | "Vad borde jag se?" och "mer med X" ger korrekta, behörighetsfiltrerade titlar med länkar |
| **A4** Handlingar | Titlar i svar blir kort med "Spela" och "Visa". Knappen "Fråga om scenen" i spelaren | Webbtester |
| **A5** Ramverk | `cli`-leverantör: kör Claude Code/Hermes med genererad MCP-konfig och en kortlivad token med `read`+`playback` för tittaren. Dokumentation för alltid-på externa agenter | Prov med `claude -p` på Pi:n |

Rekommenderad ordning: A0 → A1 → A2 (det användaren testar nu) → A3 → A4 → A5.

## Öppna frågor till användaren

1. Vilken leverantör ska vi prova först? Det kräver en nyckel, eller Ollama lokalt. Pi:ns `hailo-ollama` klarar små modeller (Qwen 1,5–3B) men troligen inte pålitliga verktygsanrop.
2. Ska medtittaren få pausa uppspelningen (`sync_playback`) när den förklarar något? Förslaget är nej i första versionen.
3. Release: hör detta till 1.3-serien eller till nästa version? Enligt releaseplanen är 1.3.10 bara buggfixar, så förslaget är att det kommer efter 1.3, parallellt med eller före Android.

## Risker

- **JS-budgeten** (148,7/150 KB): chattlådan och adminvyn måste laddas latent.
- **Små lokala modeller** gör dåliga verktygsanrop. Därför finns reservläget utan verktyg, och anslutningstestet visar om verktyg fungerar.
- **Kostnad** med molnleverantörer: tak per användare och dag, och frekvensen "hög" ger ungefär 40 anrop per timme och session.
- **Pi:ns minne** vid lokala modeller samtidigt som omkodning pågår: anropen ska ge vika för uppspelning, som skanningsjobben redan gör.
