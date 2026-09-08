# Wspólny plan między krokami — plan wykonawczy

Zlecenie: [`CLAUDE-HANDOFF.md`](CLAUDE-HANDOFF.md) — wspólny plan workflow, etapy WP-01…WP-07.
Ten plik jest **planem wykonania**, wymaganym przez §13 zlecenia: rzeczywiste ścieżki,
zależności od etapów Context (CT) i test dla każdego etapu.

Funkcja **Context** jest osobną, równolegle budowaną dostawą
([`../context-library/PLAN.md`](../context-library/PLAN.md),
dziennik: [`../context-library/IMPLEMENTATION.md`](../context-library/IMPLEMENTATION.md)).
Context trzyma **wielokrotnie używane materiały źródłowe**; Plan jest **dokumentem
konkretnego biegu**. Współdzielą składanie wejść, budżet i dostęp do źródeł.

---

## 0. Pomiar, który zmienia WP-03 — zrobiony przed napisaniem tego planu

Zlecenie zakłada w §6 „minimalną operację zgłoszenia planu, **np.** `submit_plan`" na moście.
Zmierzyłem, czy to jest wykonalne u obu vendorów. **Nie jest — u Codeksa.**

Sonda: jeden serwer MCP po stdio, dwa narzędzia w tej samej sesji — `read_thing`
z `annotations.readOnlyHint: true` i `submit_thing` bez niej. `codex-cli 0.153.4`,
`gpt-5.6-sol`, `codex exec`:

```
read_thing    → completed,  READ_OK_9931
submit_thing  → failed,     "MCP tool call requires approval, but approval policy is never"
```

Pełna przestrzeń opcji dla narzędzia **mutującego** pod `codex exec`:

| Droga | Wynik |
|---|---|
| domyślnie (`-s read-only`, polityka `never`) | odmowa |
| `-c mcp_servers.<n>.enabled=true` | odmowa |
| `-c approval_policy=on-failure` / `on-request` / `untrusted` | **nadpisanie ignorowane**, komunikat dalej mówi `is never` |
| `annotations.readOnlyHint: true` | przechodzi — ale **jest kłamstwem** o narzędziu zmieniającym stan |
| `--dangerously-bypass-approvals-and-sandbox` | przechodzi — zdejmuje cały dial, zakazane wprost w `engine/drivers/codex.rs:2287` |

`--full-auto` nie jest flagą `codex exec`.

**Wniosek: dla `submit_plan` nie ma legalnej drogi przez MCP w kroku Codeksa.**
To nie jest kwestia znalezienia właściwej flagi.

### Droga wybrana zamiast tego: kandydat w pliku, ścieżkę dyktuje Loadout

Krok Create/Update zapisuje kandydata do **ścieżki wskazanej przez Loadouta w swoim
worktree**; Loadout czyta go i waliduje **po zakończeniu kroku**. Spełnia to §6 i §9
dosłownie, a nie przez interpretację:

- „Agent proponuje dane, a Loadout je zapisuje" — dosłownie prawda;
- „Zgłoszenie jest kandydatem, nie natychmiastową publikacją" — wychodzi z konstrukcji;
- „Agent nie wybiera ścieżki zapisu" — ścieżkę podaje aplikacja;
- „Zachowaj natywne wykonanie kroków obu vendorów" — działa identycznie na `codex exec`
  i na `claude -p`, bez przepinania Codeksa na transport rozmowy;
- brak `readOnlyHint` na czymkolwiek mutującym, brak zdejmowania piaskownicy.

**MCP zostaje tam, gdzie jest uczciwe: do CZYTANIA** szczegółów planu — tak samo jak
czytniki Context. Ta droga jest zmierzona i działa u obu vendorów
(patrz `../context-library/IMPLEMENTATION.md` §0d).

**Odrzucone nośniki i dlaczego.** Kontrakt `handover` (`FIELDS_ASKED_FOR`,
`run.rs:678`) jest **liniowy** — `nazwa: wartość` w osobnym wierszu — więc nie uniesie
wielolinijkowego dokumentu. Transport rozmowy (App Server) dla wszystkich kroków Codeksa
jest zakazany przez samo zlecenie.

### Pomiar, którego JESZCZE NIE ZROBIŁEM — pierwsza rzecz w WP-03

Czy **App Server** (transport lidera) też odmawia narzędzi mutujących. Ustawia
`approvalPolicy: "never"` tym samym słowem (`engine/drivers/codex.rs:1644`).

Stawka jest większa niż ten plan: jeżeli odmawia, to **lider Codeksa nie może dziś wywołać
żadnego ze swoich mutujących czasowników** — `start_workflow`, `stop_run`, `ask_the_person`
(`bridge/verbs.rs:117`, `:149`, `:84`) — czyli byłaby to **żywa wada w wydanym produkcie**,
niezależna od funkcji Plan. Do czasu pomiaru: `not-tested`, bez domysłów w żadną stronę.

**Próbowałem i przerwałem — co z tego wiem, a czego nie.** Napisałem sondę mówiącą do
`codex app-server --listen stdio://` protokołem z `codex.rs:1606-1720`. Ustalone:

- App Server **wstaje i ładuje serwery MCP**: mój `probe2` zameldował
  `mcpServer/startupStatus/updated → status: "ready"`;
- `thread/start` z `{ephemeral, approvalPolicy: "never", sandbox: "read-only", model}`
  **zakłada wątek**; identyfikator wraca w `result.thread.id`, a nie `result.threadId`;
- ta droga wciąga też **prywatne serwery MCP użytkownika**, dlatego Loadout je wycisza
  przez `curated_mcp_overrides` (`codex.rs:1364`) — sonda bez tego widziała cudze `notion`
  i `linear-server`.

**Czego NIE ustaliłem:** `turn/start` nie oddał w mojej sondzie ani jednego zdarzenia tury,
więc **do samego wywołania narzędzia nigdy nie doszło**. To nie jest odpowiedź „App Server
odmawia" — to jest „moja sonda nie dojechała do pytania".

**Dlaczego przerwałem i co jest właściwą drogą.** Wierne odtworzenie tego handshake'u to
osobny projekt: dochodzi `config/read`, kuratela serwerów i dokładny kształt `input`.
Tańszy i uczciwszy pomiar to **przejście tej drogi produktem** — lider Codeksa w działającej
aplikacji, proszony o czasownik mutujący. To i tak należy do natywnego QA (WP-07 / CT-09),
więc pomiar wykonuję tam, zamiast utrzymywać drugą, niewierną implementację protokołu.

---

## 1. Zależności od etapów Context

| Etap WP | Blokowany przez | Powód |
|---|---|---|
| **WP-01** | **nic** | Nowy moduł `work_plan/`, zero plików wspólnych z CT. Zlecenie §1 dopuszcza to wprost. |
| WP-02 | CT-05 | Obie funkcje dokładają pole kroku i ruszają `workflow/{mod,file,check}.rs`, `state/workflows.ts`, `step-panel/more-settings.tsx`. Format pliku ustala się **raz**, po CT-05. |
| WP-03 | CT-03 | Most i `bridge/verbs.rs`; czytelnik szczegółów planu stoi na tym samym rdzeniu dostępu. |
| WP-04 | CT-06 | „Integracja z **gotowym** Context" — wspólna kompozycja wejścia żyje w module dostarczonym przez CT-06. |
| WP-05 | CT-06 | Kryteria i ocena czytają dostarczone wejście. |
| WP-06 | CT-07, CT-08 | Lead, Start, historia, replay, Lab. |
| WP-07 | CT-09 | Natywne QA obu funkcji naraz, żeby nie płacić dwa razy za ten sam dowód. |

**Jedenaście plików, o które biją się oba zlecenia** — dlatego kolejność powyżej nie jest
ostrożnością, tylko warunkiem uniknięcia konfliktu:

```
src-tauri/src/workflow/{mod,file,check}.rs      src-tauri/src/commands/run.rs
src-tauri/src/bridge/{verbs,serve}.rs           src-tauri/src/evidence.rs
src-tauri/src/ipc.rs                            src-tauri/commands.golden.txt
src/state/workflows.ts                          src/sections/workflows/step-panel/more-settings.tsx
```

### Format pliku workflow — jedna decyzja, nie dwie

Na `main` `workflow::file::CURRENT` wynosi **1**, a `MIGRATIONS` jest **pusta**
(`workflow/file.rs:28`, `:39`). Context planuje podniesienie do 2.

**Format 2 nie ochroni automatycznie dwóch niezależnie dodanych funkcji.** Starszy czytnik
odmawia po numerze, więc dokument z Planem musi wymagać formatu **wyższego niż ten, który
zna czytnik bez Planu**. Tę liczbę ustala WP-02 **po** CT-05, patrząc na rzeczywisty stan —
nie zakładamy jej dziś. Rozdzielić **najwyższy obsługiwany format** od **formatu potrzebnego
konkretnemu dokumentowi**: workflow bez Planu i bez Contextu nie ma prawa dostać nowego
numeru przy zwykłym zapisie.

---

## 2. Etapy: ścieżki i test

Nowa logika: **`src-tauri/src/work_plan/`** (model, walidacja zmian, publikacja, render)
oraz **`src-tauri/src/workflow/work_plan.rs`** (parser konfiguracji kroku).
Nie mylić z istniejącym wewnętrznym typem `Plan` w `commands/run.rs` ani z planem Lab.
W `run.rs` zostają **cienkie wpięcia**.

### WP-01 — model dokumentu, wersje, publikacja *(bez zależności)*

`work_plan/{mod,document,change,publish,render}.rs`.

Schemat: cel, zakres i wyłączenia; wymagania ze **stabilnymi ID**; kryteria akceptacji
z metodą sprawdzenia; decyzje i ograniczenia; sekcje szczegółowe; propozycje, założenia,
pytania i konflikty; odwołania do źródeł i ich wersji. Wersja niesie ID biegu i dokumentu,
numer wersji, rodzica, **fizyczny krok i próbę autora** oraz wersję schematu i renderera.
Tożsamość, ścieżkę, pochodzenie i wskaźnik aktualnej wersji nadaje **aplikacja**.

Publikacja przez istniejące `durable_file.rs` — atomowa, **warunkowa względem rodzica**,
idempotentna względem operacji. Zero `last writer wins`.

**Test:** `src-tauri/tests/it/work_plan_versions_are_immutable.rs`
(`cargo test --test it work_plan_versions_are_immutable::`).
Aktualizacja sekcji Design **zachowuje wymagania** spoza zakresu i nie renumeruje ID;
konflikt rodzica odrzucony; awaria w połowie publikacji **nie niszczy** starej wersji;
odczyt po restarcie bez SQLite; `unchanged` **nie tworzy** nowej wersji; powtórzona
publikacja tej samej operacji nie daje v4 i v5.

Osobno: **propozycja modelu nie może stać się zatwierdzonym wymaganiem** przez sam zapis
planu — pochodzenie (`human` / `generated`) jest odrębne od statusu.

### WP-02 — konfiguracja w workflow i panelu, resolver *(po CT-05)*

`workflow/work_plan.rs`, wpięcia w `workflow/{mod,file,check}.rs`, `src/state/workflows.ts`,
`src/sections/workflows/step-panel/{panel,more-settings}.tsx`.

`Plan: Off | Create | Update | Use` w **istniejących dodatkowych ustawieniach** — bez
szóstego stale rozwiniętego pola. Zmienione ustawienie widoczne w podsumowaniu panelu.
Jeden resolver zależności, wspólny dla zapisu, podglądu, Startu, wznowień i wykonania.

**Test:** `src-tauri/tests/it/work_plan_graph_is_unambiguous.rs` +
`src/sections/workflows/step-panel/plan-row.test.tsx` +
`e2e/tests/work-plan-config.spec.ts`.
Brak Create; drugi Create; **dwaj nieuporządkowani autorzy** odrzuceni **przed pierwszym
procesem**; niejednoznaczne źródło przy join; niedozwolona pętla autora; remapowanie ID przy
duplikowaniu kroku i workflow; round-trip przez **prawdziwe IPC**; stare dokumenty bez pola
działają jako Off i **nie dostają nowego numeru formatu**; starszy czytnik **odmawia**
dokumentu z Planem.

### WP-03 — Create/Update przez krok, Use z przypiętą wersją *(po CT-03)*

**Najpierw pomiar App Servera z §0.** Potem: droga kandydata w pliku (§0), walidacja
i publikacja po zakończeniu kroku, czytelnik szczegółów planu na rdzeniu dostępu z CT-03.

**Test:** `src-tauri/tests/it/work_plan_candidate_becomes_a_version.rs`.
Pełny obrót przez **oba produkcyjne adaptery** z kontrolowanym procesem; brak poprawnego
kandydata to **nazwane niepowodzenie**, którego `exit 0` nie zmienia; kandydat **nie**
publikuje się po Stopie, timeoucie ani przez spóźnioną próbę; podwójna publikacja
idempotentna; Use i Off **nie mają** uprawnienia do publikacji, także przy bezpośrednim
wywołaniu mostu.

### WP-04 — wspólna kompozycja wejścia *(po CT-06)*

Rozszerzenie modułu dostarczonego przez CT-06, **nie drugi kompozytor**. Trzy warstwy:
obowiązkowy rdzeń w całości, szczegóły dobrane do kroku, indeks plus odczyt na żądanie.
Ta sama obowiązkowa treść tej samej wersji renderowana **identycznie** dla implementera
i QA, niezależnie od vendora. Poprawka `memory/handoff.rs` (`BODY_CAP`, `cap`): krótki
wstęp nie może wypchnąć sekcji merytorycznych na rzecz samych wskaźników.

**Test:** `src-tauri/tests/it/work_plan_core_reaches_both_vendors.rs` +
`src-tauri/tests/it/handoff_keeps_substance_under_cap.rs`.
Asercje na **bajtach odebranych przez kontrolowany proces za rzeczywistym adapterem**,
nie na wyniku resolvera. Rdzeń, który się nie mieści, **odmawia startu konsumenta** nazwanym
zdaniem na ekranie — nie jest obcinany ani zastępowany „Moved to …".

### WP-05 — ocena względem tej samej podstawy *(po CT-06)*

Istniejące `workflow/criteria.rs` i mechanizm oceny — **bez drugiego enuma wyników**.
Pobranie kryteriów z planu jest **jawną konfiguracją** widoczną w panelu, nigdy skutkiem
nazwy kafelka.

**Test:** `src-tauri/tests/it/work_plan_review_shares_the_version.rs`.
QA sprawdza tę samą wersję **i ten sam wynik pracy**; `passed` / `failed` / `not-tested`
zostają rozłączne; brak pomiaru **nie jest** zaliczeniem; stara nierozwiązana uwaga
**nie znika** przez brak wzmianki w nowszym raporcie.

### WP-06 — historia, replay, Lead, Lab *(po CT-07, CT-08)*

**Test:** `src-tauri/tests/it/work_plan_survives_replay.rs`.
Odtworzenie zapisanych wejść konsumenta przywraca **tę samą treść**; pełne ponowne wykonanie
grafu z Create/Update **generuje wynik od nowa** i nie wolno nazywać tego identycznym
odtworzeniem; uszkodzony snapshot to **odmowa**, nie „podobny" plan z repo.

### WP-07 — natywne QA i pomiar *(po CT-09)*

Prawdziwa aplikacja Tauri na macOS z backendem, oba CLI, przekazanie Claude → Codex
i Codex → Claude. Pomiar z §15 na zamrożonych danych. Brak metryki to **brak danych**,
nie zero. Luka bez pokrycia zostaje `not-tested` i **blokuje** deklarację gotowości.

---

## 3. Stan

| Etap | Stan |
|---|---|
| §0 pomiar Codeksa | **zrobiony** — droga MCP dla mutacji zamknięta, wybrana droga kandydata w pliku |
| §0 pomiar App Servera | `not-tested` — pierwsza rzecz w WP-03 |
| **WP-01** | **WYLĄDOWANY** (`3a83d1ce`), CI zielone 759 s. Codex, 2 rundy, 35 min, **10 testów**. Mutacja: propozycja modelu wpuszczona jako zatwierdzone wymaganie → właściwy test padł. |
| **WP-04a** | **WYLĄDOWANY**, CI zielone 596 s. Codex, **1 runda, 17 min**, 6 testów, mutacja zabiła dokładnie 2 właściwe. Poprawka `cap()` z §8 zlecenia. |
| **WP-02** | **WYLĄDOWANY**, CI zielone 505 s. Codex, 3 rundy + naprawa ręczna, **9 testów Rusta + 7 frontu**. |
| **WP-03** | **WYLĄDOWANY**, CI zielone. Codex, 3 rundy + naprawa ręczna, **8 testów** (w tym dowód mutacyjny na identyfikatorach z panelu). |
| **WP-04b** | **GOTOWY DO LĄDOWANIA** — jeden kompozytor Planu, Context i przekazań; **6 zawężonych testów Rusta** na stdin obu adapterów, wspólnym limicie, odmowie i ponownej turze. |
| **WP-04b** | **WYLĄDOWANY**, CI zielone 524 s. Codex, **1 runda**, 3758 s, **6 testów**. Dwie mutacje trafiły po 2 i po 1 właściwym teście. Jedna regresja złapana dopiero pełnym CI — patrz niżej. |
| **WP-05** | **WYLĄDOWANY**, CI zielone. Codex, 3 rundy, 5432 s, **8 testów**. Dwie mutacje trafiły w 2 i w 1 właściwy test. |
| WP-06…WP-07 | czekają na CT-08 i CT-09 |

### WP-04b: `STEP_PROMPT_BYTES` znaczy teraz sumę, nie „tyle dla Contextu"

Trzy osobne przydziały po 24 KiB pozwalały Loadoutowi dołożyć **72 KiB** przed instrukcjami
i historią vendora. Jedna liczba ogranicza teraz Plan, wymagania, indeks Contextu i indeks
przekazań **razem**. `render_core` nie przyjmuje roli **w ogóle**, więc identyczność rdzenia
dla implementera i QA jest strukturalna, a nie pilnowana sprawdzeniem — to ten sam chwyt,
którym WP-01 zamknął autorytet propozycji modelu.

### Regresja, którą złapała dopiero wyrocznia bajt w bajt

`index_of_what_came_before` zaczynała bezwarunkowym `\n\n`, bo dopisywała na koniec
**niepustego** promptu. WP-04b przekierował ją do świeżego bufora wspólnego bloku i separator
stał się pustą linią na początku, a wołający dokleił własny — prompt Claude'a urósł o dwa znaki.
Złapał to `t115_codex_handoff_paths_are_actionable::the_claude_prompt_is_byte_for_byte_the_pre_t115_prompt`,
czyli test, którego cały sens polega na tym, że zmiana po stronie Codeksa nie rusza **ani jednego
bajtu** promptu Claude'a. Naprawiony **produkt**, nie test (`584553a4`).

**Wniosek na resztę dostawy:** wąska bramka etapu tego nie widziała. Kotwica na treści zamiast
na bajtach przepuściłaby dwie puste linie w prompcie każdego kroku Claude'a.

### Format pliku workflow — liczba wyprowadzona, nie wybrana

WP-02 rozstrzygnął to, przed czym ostrzegało §10 zlecenia („nie zakładaj, że format 2
automatycznie chroni dwie niezależnie dodane funkcje"). W `workflow/file.rs` stoją dziś:

```rust
pub const CURRENT: u32 = 1;              // co zachowuje ZWYKŁY dokument
pub const CONTEXT_FORMAT: u32 = 2;       // czego wymaga dokument z Context
pub const PLAN_FORMAT: u32 = 3;          // czego wymaga dokument z Planem
pub const HIGHEST_SUPPORTED: u32 = PLAN_FORMAT;
```

Dokument dostaje **najwyższy format, którego naprawdę potrzebuje**. Gdyby Plan wymagał
dwójki, build znający Context, ale nie Plan, przyjąłby taki dokument i wykonał go
**ignorując Plan** — po cichu inaczej, niż chciał człowiek.

**Skutek uboczny, złapany dopiero pełnym CI:** `workflow_load_forward` zaszywał `"format": 3`
jako „plik z przyszłości", więc podniesienie sufitu odebrało mu przesłankę. Liczba jest tam
teraz liczona jako `HIGHEST_SUPPORTED + 1` i podniesie się sama przy każdym kolejnym formacie.

### WP-05: tożsamość sprawdzona z OBU stron

Najtrudniejsze wymaganie §8 brzmi „to samo ID planu przy innym kodzie nie dowodzi sprawdzenia
właściwego produktu". Bieg oddał na to **dwa lustrzane testy**, dokładnie tak, jak żąda lekcja
z CT-02: `the_same_plan_id_does_not_cover_another_commit` **oraz**
`the_same_work_does_not_cover_another_plan_version`. Jeden test na obie rzeczy naraz nie
odróżniałby ich od siebie.

`about_other_work` porównuje **parę** (wersja planu, odcisk pracy) i ma osobne zdanie dla każdej
z czterech kombinacji — meldunek o innym produkcie nie opisuje ani wady, ani zaliczenia tego
produktu, więc wymagane kryteria lądują w `not_tested`, a werdykt w `NotJudged`.

Mutacje: zdjęcie porównania odcisku pracy przewraca 2 właściwe testy, dopuszczenie milczenia do
zamykania starej uwagi — 1 właściwy.

### Trzy zatrzymania biegu, wszystkie z tej samej przyczyny

CT-04, CT-05 i WP-02 zatrzymały się po trzech rundach, a **za każdym razem kod był poprawny**.
Czerwony był lint albo formatter, który **zabija kompilację całego celu `it`** — wtedy
`rust-test` melduje „POMINIETY" i **ani jeden napisany test nie biegnie**, więc weryfikator
słusznie mówi „żaden punkt akceptacji nie ma dowodu wykonania". Naprawa zajmowała minuty,
bieg tracił na tym trzy rundy.

Najczęstsze: brak `#![allow(clippy::expect_used)]` w nowym module testowym (mają go wszyscy
sąsiedzi), asercja nad samymi stałymi, blok wokół literału struktury w domknięciu.
Dopisane do promptów CT-07 i CT-08 razem z komendą do uruchomienia przed zgłoszeniem.

### Jak WP-01 rozwiązał najtrudniejsze wymaganie zlecenia

„Model może zaproponować wymaganie, ale nie może oznaczyć go jako zatwierdzone przez
człowieka" (§4) nie zostało zrobione **sprawdzeniem**, tylko **strukturą**:
`ProposedRequirement` nie ma pól `origin` ani `status` **w ogóle**, a `deny_unknown_fields`
zamyka drogę na skróty. Model nie może nadać sobie autorytetu, bo nie ma go gdzie wpisać —
to jest mocniejsze niż walidator, który trzeba pamiętać wywołać.
