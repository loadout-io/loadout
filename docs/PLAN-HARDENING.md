# Plan: twardnienie agentów, pętli i pamięci — faza 7

2026-08-24 · analiza + weryfikacja właściciela w trunku + pierwszy żywy bieg · decyzje D-1…D-6 rozstrzygnięte
2026-08-24 (§6) · **decyzja wykonawcza właściciela: KAŻDE zadanie idzie pełną pętlą zadaniową
(`ship-task.sh`, cross-vendor domyślnie, D3); bez fal — kolejność wynika wyłącznie
z zależności, równolegle wolno wszystko, co nie dzieli OWNS** · czytaj po
`docs/PLAN-AGENTS-CONTEXT.md`, przed `tasks/T-98.md`

Źródła: pełna analiza architektury (4 raporty z kodu + 32 prawdziwe biegi), zweryfikowana
przez właściciela w obecnym trunku (9 z 10 najostrzejszych twierdzeń potwierdzone, K2
skorygowane, przelotka podniesiona do P0). Prawdą o zadaniu jest `tasks/<ID>.md`; tu jest
dlaczego, w jakiej kolejności i gdzie są pułapki. Znalezisko spoza mapy → `docs/STATUS.md`,
bez rozszerzania zadań.

---

## 1. Diagnoza w pięciu zdaniach

1. Silnik (DAG, procesy, dowody śmierci, pliki-jako-prawda) jest poprawny; granice trzymają.
2. Pętla sędziego jest przyczyną 0/9 sukcesów dużych biegów ($24–40, 50–96 min każdy):
   nośnik werdyktu to jedna literalna linia prozy (`brak = fail`), sędzia nie widzi prób
   implementera, a domyślny `carry-on` dowozi bieg do końca na padłych danych.
3. Przelotka vendorowa od T-90 dociera do argv, a lista zarezerwowanych nie urosła razem
   z tym — dziura z teoretycznej stała się osiągalna.
4. Pętla learningów jest wpięta end-to-end, ale nie zadziała w praktyce: pamięć wyłącznie
   globalna, odrzucone notatki wracają, refleksja nieaudytowalna i sama wycieka auto-pamięcią,
   koszt Codeksa niewidzialny.
5. Pierwszy prawdziwy bieg po fazie 6 potwierdził historię rund, fan-in, pola `outcome` i pełne
   koło pamięci, ale ujawnił wspólny zapis `~/.claude.json`, utratę `outcome:` przy cięciu 8 KB
   i pusty wynik martwego kroku w indeksie. Faza 7 zamyka je przed wyrocznią T-107.

**Lead na Codeksie ma zmierzoną naprawę, która idzie przez zastępcze T-111:**
`app_server_sandbox` (`codex.rs:1114-1120`) wysyła `readOnly/workspaceWrite/dangerFullAccess`,
a `codex-cli 0.148.0` odrzuca je z `-32600: unknown variant …, expected one of `read-only`,
`workspace-write`, `danger-full-access``. Zmierzone 2026-08-24 na żywym `thread/start`:
z kebab-case wątek otwiera się poprawnie (`ephemeral: true`, `path: null`). Poprawka była
chwilowo w drzewie roboczym właściciela i została COFNIĘTA na rzecz pętli — trunk jest czysty.

Pierwotne T-105 zostało **ZAMKNIĘTE** po drugiej czerwieni kontraktu: wymagane w AC-3
`--ignore-user-config` istnieje dla `codex exec`, ale `codex app-server` 0.149.1 odrzuca je
przed i po subkomendzie. Pozorny zamiennik `-c 'mcp_servers={}'` także jest nieuczciwy —
pusta tabela scala się niedestrukcyjnie i nie usuwa istniejących wpisów. T-110 zostało
**ZAMKNIĘTE** bez lądowania, gdy pełna bramka po naprawie zawisła na fiksturze protokołu
spoza OWNS. T-111 zachowuje AC-1/AC-2 i wyłącza każdy efektywny serwer osobną, wspieraną
nakładką `mcp_servers.<id>.enabled=false` w `thread/start`, po bezpiecznym `config/read`;
obejmuje wszystkie pełne fikstury i zachowuje jawnie zatwierdzone Connections.

T-99 zostało **ZAMKNIĘTE** bez lądowania po drugiej czerwieni. AC-2 wymagało bezwzględnego
wskaźnika w trwałym handoffie, podczas gdy istniejące `memory_handoff_cap.rs` poza OWNS
asertuje dokładnie względną wartość; po wznowieniu bezwzględny zapis wskazywałby ponadto
stary bieg. Recenzent wykrył też niemożliwy ref z `~` w AC-1 i pomylenie celu ze źródłem
strzałki powrotnej w AC-4. Zastępcze T-112 rozdziela przenośny zapis od chwilowego promptu,
używa poprawnego refa `s_2-2` i nazywa sędzią `link.from`.

T-112 także zostało **ZAMKNIĘTE** bez lądowania. Formalna bramka była zielona, ale AC-1
nie obejmowało kolizji `step~2` z literalnym `step-2`, a oba klucze kodowały się do tego
samego refa. Co ważniejsze, cały certyfikat `before` był fałszywy: wspólny target `it` nie
kompilował się przez E0308 w specu AC-3, a harness uznał to za czerwień zachowania. Faza stoi
przed T-100 do czasu osobnej naprawy harnessu i nowego kontraktu zastępczego.

---

## 2. Mapa: każde znalezisko ma adres

| # | Znalezisko (zweryfikowane w trunku) | Gdzie ląduje |
|---|---|---|
| H1 | `RESERVED_CLAUDE` bez `--settings --add-dir --mcp-config --plugin-dir --tools --allowedTools --disallowedTools --append-system-prompt --max-budget-usd --resume --continue --agents --permission-prompt-tool`; `RESERVED_CODEX` bez `sandbox_mode`, `sandbox_workspace_write.*`, `approval_policy`, `mcp_servers.*`, `model_provider(s)`; filtr eskalacji = trzy podciągi | **T-98** |
| H2 | Kolizja przelotki z `--max-budget-usd` (dług T-94) | **T-98** |
| H3 | K1: `copies>1` + `fresh-copy` w repo gitowym nie startuje — gałąź z `tile_key` (`run.rs:3815`), katalog z `work_key_for` | **zastępstwo oczekuje** (T-99/T-112 zamknięte) |
| H4 | K2 (resztka): wskaźnik `Moved to attachments/…` względny wobec katalogu biegu, nierozwiązywalny z `work/<kafelek>`; Codex bez `--add-dir` w ogóle | **zastępstwo oczekuje**, **T-102** (zdanie dla Codeksa), pomiar w **T-107** |
| H5 | K4/K11: pusta udana odpowiedź = przekazanie z trzema pustymi sekcjami, bez sygnału w indeksie | **zastępstwo oczekuje** |
| H6 | Werdykt = jedna literalna linia prozy; zmierzone 21× fail / 3× pass; „PASS" w nagłówku = fail | **T-100** |
| H7 | K3: sędzia nie widzi wcześniejszych prób implementera (`run.rs:7764-7767`) | **T-100** |
| H8 | Sędzia nieostatniej rundy z „fail" zapisany `succeeded` — brak nośnika dla UI | **T-100** (nośnik w run.json), ekran w backlogu §7 |
| H9 | `Route::Blocked` omija `when_it_fails`; okno `succeeded`, książka `failed`, bez ponownego `StepState` | **T-101** |
| H10 | `CONTEXT_NOT_PROVEN` omija `when_this_one_fails` (dług po T-87 AC-5) | **T-101** |
| H11 | Potomkowie kroku zatrzymanego budżetem lądują `cancelled` bez powodu | **T-101** |
| H12 | Budżet ×N przy równoległości | **rozbrojone przez D-5**: budżet jest analityką; miękkość ×N przy jawnie ustawionej kwocie zapisać w docs (§5), bez zmiany wzoru |
| H13 | Codex `cost_usd: None` — wydatki połowy D3 niewidzialne | **T-102** |
| H14 | Refleksja: goły sterownik (wyciek auto-pamięci do `~/.claude/projects/…`, bez evidence, bez sufitu kosztu), zdarzenia porzucane, zero śladu w `run.json`, bez przełącznika, biegnie po anulowanym | **T-103** |
| H15 | Zbiór z `mem/<kafelek>/` bierze pierwszą linię; `because` = boilerplate | **T-103** |
| H16 | L2: odrzucona notatka wraca — `record()` nie zagląda do `discarded/` | **T-104** |
| H17 | `Block::dropped` bez konsumenta; etykieta ekranu kłamie o zasięgu; `from` przeciążone | **T-104** |
| H18 | L1: pamięć wyłącznie globalna — `this-project` przecieka między repo | **T-104** (D-2 = TAK) |
| H19 | Lead na Codeksie: `thread/start` odrzucany (camelCase sandbox) — naprawa zmierzona, patrz §1 | **T-111** (T-105/T-110 zamknięte) |
| H20 | Lead na Codeksie połyka treść błędu JSON-RPC; prywatne MCP z `~/.codex` wchodzą boczną furtką; `--ignore-user-config` nie istnieje dla App Servera, a `mcp_servers={}` jest no-opem | **T-111** (D-4 = TAK; per-serwer `enabled=false`, Connections `enabled=true`) |
| H21 | `prove_agent_dead` bez sufitu; zapadka `live` trzymana na zawsze; `reap_group` bez eskalacji | **T-106** |
| H22 | Martwa maszyneria: tabela SQLite `memory`, `RecoveryPlan.ask`/`RunSpec::resume`, kłamiące nagłówki; (`supersede`/`Kind` i `Absent` ZOSTAJĄ, D-6) | **T-108** |
| H23 | Sędzia z `copies>1`: first-pass-wins vs padła kopia tnie stożek; `nothing_to_judge` patrzy na kopię 0 | **zastępstwo oczekuje** (walidator: zakaz źródła strzałki powrotnej) |
| H24 | Dwa równoległe pytania do człowieka dzielą jeden slot odpowiedzi | backlog §7 |
| H25 | Serve: sukces = spawn; późniejsza śmierć niewidzialna | backlog §7 |
| H26 | Docs: ARCHITECTURE §4/§5/§6b/§8 rozjechane z kodem | **§5** po ostatnim lądowaniu |
| H27 | Pasek `$3.41 of $20` liczony i nigdy niepokazany (`index.tsx` woła `stripFor` bez trzeciego argumentu — dług T-94) | **T-102** |
| H28 | Żywy bieg: limit 8 KB usuwa końcowe `outcome:` z uciętej kopii (20/28 przekazań miało pełny załącznik), więc następny agent nie zna decyzji odczytanej przez silnik | **zastępstwo oczekuje** |
| H29 | Żywy bieg: sześć równoległych procesów Claude'a zapisuje wspólny `~/.claude.json`; jeden padł po 273 ms na uszkodzonym JSON-ie i nadał biegowi `processExit` | **T-109** |

---

## 3. Zadania — wszystkie pełną pętlą (`ship-task.sh`, recenzent cross-vendor)

| ID | Tytuł | Zależy od | Dotyka `commands/run.rs` | Kryteriów (szac.) |
|---|---|---|---|---|
| T-98 | Przelotka nie sięga ponad dial: pełne listy i filtr po kluczach | — | nie | 4 |
| T-99 | **ZAMKNIĘTE:** sprzeczny wskaźnik i dwa błędy tekstu kryteriów | — | tak | 4 |
| T-100 | Werdykt jest polem, sędzia widzi próby | zastępstwo T-112 | tak | 4 |
| T-101 | Każda porażka przechodzi przez jedno miejsce — naprawdę | T-100 | tak | 4 |
| T-102 | Wydatki są analityką: koszt obu vendorów policzony i pokazany | T-101, T-111 (wspólny `codex.rs`) | tak | 4 |
| T-103 | Refleksja audytowalna, oszczędna i wyłączalna | T-102 | tak | 5 |
| T-104 | Pamięć: per projekt, odrzucone nie wraca, pominięte widać | T-103 | tak (skan) | 5 |
| T-105 | **ZAMKNIĘTE:** AC-3 wymaga nieistniejącej flagi App Servera | — | nie | 3 |
| T-106 | Zatrzymanie ma sufit i eskalację | T-102 | tak | 3 |
| T-107 | Prawdziwy bieg jest wyrocznią fazy | wszystkie | nie (`e2e/`, `tests/` `--ignored`) | 3 |
| T-108 | Sprzątanie po D-6: martwa tabela i martwa gałąź odzyskiwania znikają | T-104 | nie | 2 |
| T-109 | Prywatny stan procesu Claude'a bez utraty równoległości | T-103 | nie | 3 |
| T-110 | **ZAMKNIĘTE:** pełna bramka wymagała fikstury App Servera spoza OWNS | — | nie | 3 |
| T-111 | Lead Codeksa: poprawny sandbox, jawna odmowa, prywatne MCP wyłączone, Connections zachowane | — (zastępuje T-105/T-110) | nie | 3 |
| T-112 | **ZAMKNIĘTE:** fałszywe `before` i kolizyjne kodowanie refów | — | tak | 5 |

### Zakres per zadanie (kontrakty pisać z tego, nie rozszerzać)

**T-98 — przelotka.** (P0; mandat D-1 na zmianę przesłanki `agents_vendor_args_filtered.rs`,
który dziś używa `--settings` jako przykładu flagi NIEzarezerwowanej — dać testowi inny przykład)
- `RESERVED_CLAUDE` += lista z H1 (w tym `--model`); `RESERVED_CODEX` += `sandbox_mode`,
  `sandbox_workspace_write.network_access`, `approval_policy`, prefiksy `mcp_servers.`,
  `model_provider`, `model_providers.`.
- Filtr eskalacji z trzech podciągów na reguły po kluczu (dla `-c` Codeksa dopasowanie po
  prefiksie klucza przed `=`); wartości dalej skanowane o trzy literały.
- Kolizja z `--max-budget-usd` w `FORBIDDEN_ESCALATIONS` (H2).
- OWNS: `library/agents.rs`, `workflow/check.rs`, testy; bez `run.rs`.

**T-99 — ZAMKNIĘTE, bez lądowania.** Po jedynej naprawie 4/4 AC było zielone, lecz pełna
bramka wymagała zmiany `memory_handoff_cap.rs` spoza OWNS. Stara wyrocznia przypina względny
wskaźnik w trwałym pliku, a AC-2 wymagało w tej samej linii ścieżki bezwzględnej. AC-1 żądało
refa z niedozwolonym przez Git `~`; AC-4 nazywało sędzią cel zamiast źródła powrotu. Gałąź
jest dowodem, nie źródłem commitów.

**T-112 — ZAMKNIĘTE, bez lądowania.** Formalna bramka 21/21 nie jest dowodem: wszystkie
`before` padły na E0308 we wspólnym targetcie, a AC-1 pomijało zaakceptowany plik, w którym
`s_2~2` i literalne `s_2-2` wybierają ten sam ref. Poniższy zakres pozostaje mapą dla nowego,
globalnie unikalnego kontraktu dopiero po naprawie harnessu:
- AC-1: katalog drugiej kopii zachowuje `s_2~2`, ale dokładny poprawny ref to
  `loadout/<bieg>/s_2-2`; prawdziwe repo, równoległość i wznowienie własnej kopii.
- AC-2: trwały wskaźnik zostaje względny i przenośny; zmontowany prompt konkretnego odbiorcy
  podaje bezwzględny, otwieralny adres pełnej kopii w katalogu bieżącego biegu, także po
  wznowieniu i usunięciu starego katalogu.
- AC-3: ostatnie `outcome:` przeżywa limit dokładnie raz, bez syntezy decyzji.
- AC-4: puste ciało → stały dopisek w prawdziwym indeksie następnego kroku.
- AC-5: walidator odmawia `copies>1` na źródle strzałki powrotnej (`link.from`); cel powrotu
  i zwykłe kroki mogą mieć kopie.

**T-100 — werdykt i pamięć sędziego.**
- Werdykt drogą pól (`FIELDS_ASKED_FOR`, T-90 AC-4): sędzia dostaje automatycznie wymagane
  pole `outcome` (`pass`/`fail`); linia `outcome:` w prozie zostaje fallbackiem; brak obu
  w rundzie nieostatniej = fail jak dziś, w ostatniej = `when_this_one_fails`.
- Sędzia rundy k dostaje w indeksie wszystkie wcześniejsze próby implementera (istniejące
  etykiety „try N of M").
- `run.json` na kroku sędziego zapisuje werdykt rundy (pole addytywne) — nośnik dla
  przyszłego ekranu (backlog).

**T-101 — jedno miejsce porażki.**
- `CONTEXT_NOT_PROVEN` przez `when_this_one_fails` (`run.rs:6815-6825`).
- `Route::Blocked` wraca do `Live` i idzie przez `when_this_one_fails`; książka i okno
  dostają ten sam stan (ponowny `StepState` po korekcie).
- Stożek zatrzymany budżetem = `skipped` ze zdaniem, nie `cancelled` bez powodu
  (`run.rs:6438-6454`; kod ma dogonić własny komentarz z `run.rs:8139-8144`).
- Testy z krokiem PONIŻEJ zatrzymanego (dzisiejsza fikstura go nie ma).

**T-102 — wydatki jako analityka (kształt z D-5: łagodnie).**
- Codex: `cost_usd` szacowany z tokenów po tabeli cen w jednej stałej (wejście/wyjście/cache,
  po prefiksie modelu; nieznany model → tokeny bez dolarów + zdanie w `run.json`). Szacunek
  oznaczony jako szacunek (osobne pole, nie udawany pomiar).
- Pasek wydatków POKAZANY: `index.tsx` woła `stripFor` z trzecim argumentem (H27); bez
  ustawionego budżetu pasek pokazuje samą sumę wydatków biegu.
- Zachowanie budżetu bez zmian: twardy stop tylko gdy człowiek jawnie ustawił kwotę przy
  Starcie; wzoru ×N nie ruszamy, miękkość zapisana w docs (§5).
- Kroki Codeksa dostają w indeksie przekazań zdanie, że pliki leżą poza cwd i czyta się je
  po ścieżce bezwzględnej (H4; pomiar skuteczności w T-107).

**T-103 — refleksja (D-3: domyślnie włączona, wyłączalna, nie po anulowanym).**
- Sterownik refleksji przez `with_settings` (auto-pamięć do `<bieg>/mem/_reflection/` + deny
  gospodarza) i `with_evidence` (`logs/reflection.jsonl` + `input.json`).
- Wynik (ile par, ile odrzuconych bez powodu, koszt) w `run.json` (pole addytywne).
- Sufit kosztu ze stałej na turę; nie biegnie po biegu anulowanym; przełącznik przy Starcie
  (domyślnie włączona).
- Zbiór z `mem/<kafelek>/`: `rule` = pierwszy akapit, ciało pliku → ciało notatki,
  `because` z wiersza `**Why:**` gdy jest (format, który auto-pamięć realnie produkuje).

**T-104 — pamięć (D-2 = TAK: drugi korzeń).**
- `<repo>/.loadout/memory/` dla `this-project`: `notes_root` dostaje wariant projektowy,
  `what_the_agents_know` skanuje dwa korzenie, refleksja pisze kandydatki `this-project`
  pod korzeń projektu, ekran Pamięć pokazuje obie grupy z nazwą źródła.
- `record()` sprawdza `discarded/` po slugu — trafienie = kandydatka nie powstaje, licznik
  w `run.json` obok pól refleksji.
- `Block::dropped` dojeżdża do UI (lustro drutu: nowy klucz w `NoteWire` = wiersz
  w `commands-wired`/golden; czerwień widać w full-test).
- `from` rozdzielone na `from` (id biegu) i `project` (nazwa) — wiersz pokazuje nazwę.
- Etykieta ekranu mówi prawdę o zasięgu.

**T-105 — ZAMKNIĘTE, bez lądowania.** AC-1 i AC-2 dostały uczciwe czerwone specy, lecz
AC-3 wymagało flagi odrzucanej przez prawdziwy App Server. Dodanie asercji na nieobsługiwane
argv byłoby zazielenieniem fikstury kosztem zepsucia produktu. Kontraktu nie łatamy; zastępuje
go nowy, globalnie unikalny kontrakt.

**T-110 — ZAMKNIĘTE, bez lądowania.** Mechanizm konfiguracji per wątek był właściwy i po
jednej rundzie naprawy wszystkie trzy AC były zielone. Pełna suita zawisła jednak na
`lead_evidence_is_durable.rs`, którego App Server nie odpowiadał na nowe `config/read`; plik
leżał poza OWNS T-110. Reguła fazy zabrania rozszerzenia własności po biegu. Gałąź zostaje
dowodem, nie źródłem commitów; zastępuje ją T-111 z nowymi ścieżkami speców.

**T-111 — lead na Codeksie (D-4 = TAK; zastępstwo T-105/T-110).**
- AC-1: `app_server_sandbox` wysyła `read-only` / `workspace-write` / `danger-full-access`
  i wiąże drogę App Servera z `exec` jedną tabelą.
- AC-2: `app_server_actor` dokleja dynamiczne `error.code` + `error.message` vendora do
  odmowy, którą dostaje okno dla wiadomości tekstowej; kontrola sukcesu pozostaje zielona.
  Bezpieczna, stała odmowa dla szkicu z obrazem zostaje zgodnie z T-34.
- AC-3: po `initialize`, przed `thread/start`, `config/read` daje identyfikatory efektywnych
  `mcp_servers`; prywatne dostają bezpiecznie zakodowane `enabled=false`, a nazwy z
  `DriverConfiguration.servers` jawne `enabled=true`. Środowisko zatwierdzonego Connection
  dociera do procesu App Servera przez supervisor. Błąd lub zły kształt konfiguracji odmawia
  przed `thread/start` zamiast wracać do prywatnych narzędzi.
- Wyrocznia jest przywiązana do źródeł OpenAI: request overrides z
  `ThreadStartParams.config` są konwertowane JSON→TOML i dokładane po CLI; referencja definiuje
  `mcp_servers.<id>.enabled`. Wszystkie stare pełne fikstury odpowiadają na nowy krok protokołu.
- OWNS: `engine/drivers/codex.rs`, współdzielony encoder klucza, runtime Connections, obie
  pełne fikstury App Servera i unikalne testy; bez `run.rs`. Musi wylądować przed T-102.
- Pułapka słownictwa: treść błędu vendora wchodzi do zdania w RUNTIME (interpolacja), nie
  jako literał w kodzie — literały zdania trzymać w dzisiejszej rodzinie.

**T-106 — zatrzymanie.**
- `prove_agent_dead`: sufit prób (stała); po nim krok `failed` ze zdaniem o żywym procesie
  i pid/pgid w `run.json`; zapadka `live` puszczana.
- `reap_group` przy starcie: łaska → SIGKILL → sonda (jak `Supervised::stop`); wynik w wierszu
  biegu (`interrupted` z powodem), nie tylko w logu.

**T-107 — wyrocznia fazy.**
- Zadanie pisze wyrocznię: rozszerzenie wyroczni flow (`--ignored`, konwencja
  `flow-oracles`) o graf plan → pętla (implementer + sędzia, `max_turns: 2`) → synteza;
  raz Claude-pisarz/Codex-sędzia, raz odwrotnie.
- Sądzi: rundy widzą przeszłość (indeks w `input.json`), werdykt-pole kończy pętlę przed
  wyczerpaniem rund, refleksja zostawia ≤3 kandydatki i wpis w `run.json`, `mem/` nie wycieka
  poza katalog biegu, koszt Codeksa policzony i pokazany, krok Codeksa czyta przekazanie po
  ścieżce bezwzględnej (pomiar H4).
- Bramka zadania sądzi, że wyrocznia istnieje i kompiluje się; PRZEBIEG wyroczni to osobne,
  płatne uruchomienie `--ignored` po lądowaniu — wynik wpisać do `docs/STATUS.md`.

**T-108 — sprzątanie po D-6.**
- Usunąć: tabelę SQLite `memory` (+ migracja), `RecoveryPlan.ask` z martwą gałęzią
  „pick up here" w `recovery.rs`; poprawić kłamiące nagłówki (`notes.rs:23`).
- Zostawić z poprawionym nagłówkiem: `supersede()`/`Kind` (wraca przy `/correct`),
  sterownik `Absent` (trzeci vendor).

**T-109 — prywatny stan Claude'a.**
- `RunSettings::for_step` zakłada `<bieg>/claude/<work-key>` i niesie tę ścieżkę do
  `ClaudeDriver::command`; komenda ustawia `CLAUDE_CONFIG_DIR` per krok.
- Nie kopiuje stanu ani poświadczeń gospodarza. Na macOS poświadczenia pozostają w Keychain;
  `HOME` zostaje przepuszczony, lecz `~/.claude.json` nie jest już celem zapisu procesu kroku.
- Dwa procesy-atrapy muszą realnie nałożyć się w czasie i zapisać dwa odrębne znaczniki;
  nieużywalny katalog odmawia przed spawnem zamiast wracać do wspólnego `HOME`.
- T-107 mierzy na prawdziwym CLI, że odcisk i istnienie `~/.claude.json` nie zmieniły się.

---

## 4. Kolejność — z zależności, nie z fal

- **T-111 wylądowało; T-99 i T-112 zamknięte; STOP przed T-100.** Nie wznawiać starych
  gałęzi ani nie przenosić ich testów lub commitów. Najpierw osobna naprawa fałszywego
  certyfikatu `before`, potem nowy kontrakt z widoczną odmową kolizji zakodowanych refów.
- **Łańcuch `run.rs`** (dzielony OWNS, więc szeregowo):
  `nowe zastępstwo → T-100 → T-101 → T-102 → T-103 → T-104 → T-106`.
- **T-109 po T-103**, bo refleksja ma korzystać z gotowego szwu ustawień; potem może wejść
  przed T-104. Nie wolno go przesunąć za T-107, bo żywa wyrocznia sądzi właśnie ten zapis.
- **Równolegle** (zmierzone porównaniem bloków OWNS 2026-08-24, nie założone): pierwotną parą
  bez ani jednego wspólnego pliku było **T-98 ∥ T-105**. T-98 wylądowało, T-105 i pierwsze
  zastępstwo T-110, T-99 oraz T-112 zostały zamknięte; T-111 wylądowało, a nowe zastępstwo
  oczekuje na zgodę i naprawę harnessu. Wszystko dalej
  dzieli `run.rs`,
  `check.rs`, `codex.rs`, `drivers/mod.rs` albo `recovery.rs` i idzie szeregowo:
  nowe zastępstwo po T-98 (`workflow/check.rs`), T-102 po T-111 (`codex.rs`),
  **T-108 po T-106** (`recovery.rs`), T-107 na końcu (sądzi zachowanie z T-100 i T-103);
  **T-108** po T-104; **T-107** po wszystkim.
- Przy zajętym trunku wolno stackować: `FROM=` dla bazy, `LOADOUT_TRUNK=` dla zakresu
  (sprawdzone w fazie 5). `LOADOUT_CARGO_LOCK_WAIT=2400`, gdy równolegle biegnie więcej niż
  jedno zadanie rustowe.
- Lądowanie zawsze pojedynczo (`integrate.sh`, pełna bramka po każdej gałęzi). Po ręcznym
  konflikcie: `cargo check --all-targets --keep-going`, potem `./verify.sh full` — nigdy sam
  commit.
- Przed startem fazy warto przebiec obie wyrocznie flow (`--ignored`, płatne) jako baseline.

---

## 5. Dokumentacja po fazie (bez zadania, wyjątek właściciela)

- ARCHITECTURE §6b: sześć angielskich etykiet indeksu zamiast trzech polskich cytatów.
- §8: `attachments/` trzyma CAŁĄ znormalizowaną kopię (nie „ogon"); dopisać, że silnik pisze
  wyłącznie `findings`; dopisać drugi korzeń pamięci (po T-104) i prywatny
  `claude/<work-key>` (po T-109).
- §4: argv uzupełnić o `--add-dir` (przekazania + załączniki), `--tools`,
  `--append-system-prompt`, `--model`.
- §5: wiersz o suficie `prove_agent_dead` (po T-106).
- Budżet: zapisać wprost, że przy jawnie ustawionej kwocie sufit jest miękki do ×N przy
  równoległości (D-5 — świadoma decyzja, nie przeoczenie).

---

## 6. Decyzje człowieka — ROZSTRZYGNIĘTE 2026-08-24

| # | Decyzja | Rozstrzygnięcie |
|---|---|---|
| D-1 | `--settings` (i reszta list) na flagi zarezerwowane; zmiana przesłanki `agents_vendor_args_filtered.rs` | **TAK** — test dostaje inny przykład flagi wolnej |
| D-2 | Pamięć per projekt (`<repo>/.loadout/memory/`) | **TAK** |
| D-3 | Refleksja domyślnie włączona, wyłączalna; nie biegnie po anulowanym | **TAK** |
| D-4 | Lead Codeksa bez prywatnych MCP | **TAK** — T-105 zamknięte, bo App Server nie przyjmuje `--ignore-user-config`; T-110 zamknięte na fiksturze poza OWNS; T-111 wyłącza prywatne `mcp_servers.<id>` i jawnie włącza zatwierdzone Connections w konfiguracji `thread/start` |
| D-5 | Budżet | **ŁAGODNIE** — wydatki to analityka (właściciel na subskrypcjach obu vendorów): koszty policzone i pokazane u obu vendorów, twardy stop TYLKO przy jawnie ustawionej kwocie, wzór ×N bez zmian, miękkość w docs |
| D-6 | Martwa maszyneria | **wg rekomendacji** — usunąć tabelę `memory` i `RecoveryPlan.ask`; zostawić `supersede()`/`Kind` i `Absent` z poprawionymi nagłówkami |
| D-7 | Tryb wykonania fazy | **wszystko pełną pętlą zadaniową, bez fal** — wcześniejszy podział na tryby (szybki/wprost) cofnięty decyzją właściciela 2026-08-24 |

---

## 7. Poza fazą (backlog, żeby nie zginęło)

- H24: równoległe pytania do człowieka (jeden slot odpowiedzi) — kanał per krok.
- H25: żywotność kroku `serve`.
- Ekran czerwonej rundy sędziego (nośnik powstaje w T-100).
- Selekcja trafności notatek (FTS5, T6 §7) — wróci przy realnym wzroście liczby notatek.
- Klucz Linear jawnym tekstem → Keychain (osobny research).
- Prompty importowanych agentów (urc) wskazują artefakty, których Loadout nie tworzy —
  praca w bibliotece użytkownika, nie w silniku.

## 8. Pułapki, które ta faza zna z góry

1. **`RunSpec`/`AgentJob` bez `Default`** — nowe pola wyłącznie szwem addytywnym
   (`with_*` / `Option` + `#[serde(default)]`), inaczej `quick-scope` pali 30+ plików.
2. **Słownictwo**: `quick-vocabulary` skanuje tekst widoczny i komunikaty asercji — bez
   `handoff/verdict/judge/loop/session/gate/node/DAG`; nowe etykiety (zakres zamkniętego
   T-112 AC-4) z istniejącej
   rodziny („what it passed on", „try N of M").
3. **Lustro drutu porównuje ZBIÓR kluczy** — nowe pole w `NoteWire`/`Line`/`run.json` widoczne
   z frontu ciągnie wiersz w `commands-wired.test.ts`/golden; czerwień często dopiero w full.
4. **`quick-clippy` biegnie `--lib`** — linty w `tests/` widać dopiero w full; przed `full`
   raz na zadanie `cargo clippy --all-targets --keep-going`.
5. **Backticki w backtickach w `///` palą clippy** (zmierzone przy sondzie leada) — cytaty
   błędów vendora w zwykłych `//`.
6. **Fikstury równoległości**: dwa kroki bez strzałki w folderze projektu = Problem
   z niezmiennika 12; następca testu K1 (zamknięte T-112) na `fresh-copy`.
7. **`before` czerwone NA ASERCJI**: Rust — `todo!()` + linia `mod` w `tests/it/main.rs`;
   TS — szkielet importowalny padający na `expect`.
8. **Wyrocznie `--ignored` kosztują prawdziwe pieniądze** — bramka sądzi ich istnienie
   i kompilację; przebieg to decyzja właściciela po lądowaniu (T-107, baseline w §4).
