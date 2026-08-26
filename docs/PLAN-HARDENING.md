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
przed T-100 do czasu osobnej naprawy harnessu i nowego kontraktu zastępczego. Właściciel
autoryzował oba kroki: commit `5604c3d` odrzuca każdą diagnostykę kompilatora Rusta jako
fałszywe `before`, a T-113 zastąpiło T-99/T-112 i dodało widoczną odmowę kolizji
zakodowanych refów przed pierwszym spawnem.

T-113 zostało **ZAMKNIĘTE** bez lądowania po drugiej czerwieni 20/22. Pięć AC było zielonych,
lecz nowy spec adresu użył jednej dokładnej etykiety dla pierwszej dostawy i wznowienia.
Istniejąca semantyka prawidłowo nazywa wznowiony plik `what an earlier run left here`, a spec
wymagał `what the step before left`. Zmiana produkcji uzależniona od długości przekazania
byłaby oszustwem. Właściciel zatwierdził **T-114**: pełne zastępstwo z sześcioma nowymi
ścieżkami, które osobno asertuje prawdziwe pochodzenie w obu biegach.

---

## 2. Mapa: każde znalezisko ma adres

| # | Znalezisko (zweryfikowane w trunku) | Gdzie ląduje |
|---|---|---|
| H1 | `RESERVED_CLAUDE` bez `--settings --add-dir --mcp-config --plugin-dir --tools --allowedTools --disallowedTools --append-system-prompt --max-budget-usd --resume --continue --agents --permission-prompt-tool`; `RESERVED_CODEX` bez `sandbox_mode`, `sandbox_workspace_write.*`, `approval_policy`, `mcp_servers.*`, `model_provider(s)`; filtr eskalacji = trzy podciągi | **T-98** |
| H2 | Kolizja przelotki z `--max-budget-usd` (dług T-94) | **T-98** |
| H3 | K1: `copies>1` + `fresh-copy` w repo gitowym nie startuje — gałąź z `tile_key` (`run.rs:3815`), katalog z `work_key_for` | **T-114** (T-99/T-112/T-113 zamknięte) |
| H4 | K2 (resztka): wskaźnik `Moved to attachments/…` względny wobec katalogu biegu, nierozwiązywalny z `work/<kafelek>`; Codex bez `--add-dir` w ogóle | **T-114**, **T-115** (T-102 zamknięte; zdanie dla Codeksa), pomiar w **T-107** |
| H5 | K4/K11: pusta udana odpowiedź = przekazanie z trzema pustymi sekcjami, bez sygnału w indeksie | **T-114** |
| H6 | Werdykt = jedna literalna linia prozy; zmierzone 21× fail / 3× pass; „PASS" w nagłówku = fail | **T-100** |
| H7 | K3: sędzia nie widzi wcześniejszych prób implementera (`run.rs:7764-7767`) | **T-100** |
| H8 | Sędzia nieostatniej rundy z „fail" zapisany `succeeded` — brak nośnika dla UI | **T-100** (nośnik w run.json), ekran w backlogu §7 |
| H9 | `Route::Blocked` omija `when_it_fails`; okno `succeeded`, książka `failed`, bez ponownego `StepState` | **T-101** |
| H10 | `CONTEXT_NOT_PROVEN` omija `when_this_one_fails` (dług po T-87 AC-5) | **T-101** |
| H11 | Potomkowie kroku zatrzymanego budżetem lądują `cancelled` bez powodu | **T-101** |
| H12 | Budżet ×N przy równoległości | **rozbrojone przez D-5**: budżet jest analityką; miękkość ×N przy jawnie ustawionej kwocie zapisać w docs (§5), bez zmiany wzoru |
| H13 | Codex `cost_usd: None` — wydatki połowy D3 niewidzialne | **T-115** (T-102 zamknięte) |
| H14 | Refleksja: goły sterownik (wyciek auto-pamięci do `~/.claude/projects/…`, bez evidence, bez sufitu kosztu), zdarzenia porzucane, zero śladu w `run.json`, bez przełącznika, biegnie po anulowanym | **T-121 → T-126 wylądowały** (T-103/T-116/T-117/T-118/T-119/T-120/T-123/T-125 zamknięte; dokładny budżet realnego spawnu mierzy końcowy oracle) |
| H15 | Zbiór z `mem/<kafelek>/` bierze pierwszą linię; `because` = boilerplate | **T-124 wylądowało** (T-103/T-116/T-117/T-118/T-119/T-120/T-122 zamknięte) |
| H16 | L2: odrzucona notatka wraca — `record()` nie zagląda do `discarded/` | **T-139 wylądowało** (T-104/T-128/T-136/T-137/T-138 zamknięte) |
| H17 | `Block::dropped` bez konsumenta; etykieta ekranu kłamie o zasięgu; `from` przeciążone | **T-129 zakontraktowane:** bieżący limit/zasięg/pochodzenie → **T-130 zakontraktowane:** zamrożeni odbiorcy po lądowaniu T-129 |
| H18 | L1: pamięć wyłącznie globalna — `this-project` przecieka między repo | **T-139 wylądowało** (D-2 = TAK) |
| H19 | Lead na Codeksie: `thread/start` odrzucany (camelCase sandbox) — naprawa zmierzona, patrz §1 | **T-111** (T-105/T-110 zamknięte) |
| H20 | Lead na Codeksie połyka treść błędu JSON-RPC; prywatne MCP z `~/.codex` wchodzą boczną furtką; `--ignore-user-config` nie istnieje dla App Servera, a `mcp_servers={}` jest no-opem | **T-111** (D-4 = TAK; per-serwer `enabled=false`, Connections `enabled=true`) |
| H21 | `prove_agent_dead` bez sufitu; zapadka `live` trzymana na zawsze; `reap_group` bez eskalacji | **T-106** |
| H22 | Martwa maszyneria: tabela SQLite `memory`, `RecoveryPlan.ask`/`RunSpec::resume`, kłamiące nagłówki; (`supersede`/`Kind` i `Absent` ZOSTAJĄ, D-6) | **T-108** |
| H23 | Sędzia z `copies>1`: first-pass-wins vs padła kopia tnie stożek; `nothing_to_judge` patrzy na kopię 0 | **T-114** (walidator: zakaz źródła strzałki powrotnej) |
| H24 | Dwa równoległe pytania do człowieka dzielą jeden slot odpowiedzi | backlog §7 |
| H25 | Serve: sukces = spawn; późniejsza śmierć niewidzialna | backlog §7 |
| H26 | Docs: ARCHITECTURE §4/§5/§6b/§8 rozjechane z kodem | **§5** po ostatnim lądowaniu |
| H27 | Pasek `$3.41 of $20` liczony i nigdy niepokazany (`index.tsx` woła `stripFor` bez trzeciego argumentu — dług T-94) | **T-115** (T-102 zamknięte) |
| H28 | Żywy bieg: limit 8 KB usuwa końcowe `outcome:` z uciętej kopii (20/28 przekazań miało pełny załącznik), więc następny agent nie zna decyzji odczytanej przez silnik | **T-114** |
| H29 | Żywy bieg: sześć równoległych procesów Claude'a zapisuje wspólny `~/.claude.json`; jeden padł po 273 ms na uszkodzonym JSON-ie i nadał biegowi `processExit` | **T-127 wylądowało** (T-109 zamknięte) |

---

## 3. Zadania — wszystkie pełną pętlą (`ship-task.sh`, recenzent cross-vendor)

| ID | Tytuł | Zależy od | Dotyka `commands/run.rs` | Kryteriów (szac.) |
|---|---|---|---|---|
| T-98 | Przelotka nie sięga ponad dial: pełne listy i filtr po kluczach | — | nie | 4 |
| T-99 | **ZAMKNIĘTE:** sprzeczny wskaźnik i dwa błędy tekstu kryteriów | — | tak | 4 |
| T-100 | Werdykt jest polem, sędzia widzi próby | T-114 | tak | 4 |
| T-101 | Każda porażka przechodzi przez jedno miejsce — naprawdę | T-100 | tak | 4 |
| T-102 | **ZAMKNIĘTE:** zielone wyrocznie nie odróżniały kolumn cen ani sumy dwóch kroków | T-101, T-111 | tak | 4 |
| T-103 | **ZAMKNIĘTE:** dokładne evidence i argument Startu wymagały dwóch plików poza OWNS | T-115 | tak | 5 |
| T-104 | **ZAMKNIĘTE:** cztery filtrowane checki, brak pełnego adresu i konsumentów prawdziwego promptu | T-124, T-126 | tak (skan) | 5 |
| T-105 | **ZAMKNIĘTE:** AC-3 wymaga nieistniejącej flagi App Servera | — | nie | 3 |
| T-106 | Zatrzymanie ma sufit i eskalację | T-115 | tak | 3 |
| T-107 | Prawdziwy bieg jest wyrocznią fazy | wszystkie | nie (`e2e/`, `tests/` `--ignored`) | 3 |
| T-108 | Sprzątanie po D-6: martwa tabela i martwa gałąź odzyskiwania znikają | T-104 | nie | 2 |
| T-109 | **ZAMKNIĘTE:** trzy filtrowane checki i wymagane pliki produkcyjne poza OWNS | T-126 | nie | 3 |
| T-110 | **ZAMKNIĘTE:** pełna bramka wymagała fikstury App Servera spoza OWNS | — | nie | 3 |
| T-111 | Lead Codeksa: poprawny sandbox, jawna odmowa, prywatne MCP wyłączone, Connections zachowane | — (zastępuje T-105/T-110) | nie | 3 |
| T-112 | **ZAMKNIĘTE:** fałszywe `before` i kolizyjne kodowanie refów | — | tak | 5 |
| T-113 | **ZAMKNIĘTE:** spec wznowienia fałszował etykietę pochodzenia | — | tak | 6 |
| T-114 | Kopie: poprawne i niekolizyjne refy; prawdziwe pochodzenie i trwała decyzja | — (zastępuje T-99/T-112/T-113) | tak | 6 |
| T-115 | Wydatki obu vendorów z rozróżnialnym cennikiem i prawdziwą sumą na ekranie | — (zastępuje T-102) | tak | 4 |
| T-116 | **ZAMKNIĘTE:** idempotentny Store poza OWNS, wada setupu AC-6 i cztery luki wyroczni | T-115 | tak | 6 |
| T-117 | **ZAMKNIĘTE:** pierwsza bramka 19/22, martwa wyrocznia handlera, naprawa utracona na ENOSPC | T-115 | tak | 6 |
| T-118 | **ZAMKNIĘTE:** 20/22; AC-4 przepisało historyczne nagłówki zamiast kanonicznego formatu | T-115 | tak | 6 |
| T-119 | **ZAMKNIĘTE:** 17/22; logiczny klucz zamiast UUID, trzy pliki poza OWNS i dwa lity | T-115 | tak | 6 |
| T-120 | **ZAMKNIĘTE:** 19/22; wadliwy porządek eventów, `index.tsx` poza OWNS i regresje dubli/scope | T-115 | tak | 6 |
| T-121 | **WYLANDOWAŁO:** dokładny, idempotentny i atomowy snapshot czterech tabel Store | T-115 | nie | 2 |
| T-122 | **ZAMKNIĘTE:** dwa kolejne lity helperów po jedynej naprawie; copy-over przechodziło wyrocznię | T-121 | tak | 2 |
| T-123 | **ZAMKNIĘTE:** 19/20 po naprawie; martwa wyrocznia efektu i 116-wierszowa funkcja | T-121, T-124 | tak | 4 |
| T-124 | **WYLANDOWAŁO:** auto-pamięć kroku, pełny Markdown i trwały atomowy persist | T-121 | tak | 3 |
| T-125 | **ZAMKNIĘTE:** 15/20 po naprawie; zły korzeń skanu, timeout 6 s/5 s, `float_cmp` i format | T-121, T-124 | tak | 4 |
| T-126 | **WYLANDOWAŁO:** refleksja prywatna z fizycznym PGID, bieżącym budżetem i trzema stabilnymi drogami w Chromium | T-121, T-124 | tak | 4 |
| T-127 | **WYLANDOWAŁO:** prywatny stan każdego procesu Claude'a, także kopii i refleksji | T-126 (zastępuje T-109) | tak | 3 |
| T-128 | **ZAMKNIĘTE:** własne AC zielone, lecz dwa konieczne stare testy poza OWNS | T-127 (pierwszy następca T-104) | tak | 2 |
| T-136 | **ZAMKNIĘTE:** 15/18 po naprawie; zły oracle multizbioru, lint i trzy luki dowodu | T-127 (pełny następca T-128) | tak | 2 |
| T-137 | **ZAMKNIĘTE:** 17/19 po naprawie; refleksja atrapy nie przechodziła wrapperów, dwa martwe oracle | T-127 (pełny następca T-136) | tak | 3 |
| T-138 | **ZAMKNIĘTE:** 18/19 po naprawie; drugi lint i brak biblioteka→projekt tombstone | T-127 (pełny następca T-137) | tak | 3 |
| T-139 | **WYŁĄDOWANE:** dwa korzenie, trwały Move, pełny adres UI i folder renderowanych notatek | T-127 (pełny następca T-138) | tak | 3 |

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
globalnie unikalnego kontraktu po naprawie harnessu:
- AC-1: katalog drugiej kopii zachowuje `s_2~2`, ale dokładny poprawny ref to
  `loadout/<bieg>/s_2-2`; prawdziwe repo, równoległość i wznowienie własnej kopii.
- AC-2: trwały wskaźnik zostaje względny i przenośny; zmontowany prompt konkretnego odbiorcy
  podaje bezwzględny, otwieralny adres pełnej kopii w katalogu bieżącego biegu, także po
  wznowieniu i usunięciu starego katalogu.
- AC-3: ostatnie `outcome:` przeżywa limit dokładnie raz, bez syntezy decyzji.
- AC-4: puste ciało → stały dopisek w prawdziwym indeksie następnego kroku.
- AC-5: walidator odmawia `copies>1` na źródle strzałki powrotnej (`link.from`); cel powrotu
  i zwykłe kroki mogą mieć kopie.

**T-113 — ZAMKNIĘTE, bez lądowania.** Pięć AC było zielonych, lecz AC-3 po wznowieniu
wymagało dokładnej etykiety zwykłego poprzednika zamiast prawdziwej etykiety wcześniejszego
biegu. Jedyna naprawa poprawnie odmówiła fałszowania produkcji i zmiany zamrożonego speca.

**T-114 — pełne zastępstwo T-99/T-112/T-113.** Startuje z trunka po `5604c3d`; stare gałęzie
są dowodem, nigdy źródłem commitów lub speców.
- Zachowuje sześć celów T-113, ale każdy ma nową, globalnie unikalną ścieżkę testu.
- Dodatkowe AC liczy wszystkie planowane klucze `fresh-copy` tym samym kodowaniem, którego
  używa tworzenie gałęzi. Kolizja `s_2~2` z literalnym `s_2-2` jest ostrzeżeniem przy zapisie
  i Problemem przy Starcie; widoczne angielskie zdanie nazywa obie prace oraz wspólną gałąź.
- Odmowa pada przez prawdziwą komendę przed katalogiem biegu, drzewem Gita i pierwszym
  wywołaniem sterownika. Hash, losowy sufiks i zmiana nazw niekolizyjnych refów są zakazane.
- Spec adresu rozróżnia etykietę zwykłego poprzednika od etykiety przeniesionego pliku;
  oba warianty nadal wymagają absolutnej ścieżki do pełnej kopii bieżącego biegu.
- OWNS pozostaje w `run.rs`, `isolate.rs`, `handoff.rs`, `check.rs` i sześciu nowych specach;
  znalezisko historii refów w `commands/history.rs` zostaje zapisane poza tym zadaniem.

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

**T-102 — ZAMKNIĘTE, bez lądowania.** Formalna bramka była zielona 20/20, lecz równe
liczniki wejścia/cache/wyjścia dla Terra i Luna nie wykrywały zamiany stawek, a ekranowa
wyrocznia z jednym płatnym krokiem nie odróżniała sumy od pierwszej lub ostatniej kwoty.
Jedyna naprawa poprawiła lint i odmówiła wzmocnienia zamrożonych speców. Gałąź jest dowodem,
nie źródłem commitów, implementacji ani testów.

**T-115 — pełne zastępstwo T-102 (kształt z D-5: łagodnie).** Startuje z aktualnego trunka
i ma cztery nowe, globalnie unikalne ścieżki wyroczni.
- Codex: `cost_usd` szacowany z tokenów po tabeli cen w jednej stałej (wejście/wyjście/cache,
  po prefiksie modelu; nieznany model → tokeny bez dolarów + zdanie w `run.json`). Szacunek
  oznaczony jako szacunek (osobne pole, nie udawany pomiar).
- Każdy znany model dostaje nierówne 10k/5k/20k tokenów; test osobno wymaga Sol `$0.442`,
  Terra `$0.261` i Luna `$0.0261`, więc zamiana kolumn nie zachowuje zieleni.
- Pasek wydatków POKAZANY: `index.tsx` woła `stripFor` z trzecim argumentem (H27); bez
  ustawionego budżetu pasek pokazuje samą sumę wydatków biegu. Prawdziwy ekran dostaje co
  najmniej dwa płatne kroki różnych vendorów i musi pokazać ich sumę, nie jeden z operandów.
- Zachowanie budżetu bez zmian: twardy stop tylko gdy człowiek jawnie ustawił kwotę przy
  Starcie; wzoru ×N nie ruszamy, miękkość zapisana w docs (§5).
- Kroki Codeksa dostają w indeksie przekazań zdanie, że pliki leżą poza cwd i czyta się je
  po ścieżce bezwzględnej (H4; pomiar skuteczności w T-107).

**T-103 — ZAMKNIĘTE, bez lądowania.** Dokładne `logs/reflection.*` wymagały jawnej
tożsamości w `evidence.rs`, a nazwany argument Tauri wymagał `io.ts`; obu plików brakowało w
`OWNS`. Jedyna naprawa dotknęła ich poza zakresem i wyłączyła dwa istniejące duble refleksji.
Nie wznawiać gałęzi ani nie przenosić z niej commitów, implementacji lub speców.

**T-116 — ZAMKNIĘTE, bez lądowania.** Druga bramka została na 19/22. AC-2 trafiło w
`UNIQUE constraint failed: runs.id`, bo `Store::rebuild_from` nie było idempotentne, a poprawna
naprawa wymagała `store/mod.rs` i drogi jednego pisarza poza `OWNS`. AC-6 przewracał helper
speca wymagający front-matter od celowo zwykłego Markdownu. Recenzent znalazł dodatkowo brak
asercji `ran: false` po awarii opakowań, ominięcie prawdziwego handlera checkboxa, fałszywy
wariant „brak przekazania” oparty o krok `failed` i brak odmowy dodatkowych plików refleksji.
Nie wznawiać gałęzi ani nie przenosić z niej commitów, implementacji lub speców.

**T-117 — ZAMKNIĘTE, bez lądowania.** Pierwsza bramka miała 19/22: zwykły krok stracił
dotychczasową ścieżkę evidence, nowy test niósł zakazane `panic!`, a guard pustego wyniku
złamał wylądowane T-114. Recenzent wykazał, że AC-3 woła handler osobnego helpera, nie elementu
z drzewa `Start`. Planner rozpisał poprawną naprawę, ale wykonawca zginął na `ENOSPC` przed
pierwszą zmianą; branch został czysty, a Harness nie ma piątej tury. Nie wznawiać ani nie
przenosić commitów, implementacji lub speców.

**T-118 — ZAMKNIĘTE, bez lądowania.** Końcowa bramka miała 20/22; pełny clippy, wszystkie
quick checks oraz AC-1, AC-2, AC-3, AC-5 i AC-6 były zielone. Jedyny nowy test AC-4 wymagał
historycznych nagłówków `What changed / Decisions / Open questions`, choć właściciel formatu
eksportuje `Answer / Evidence / Open`. Zmiana produkcji byłaby oracle-specific hackiem, a po
jedynej naprawie nie wolno poprawić wyroczni ani ponowić bramki. Nie wznawiać ani nie
przenosić commitów, implementacji lub speców.

**T-119 — ZAMKNIĘTE, bez lądowania.** Po jedynej naprawie AC-2…AC-6 były zielone, lecz
końcowa bramka miała 17/22. AC-1 pomyliło logiczny klucz workflow `build` z fizycznym UUID
kroku używanym przez istniejący kontrakt evidence. Naprawa dotknęła `launch.ts`,
`requested-launch.ts` i `run-command.ts` poza `OWNS`, nowy test TS nie był sformatowany, a
pełny clippy odrzucił 123-wierszową funkcję testową. Recenzent wykazał ponadto brak asercji
domyślnego zaznaczenia, brak jawnej nieobecności starego artefaktu i heurystyczne zgadywanie
nazw tempów. Nie wznawiać ani nie przenosić commitów, implementacji, speców lub testów.

**T-120 — ZAMKNIĘTE, bez lądowania.** Końcowa bramka miała 19/22. AC-2 porównywało
`ORDER BY body` z kolejnością wejściową; prawdziwy właściciel stanu `/run`, `index.tsx`, był
poza `OWNS`; pełny test pokazał nieprzeniesione twarde opakowania dwóch dubli i niedozwolony
awans auto-pamięci kroku z `ThisAgent` do `ThisProject`. AC-1 i AC-3…AC-6, clippy, format,
typy i wiring były zielone. Nie wznawiać ani nie przenosić kodu, commitów lub speców.

**T-121 — Store jako jeden snapshot.**
- Wyłącznie `store/mod.rs` i `store/writer.rs`; jedno zlecenie jedynego pisarza, jedna
  transakcja usuwająca stary rodzic i zapisująca wszystkie cztery kolekcje.
- Zmienione źródła tego samego id zastępują wszystko, trzecia odbudowa jest idempotentna,
  późny trigger artefaktu zostawia cały poprzedni snapshot.
- Eventy porównywane jako dokładny multiset pełnych krotek z licznością, nie w kolejności
  inserta ani przez niepełne `ORDER BY`.

**T-122 — ZAMKNIĘTE, bez lądowania.** Oba AC oraz `full-test` były zielone, ale pierwsza
bramka miała 17/18 przez infallible `draft() -> Result<_>`. Jedyna naprawa usunęła ten lint,
po czym `full-clippy` odsłonił drugi taki sam defekt `fake_drivers() -> Result<_>` w drugim
nowym teście. Końcowe ENOSPC było wtórne; wcześniejsza pełna suita przeszła. Recenzent
wykazał ponadto, że temp-then-copy-over przechodzi AC-2. Nie wznawiać ani nie przenosić kodu,
commitów, speców lub testów.

**T-124 — WYLANDOWAŁO; H15 bez zmiany właściciela, następca T-122.**
- `what_the_steps_wrote_down` zachowuje `ThisAgent + agent`; tylko osobna refleksja całego
  biegu jest `ThisProject`.
- Pierwszy akapit, całe źródłowe body i `**Why:**` przechodzą przez właścicielski wariant API
  `memory::notes`; `run.rs` nie otwiera notatki drugi raz.
- Jeden atomowy temp+persist w katalogu celu; awaria i udany retry porównują pełny listing
  oraz stare/nowe bajty. Osobny test podmienia plik tylko do odczytu przy zapisywalnym
  katalogu, więc copy-over nie może udawać rename. Zamrożony test T-80 pozostaje zielony i
  poza `OWNS`.
- Fallible helpery testów zwracają `Result`, infallible helpery konkretny typ; pełny clippy
  nie może odsłonić tej samej klasy defektu dopiero po pierwszym błędzie.

**T-123 — ZAMKNIĘTE, bez lądowania.** Pierwsza bramka przeszła 20/20, ale recenzent
wykazał, że Stop nie sięga refleksji uruchomionej po schedulerze, a frontendowy test woła
`launchRequested` bezpośrednio i nigdy nie wykonuje produkcyjnego efektu Reacta. Jedyna
naprawa poprawiła późny Stop, lecz rozbudowała `a_short_turn_about` do 116 wierszy;
autorytatywna bramka 19/20 była czerwona na pełnym clippy. Gałąź zostaje dowodem, nie źródłem
commitów, implementacji, speców lub testów.

**T-125 — H14 po wylądowaniu T-121/T-124; świeży następca T-123.**
- Prywatne ustawienia, `EvidenceTarget::reflection`, osobny sufit, addytywny rachunek w
  `run.json`, fizyczny UUID zwykłego evidence i brak fallbacku po awarii wrappera.
- Dwa istniejące duble z `reflecting()` obowiązkowo przenoszą wszystkie twarde opakowania bez
  zmiany scenariuszy/asercji.
- Wspólny stan mieszka w produkcyjnym `Run`. Browserowy oracle montuje prawdziwą aplikację
  przez istniejący `e2e/harness.ts`, zmienia widoczny checkbox i uruchamia manualny Start,
  Enter z `/run` oraz prawdziwy przycisk Run edytora. Ostatnia droga przechodzi przez
  zamontowany `useSyncExternalStore`/`useEffect`, nie przez bezpośrednie wywołanie helpera.
- Stop po skończeniu schedulera czeka na żywy proces refleksji, anuluje jego `AgentHandle` i
  dowodzi śmierci grupy. Wyłączenie i `left_nothing` także dają `ran:false`; tryb klona, nie
  model, przydziela budżet. Dotknięte funkcje produkcyjne mają najwyżej 100 wierszy.

**T-125 — ZAMKNIĘTE, bez lądowania.** Po jedynej naprawie AC-3 i AC-4 były zielone, lecz
autorytatywna bramka miała 15/20. AC-1 użyło `scan_notes(<home>/memory/notes)` zamiast
`scan_notes(<home>/memory)`, wszystkie sześć scen AC-2 czekało 6 sekund pod pięciosekundowym
limitem Vitest, pełny clippy znalazł ścisłe porównanie `f64`, a formatter kolejność importów.
Gałąź zostaje dowodem, nie źródłem commitów, implementacji, speców lub testów.

**T-126 — H14 po wylądowaniu T-121/T-124; świeży następca T-125.** Zachowuje mocniejsze
uwagi recenzenta T-125, lecz ma cztery nowe targety. Skanuje korzeń pamięci bez podwójnego
`notes`, obserwuje pierwsze IPC do 4 sekund i późne duplikaty przez co najmniej 300 ms pod
jawnym limitem ≥15 s, dowodzi Stopu prawdziwym PGID i `ESRCH`, rozróżnia tryb stanem klona
przy identycznych promptach, porównuje `f64` tolerancją i sprawdza zachowane pola starej
historii. Nie przenosi niczego z zamkniętej gałęzi.

**T-126 — WYLANDOWAŁO.** Końcowa bramka gałęzi przeszła 20/20, a obie bramki integracyjne
16/16. Naprawa odmawia uznania Stopu refleksji bez `GroupProof::Dead`. Recenzent wykazał
ograniczenie AC-4: bieżący kod używa dokładnego `REFLECTION_BUDGET_USD`, lecz target sprawdza
ręcznie złożony klon zamiast realnego `reflection_driver`; tę produkcyjną ścieżkę musi
wykonać końcowy oracle fazy.

**T-104 — ZAMKNIĘTE, bez uruchomienia.** Cztery rustowe `check:` filtrują funkcje wspólnego
targetu `tests/it`, więc łamią globalnie unikalną ścieżkę z `AGENTS.md` §2a. Kontrakt miesza
ponadto trzy osobne prawdy: fizyczny adres i mutacje, bieżący stan budżetu oraz zamrożony
receipt biegu. Nie posiada `AppState::project_for`, konsumentów prawdziwego promptu ani
regresji T-126, więc akcja Move mogłaby wylądować przed odczytem drugiego korzenia i kłamać
człowiekowi. T-128 zamknięto na dwóch starych testach poza OWNS, a T-136 po czerwonej
bramce i lukach oracle; T-137 zamknięto 17/19, T-138 18/19, a zakres przejmują świeże T-139 → T-129 → T-130. Niczego nie przenosi
się ze starej gałęzi T-104.

**T-129 i T-130 — KONTRAKTY UTWORZONE.** T-129 ma trzy standalone targety dla bieżącego
katalogu, prawdziwego Memory UI i bieżącego ekranu agenta. Nie dotyka `run.rs`, historii ani
Store. T-130 rusza dopiero po jego lądowaniu i ma trzy osobne targety dla rzeczywistych
odbiorców zapisanych po udanym `AgentDriver::start`, tolerancyjnego odczytu historycznego oraz
pełnego ekranu `/history`. Receipt zostaje w `run.json`; SQLite nie dostaje martwej kopii bez
produkcyjnego zapytania. Oba kontrakty z góry posiadają historyczne wyrocznie, które zmiana
kształtu lub tekstu może legalnie wymagać poprawić.

**T-128 — ZAMKNIĘTE, bez lądowania.** Oba nowe AC były zielone, lecz pełna suita ujawniła
pięć historycznych fixture zakładających bibliotekę dla `this-project`. Po jawnie zatwierdzonej
naprawie testów pełna bramka odmówiła dwóm koniecznym plikom poza `OWNS` i znalazła kolejny
stary adres w należącym do zadania teście evidence. Gałąź oraz dwa produkcyjne commity są
dowodem; kontraktowych i testowych commitów nie przenosić.

**T-136 — ZAMKNIĘTE, bez lądowania.** Implementacja dwóch korzeni i prawdziwe akcje powstały,
ale końcowa bramka po jedynej naprawie pozostała 15/18. Oracle porównywał kolejność zamiast
multizbioru, pełny clippy znalazł kolejny lint, a recenzent wykazał trzy drogi pozornego
przejścia: ponowny odczyt przy stemplu, nieobserwowalny protokół trwałości Move i słaby
przypadek prefiksu tombstone. Gałąź oraz trzy commity implementacyjne są dowodem, nie lądowaniem.

**T-137 — ZAMKNIĘTE, bez lądowania.** Enforced `before` certyfikowało trzy targety, AC-2 i
AC-3 przeszły, a jedyna naprawa usunęła ponowny parse/render snapshotu. Końcowa bramka
pozostała 17/19: atrapowy driver zwracał `reflecting()`, lecz nie implementował wrapperów
ustawień, evidence i budżetu, więc refleksja nie startowała. Recenzent wykazał też, że ślad
Move zapisywał próby przed delegowaniem, a E2E nie przypinało legacy do strefy
`earlier-project`. Gałąź i pięć commitów implementacyjnych są dowodem, nie lądowaniem.

**T-138 — ZAMKNIĘTE, bez lądowania.** Wszystkie trzy AC i pełna suita przeszły, lecz obie
bramki miały 18/19. Pierwszy lint identycznych ramion naprawiono; drugi odrzucił 131-wierszową
funkcję snapshot/reflection. Recenzent wykazał też, że test exact tombstone wołał tylko drogę
jednego korzenia i nie dowodził tłumienia projektu przez tombstone biblioteki. Siedem commitów
produkcyjnych jest dowodem do jawnego przejęcia, kontrakt i targety nie są.

**T-139 — WYŁĄDOWANE.** Enforced `before`, pierwsza bramka i bramka wykonawcy naprawy były
uczciwe i zielone. Recenzent znalazł oraz naprawa domknęła zamrożenie folderu widocznych
notatek podczas B → C. Po końcowym timeoutcie starego testu recovery właściciel jawnie wybrał
`integrate.sh`; pierwszą próbę zatrzymało 105 MiB wolnego dysku. Po usunięciu wyłącznie
regenerowalnego cache T-136 main przeszedł 16/16 przed i po merge'u `0fb49a4`.

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
  pełne fikstury App Servera i unikalne testy; bez `run.rs`. Musi wylądować przed T-115.
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

**T-109 — ZAMKNIĘTE, bez uruchomienia.** Każde AC filtruje funkcję we wspólnym targecie
`tests/it`, a wymagane `commands/run.rs` i vendor-neutralne `drivers/mod.rs` są poza OWNS.
Znana niewykonalność kontraktu nie jest powodem, żeby odpalać harness „dla sprawdzenia”.

**T-127 — WYLANDOWAŁO; prywatny stan Claude'a, następca T-109.**
- `StepSettings` dostaje vendor-neutralny `work_key`; zwykłe kopie używają fizycznych kluczy
  `s` / `s~2`, a refleksja `_reflection`.
- `RunSettings::for_step` zakłada `<bieg>/claude/<work-key>`, a finalne środowisko prawdziwego
  spawnu nadpisuje każdą hostile wartość `CLAUDE_CONFIG_DIR` po `env_clear`.
- Nie kopiuje ani nie czyta stanu i poświadczeń gospodarza; `HOME/.claude.json` ma pozostać
  bajtowo nietknięte.
- Dwie kopie nakładają się w czasie, refleksja jest dokładnie trzecim izolowanym spawnem, a
  nieużywalny katalog daje widoczną odmowę i błąd w `run.json` przed pierwszym procesem.
- Końcowy oracle mierzy prawdziwe CLI i odcisk gospodarza.
- Gałąź przeszła 19/19 po jedynej naprawie dwóch luk dowodu, a obie bramki integracyjne
  przeszły 16/16. W trunku jako `c5bcc5c`.

---

## 4. Kolejność — z zależności, nie z fal

- **T-114, T-100, T-101, T-115, T-121, T-124, T-126, T-127 i T-139 wylądowały; T-102, T-103, T-104, T-109, T-116, T-117, T-118, T-119, T-120, T-122, T-123, T-125, T-128, T-136, T-137 i T-138 są zamknięte. T-129 i T-130 mają świeże kontrakty; T-129 jest następne.**
  Nie wznawiać zamkniętych gałęzi ani nie przenosić ich testów, implementacji lub commitów,
  Zamkniętych gałęzi nie wolno lądować ani przenosić w całości; T-139 stoi już w trunku.
  Trzy niezależne domeny T-120 są osobno lądowalne: T-121 Store wylądowało, T-124 przejęło
  H15 po zamkniętym T-122, T-126 domknęło H14 po zamkniętych T-123 i T-125, T-127
  domknęło H29 po niewykonalnym T-109, a T-139 domknęło H16/H18.
- **Łańcuch `run.rs`** (dzielony OWNS, więc szeregowo):
  `T-114 → T-100 → T-101 → T-102 (zamknięte) → T-115 → T-103…T-120 (zamknięte) → T-122 (zamknięte) → T-124 → T-123 (zamknięte) → T-125 → T-126 → T-127 → T-128 (zamknięte) → T-136 (zamknięte) → T-137 (zamknięte) → T-138 (zamknięte) → T-139 → T-129 → T-130 → świeże zadania Stop/startup`.
- **T-121 wylądowało najpierw**, mimo rozłącznego `OWNS`: T-126 zapisuje rachunek do pliku,
  którego ponowną, atomową indeksację gwarantuje T-121. T-122 i T-123 zamknięto; T-124
  wylądowało, T-125 zamknięto, a T-126 wylądowało samo przez `run.rs`.
- **T-127 po T-126 wylądowało.** T-128 zamknięto na dwóch starych testach poza OWNS, T-136
  po czerwonej bramce i lukach oracle, T-137 po 17/19, T-138 po 18/19 i luce tombstone'a,
  a T-139 wylądowało po zielonych bramkach integracji. T-129 musi wylądować przed receiptem
  T-130. T-129 świadomie nie posiada `run.rs`, ale jest semantycznym wejściem T-130: zamraża
  kształt pochodzenia, który receipt ma potem zachować. Dlatego oba zadania idą szeregowo.
- **Równolegle** (zmierzone porównaniem bloków OWNS 2026-08-24, nie założone): pierwotną parą
  bez ani jednego wspólnego pliku było **T-98 ∥ T-105**. T-98 wylądowało, T-105 i pierwsze
  zastępstwo T-110, T-99, T-112 oraz T-113 zostały zamknięte; T-111 wylądowało, harness
  naprawił `5604c3d`, a T-114 ma zgodę właściciela. Wszystko dalej
  dzieli `run.rs`,
  `check.rs`, `codex.rs`, `drivers/mod.rs` albo `recovery.rs` i idzie szeregowo:
  T-114 po T-98 (`workflow/check.rs`), T-115 po T-111 (`codex.rs`),
  świeże zadania recovery idą po Stop/startup, a świeży następca T-107 na końcu sądzi
  zachowanie z T-100, T-126 i T-127.
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
  `claude/<work-key>` (po T-127).
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
