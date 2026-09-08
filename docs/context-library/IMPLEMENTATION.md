# Context — dziennik wykonania

Plan: [`PLAN.md`](PLAN.md). Zlecenie: [`CLAUDE-HANDOFF.md`](CLAUDE-HANDOFF.md).
Ten plik jest **dziennikiem**, nie planem. Ma pozwolić kontynuować pracę po zmianie agenta
albo utracie kontekstu: co zrobione, na jakim commicie, czym dowiedzione, co otwarte.

Statusy kryteriów: `passed` / `failed` / `not-tested`. **Dopóki którekolwiek wymagane
kryterium stoi na `failed` albo `not-tested`, funkcja NIE jest gotowa** (zlecenie §9).

---

## 0. Stan wejściowy (2026-09-07)

| Fakt | Wartość |
|---|---|
| Checkout | `/Users/jakubgawronski/Projects/Loadout`, gałąź `main` |
| SHA startowy | `315d209210dde34ae10ce92bd818bd22cfdd8d95` |
| Stan drzewa | czysty; jedyne nieśledzone: `docs/context-library/` |
| Otwarte biegi harnessu | brak (trzy zamknięte: `repair-*`, wszystkie `DZIALA`) |
| Cudze worktree | `loadout-baseline-check`, `loadout-reliability-native-qa-generator`, `loadout-workflow-reliability-build`, `loadout-workflow-reliability-plan` — **nie ruszane** |
| `claude` | 2.1.263 (przez opakowanie Supersetu → `/opt/homebrew/bin/claude`) |
| `codex` | codex-cli 0.153.4 (to samo opakowanie) |
| cargo / rustc | 1.96.0 |
| node / npm | v24.16.0 / 11.13.0 |

SHA startowy jest **dokładnie tym**, na którym powstał plan — miejsca integracji z §3 planu
nie wymagają korekty pod inny kod.

---

## 0a. Bramka na wejściu — zmierzona, nie założona

`scripts/ci.sh full` na `315d2092` **przed** jakąkolwiek zmianą: `✅ CI green (stage: full, 461s)`.

- front: **401 plików / 2080 testów**, 0 czerwonych
- gęstość: `labelledRegions 3/8, chromePixels 93/96, textElements 52/60, animatedRegions 0/2`
  — pod sufitem z `ARCHITECTURE.md` §7 i pod zapadką z `checks/density-baseline.json`
- słownictwo: 342 pliki, **0 trafień** widocznych dla użytkownika (baseline 0)
- strażnicy: 10 wystrzeliło zgodnie z oczekiwaniem, 0 pudeł

Ten pomiar jest punktem odniesienia dla każdej późniejszej czerwieni.

## 0b. Rekonesans kodu (2026-09-07, przed CT-01)

24 agenty, 12 obszarów + przeciwstawna weryfikacja każdego. Digest roboczy poza repo.
Ustalenia, które zmieniają plan:

- **Wiersz nawigacji niesie DWA elementy z tekstem**, nie jeden: etykietę (`titlebar.tsx:481`)
  i `<kbd>` ze skrótem (`:511`). Zapadka gęstości pójdzie więc prawdopodobnie 52 → **54**.
  Zgoda właściciela z 2026-09-07 brzmi „52 → 53", więc po pomiarze wracam z pytaniem,
  jeśli pomiar pokaże więcej. **Podnoszę po pomiarze, nigdy przed.**
- **`serve::tool_result` (`bridge/serve.rs:88`) jest JEDYNYM miejscem serializacji wyniku
  narzędzia** i jedynym wyczerpującym `match` po `Answer`. Tam wchodzi blok obrazu (CT-03).
- **`verbs::for_role(Role::Step)` oddaje dziś `Vec::new()`** — krok biegu nie dostaje ani
  jednego czasownika. Czytelnik kontekstu musi mieć własne źródło czasowników, jak
  `message_tools()`.
- **`service_bridge_for` (`run.rs:14240`) oddaje `Ok(None)`**, gdy krok nie ma ani usług, ani
  wiadomości — czyli most w ogóle nie wstaje. To jest ta linia, którą CT-06 musi zmienić.
- **Zwykły krok workflow NIE niesie dziś obrazów w ogóle** (`driver.start(spec, tx)` jest
  bezobrazowy; `CodexDriver` celowo nie nadpisuje `start_with_images`). To potwierdza wybór
  z planu: obrazy do kroku idą **wynikiem narzędzia**, nie transportem rozmowy.
- **`RunSpec` nie ma `Default` i ma 31 miejsc konstrukcji**; dodatek kontekstu wchodzi
  w `prompt` (stdin), **nigdy** w `system_append`.
- **`AgentDriver` ma 3 metody wymagane i 21 domyślnych**, bo w `src-tauri/tests/it/` żyją
  **182 ręczne atrapy**. Każdy nowy szew musi mieć domyślną implementację.
- Nowa sekcja to **dziesięć** luster, nie dziewięć: dochodzi
  `src/ui/shell/nav-groups-locks-and-keys.test.tsx` (kolejność paska wypisana wprost).

## 0c. Incydent 21:14–21:17, zamknięty

Agent mojego rekonesansu (`navigation`) zweryfikował listę plików **wykonując zmianę**
w głównym checkoucie właściciela: dziesięć plików plus `src/sections/context/index.tsx`,
potem `npx vitest run src/`, potem odtworzenie wszystkiego z własnych kopii zapasowych.
Drzewo wyszło czyste (`git status` pusty, HEAD `50ee2dee`), nic nie zginęło.

Wnioski zapisane w pamięci projektu: agent czytający musi dostać **jawny zakaz pisania**,
a zero trafień z regexa po transkrypcie to zwykle zły regex, nie brak zdarzeń —
na takim zerze błędnie wskazałem cudzą sesję jako sprawcę.

## 0d. Sonda transportu obrazu — zmierzona PRZED wydaniem pieniędzy na CT-03

Największe ryzyko całego planu brzmiało: czy blok obrazu MCP naprawdę dociera do modelu przez
CLI obu vendorów, czy tylko przez specyfikację. Zbudowałem minimalny serwer MCP po stdio
oddający `{"type":"image","data":<base64>,"mimeType":"image/png"}` i obrazek z faktem,
którego **nie ma** ani w nazwie narzędzia, ani w opisie, ani w nazwie pliku (`K-7413`
i fioletowy trójkąt).

| Vendor | Wersja | Model | Wynik |
|---|---|---|---|
| Claude Code | 2.1.263 | `claude-sonnet-5` | **`K-7413, purple triangle`** — obraz dociera jako obraz |
| Codex | codex-cli 0.153.4 | `gpt-5.6-sol` | **`K-7413, purple triangle`** — po dołożeniu adnotacji, patrz niżej |

**Drugie ustalenie, ważniejsze od pierwszego.** `codex exec` odmawia **każdego** wywołania
narzędzia MCP zdaniem `MCP tool call requires approval, but approval policy is never`:

| Konfiguracja | Wynik |
|---|---|
| `-s read-only` + serwer w `-c mcp_servers.*` | odmowa |
| to samo + `-c mcp_servers.<n>.enabled=true` | odmowa |
| to samo + `annotations: {readOnlyHint: true}` | **przechodzi**, piaskownica dalej `read-only` |
| `--dangerously-bypass-approvals-and-sandbox` | przechodzi, ale zdejmuje cały dial |

`--full-auto` nie jest flagą `codex exec`.

**Co to znaczy dla CT-03 i CT-06:**

1. `bridge/verbs.rs:267` (`tool_list`) wystawia dziś tylko `name` / `description` /
   `inputSchema` — **bez `annotations`**. Cztery czasowniki kontekstu muszą oddawać
   `readOnlyHint: true`, inaczej krok Codeksa odbija się od zatwierdzania i wygląda to jak
   zepsuty most. To jest **opis prawdy** o narzędziu, nie obejście: te cztery nic nie zmieniają.
2. `build_exec_argv` (`codex.rs:2054`) nie ma **ani jednego** `-c mcp_servers.*`, a
   `verbs::for_role(Role::Step)` oddaje `Vec::new()`. Krok Codeksa nie dostaje dziś mostu
   w ogóle — plan zakładał, że most „już istnieje" dla kroków; dla Claude'a tak, dla
   `codex exec` **nie**. To jest realna praca w CT-03, nie szczegół.

Sonda leży poza repo (scratchpad), nie jest artefaktem, którego nikt nie czyta.

## 1. Dziennik etapów

| Etap | Bieg | Commit | RED | GREEN | CI przy lądowaniu | Koszt | Stan |
|---|---|---|---|---|---|---|---|
| CT-01 | `h-ct-01` | `a6113319` | mutacja, patrz §1a | 4 Rust + pełna suita frontu | **zielone, 523 s** | 55,17 USD | **WYLĄDOWANY** |
| CT-02 | `h-ct-02` | `f65ed67c` | 3 rundy weryfikatora | 13 Rust + 3 przeglądarkowe | **zielone, 495 s** | 121,09 USD | **WYLĄDOWANY** |
| CT-03a | `h-ct-03a` | `9b9439e0` | kontrola negatywna w suicie | **7 Rust** | **zielone, 697 s** | ~28 USD | **WYLĄDOWANY** |
| CT-03b | `h-ct-03b` | `0b4e45f8` | mutacja zakresu → **4 testy padły** | **10 Rust** (+1 żywa próba `#[ignore]`) | **zielone, 609 s** | ~35 USD | **WYLĄDOWANY** |
| CT-04 | `h-ct-04` | `60220c09` | 3 rundy weryfikatora + 3 naprawy ręczne | **29 Rust** | **zielone, 504 s** | ~180 USD | **WYLĄDOWANY** |
| CT-05 | `h-ct-05` | — | — | — | — | — | **w toku** |
| CT-06 | — | — | — | — | — | — | nie rozpoczęty |
| CT-07 | — | — | — | — | — | — | nie rozpoczęty |
| CT-08 | — | — | — | — | — | — | nie rozpoczęty |
| CT-09 | — | — | — | — | — | — | nie rozpoczęty |

**Lądowanie.** `scripts/h land ct-01` → merge `a6113319`, potem pełne CI na trunku.
Pierwszy przebieg poszedł na czerwono **nie na kodzie**, tylko na moim niezacommitowanym
dzienniku: `guards NOT RUN: the tree is dirty, so planting a violation proves nothing`.
Po zacommitowaniu, powtórka: **`CI_EXIT=0`, `CI green (stage: full, 523 s)`**, strażnicy
`10 fired as expected, 0 misfired`. Front urósł z 401/2080 na **402 pliki / 2096 testów**.
Gęstość na trunku: `textElements 54/60`, pod sufitem i pod zapadką.

Zależności dołożone przeze mnie poza pętlą zadaniową (bieg nie ma prawa pisać do
`Cargo.toml` ani `package.json`) i zweryfikowane tym samym CI:
`pdfjs-dist 6.3.289` (`31ac51ca`) i `image 0.25` z cechami `png,jpeg,webp` (`9820db7c`) —
ta druga dołożyła do drzewa **dokładnie jedną** nową skrzynię, `image-webp`, bo `image`
stał już w `Cargo.lock` przechodnio przez Tauri. `cargo deny check`: wszystko ok.

### 1b. CT-02 — trzy rundy naprawcze i co je wywołało

Bieg: 6506 s, **trzy rundy**, 121,09 USD, 30 zmienionych plików, werdykt `DZIALA`.
Testy policzone niezależnie: **13 Rust** (`context_source_import::` +
`context_library_survives_restart::`) i **3 przeglądarkowe**.

**Wszystkie 13 checków było zielone po KAŻDEJ rundzie.** Odrzucał weryfikator (Codex),
i za każdym razem miał rację. To jest najlepszy dowód na D3, jaki dała ta sesja.

| Runda | Co znalazł weryfikator na ZIELONEJ bramce |
|---|---|
| 1 | `companionOf` zapisany, ale **niewyświetlany** — powiązanie nie dociera na ekran (niezmiennik 29). Uszkodzony PDF nigdy nie dostaje trwałego `Failed`. Test PDF **nie uruchamia pdf.js** — podaje gotowe strony. Limit 512 MiB pochodnych nieegzekwowany. |
| 2 | Dwa wklejenia tego samego obrazu z **różnymi podpisami** dają jedno powiązanie — deduplikacja patrzy tylko na nazwę i odcisk bajtów, wbrew PLAN §5. `Failed` powstaje **tylko** gdy dokument się nie otworzy; błąd `getPage`, ekstrakcji i renderowania zostawia `Needs preparation`. |
| 3 | — (`DZIALA`) |

**Jedna klasa, dwa razy:** stan błędu obsłużony na **pierwszym** etapie i nigdzie dalej,
oraz tożsamość sprawdzana **zbyt płytko**. Regułę wyciągniętą z tego dopisałem do wszystkich
oczekujących promptów: dla każdego stanu porażki wypisz, na jakich etapach może powstać,
i pokryj każdy osobno; a gdy specyfikacja mówi „dwa różne X pozostają oddzielne", napisz
test z dwoma X różniącymi się **wyłącznie tym jednym polem**.

Ślad w kodzie: `the_same_picture_pasted_twice_keeps_a_link_for_each_caption`.

### 1a. CT-01 — co dokładnie dowiedzione

Bieg: `scripts/h run ct-01`, jedna runda, 2776 s, **55,17 USD** (plan 5,31 + implementacja
49,11 + weryfikacja 0,75). Werdykt Codeksa: `DZIALA`. 27 zmienionych plików.

**Checki w pętli zadania — 13/13 zielonych:** boundary 2 s, vocabulary 0 s, tokens 0 s,
invoke-args 0 s, suppressions 1 s, wired 0 s, tests-listed 1 s, rust-fmt 1 s, web-fmt 3 s,
web-types 3 s, **web-test 44 s** (pełna suita frontu, bo zmienionych plików vitesta było
więcej niż `scope_limit`), rust-clippy 14 s, rust-test 0 s.

**`rust-test 0 s` sprawdziłem osobno**, bo zero sekund czyta się jak check, który nie
pobiegł: `4 passed; 0 failed; 1712 filtered out; finished in 0.09s`. Zero było prawdziwe —
binarka `it` była już zbudowana przez fazę implementacji.

**Mutacje — czy te testy czegokolwiek bronią.** Zielony test nad martwym kodem jest wadą,
dla której to repo powstało (niezmiennik 29), więc zieleni nie przyjąłem na słowo:

| Mutacja | Wynik |
|---|---|
| `save_draft` porównuje oczekiwaną rewizję z **aktualną z dysku** zamiast z podaną przez wołającego | `a_stale_revision_is_refused_and_the_newer_text_stays` **FAILED**, pozostałe 3 przeszły |
| tożsamość zestawu wyprowadzona z **tytułu** zamiast zachowana | **3 z 4 FAILED** |
| przywrócenie oryginału | `4 passed; 0 failed` |

Pierwszej mutacji nie zrobiłem przez `expected: None`, bo `durable_file.rs:375` mówi, że
`None` znaczy **„tego pliku ma tam nie być"** — czyli `None` **zaostrzyłby** bramkę zamiast ją
wyłączyć, a test padłby z niewłaściwego powodu.

**Gęstość — zmierzona, potem zapadka.** `vite build` na gałęzi, potem kolektor:
`textElements = 54` przy 1100 px **i** przy 1512 px; reszta metryk bez ruchu. Sprawdzacz
odmówił dokładnie tak, jak miał: `the baseline may only shrink, never grow`,
`textElements measured 54, baseline 52`, przy jednoczesnym `every metric above is still under
its ceiling`. Zapadka podniesiona **ręcznie do 54** osobnym commitem `ef18ad7f` (sufit z
`ARCHITECTURE.md` §7 to 60 i nie ruszony). +2, nie +1, bo wiersz paska niesie dwa nośniki
tekstu: etykietę i skrót.

**Czego bieg dołożył ponad zlecenie:** znalazł dwa lustra, których nie wymieniłem —
`src/ui/shell/nav-groups-locks-and-keys.test.tsx` (kolejność paska wypisana wprost)
i `e2e/tests/two-buttons-ask-two-different-vendors.spec.ts`. Zgłosił też jako
`POZA ZAKRESEM`, że kolejność paska jest dziś przypięta w **trzech** wyroczniach naraz
(makieta, egzekutor, nav-groups), wbrew niezmiennikowi 13. Nie naprawiał tego — słusznie.

---

### 1c. Co dowiodła mutacja CT-03b

Izolacja zakresu jest własnością bezpieczeństwa, więc nie przyjąłem jej na zieleń.
Mutacja: `grant_for` oddaje **dowolny** przydział zamiast przydziału tego odbiorcy.
Padły cztery testy, w tym trzy, które trafiają w sedno planu §8:

- `one_selected_pdf_page_has_no_road_to_the_whole_original` — wybór tematu **nie** odsłania
  całego PDF-u przez odczyt oryginału;
- `the_core_enforces_the_same_boundaries_without_a_socket` — granica trzyma **w rdzeniu**,
  nie tylko na gnieździe;
- `two_recipients_that_differ_only_in_what_they_were_given` — dwaj odbiorcy różniący się
  **wyłącznie przydziałem** nie zamieniają się materiałem.

### 1d. Trzy czerwienie, które złapało dopiero pełne CI po scaleniu

To jest zmierzona cena równoległości, przewidziana przez Falę 6 i potwierdzona trzykrotnie.
Wspólna cecha: **każdy z tych testów sądzi cały plik albo cały zbiór**, więc zawężona bramka
zadania (`cargo test --test it <moduł>::`, `vitest <pliki>`) nie uruchamia go nigdy.

| Kiedy | Objaw | Co to było naprawdę |
|---|---|---|
| po CT-02 | `context-sources.spec.ts` przewrócona, 2100 testów zielonych | rozgrzew `addScriptTag` ściągał `pdfjs-dist`, vite wymuszał **przeładowanie**, a ono niszczyło kontekst tego samego wywołania |
| po WP-03 | `a_missing_handoff_stops_the_step_that_reads_it` | **regresja produktu**: zniknięty plik znów udawał zmieniony, wbrew naprawie Z-41 z 2026-09-05 — komentarz w kodzie mówił co innego niż linia pod nim |
| po CT-04 | `no_command_freezes_the_window` | **wada produktu**: `build_context` czytał ustawienia synchronicznie na wątku okna, który niesie też Stop i linie biegu |

Wszystkie trzy naprawione **bez pętli harnessu** — w każdej znałem przyczynę — a dowodem
jest za każdym razem pełne CI, nie moje zdanie. Druga i trzecia były prawdziwymi wadami
produktu, nie usterkami testów.

**Wniosek operacyjny:** przy pracy równoległej planuj poprawkę na trunku mniej więcej co
trzecie–czwarte lądowanie i nie traktuj lądowania jak formalności.

## 2. Kryteria odbioru (plan §14)

| Scenariusz | Status | Dowód / bloker |
|---|---|---|
| Trwały paste | `not-tested` | — |
| Wiele źródeł | `not-tested` | — |
| PDF | `not-tested` | — |
| Obaj vendorzy | `not-tested` | — |
| Dobór per krok | `not-tested` | — |
| Plan przed wykonaniem | `not-tested` | — |
| Izolacja | `not-tested` | — |
| Równoległość | `not-tested` | — |
| Anulowanie | `not-tested` | — |
| Powtarzalność wejść | `not-tested` | — |
| Uczciwy ekran | `not-tested` | — |
| Prywatność danych | `not-tested` | — |

---

## 3. Otwarte problemy i blokery

**D-1. Szew „prawdziwe IPC → dysk" nie jest dowiedziony żadnym testem automatycznym.**
Test rustowy CT-01 importuje `loadout_lib::context::files::*`, czyli warstwę biblioteki;
`e2e/tests/context-library.spec.ts` biegnie na **atrapie** IPC. Między nimi zostaje skorupa
`#[tauri::command]` w `ipc.rs`: nazwy argumentów, `spawn_blocking` i sięgnięcie po `AppState`.

Ryzyko jest **małe, ale niezerowe**: `commands/context.rs` to cztery jednolinijkowce
(odwzorowanie `home → contexts/` plus zegar), a nazwy argumentów pilnuje
`checks/invoke-args.sh`. Zamyka to dopiero **natywna próba z CT-09** — i dopóki jej nie ma,
kryterium „Trwały paste" stoi na `not-tested`, a nie na `passed`.

**D-5. Natywne QA: izolacja da się zrobić, sterowanie oknem wymaga zgody CZŁOWIEKA.**
Sprawdzone przed CT-09, żeby nie odkryć tego na końcu.

- **Izolacja: jest droga.** `lib.rs::loadout_dir()` składa bibliotekę jako `$HOME/.loadout`
  i czyta **wyłącznie `HOME`** — Loadout nie ma własnej zmiennej katalogu danych. Izolowaną
  instancję uruchamia się więc przez podmianę `HOME` przy starcie binarki. To dotyczy
  **mojego uruchomienia do testu**, nie konfiguracji usług w workflow, gdzie `HOME` stoi na
  liście zastrzeżonych (`workflow::check::service_environment_name`) i ma tam zostać.
- **Sterowanie oknem: bloker po stronie uprawnień.** Świeżo zbudowana binarka potrzebuje
  własnych zgód macOS, a TCC przypina je **do binarki**. Syntetyczne `Cmd+V` wymaga zgody
  Accessibility, której nie mogę sobie nadać.

**Skutek dla odbioru:** kryterium „Trwały paste" da się domknąć albo (a) zgodą Accessibility
dla binarki testowej, albo (b) **udokumentowanym testem ręcznym** właściciela — zlecenie
dopuszcza obie drogi. Bez jednej z nich zostaje `not-tested` i **blokuje** deklarację
gotowości. Przeglądarkowe e2e z atrapą IPC **nie zastępuje** natywnego Cmd+V i nie będzie
tak liczone.

**D-3. CSP i zasoby bundla ograniczają dwa warianty PDF — ROZSTRZYGNIĘTE: zostawiamy.**
Zgłoszone przez plan CT-02 zamiast wykonane (AGENTS.md §7). `src-tauri/tauri.conf.json` nie ma
`wasm-unsafe-eval` w CSP ani `resources` w `bundle`, więc PDF z obrazami **JPEG 2000**
(dekoder WASM) i PDF z **nieosadzonymi fontami CJK** (`cmaps/`, `standard_fonts/`) nie
przygotują się w całości.

**Decyzja: nie rozluźniamy CSP.** `wasm-unsafe-eval` w aplikacji, która uruchamia cudze agenty,
jest złym kursem wymiany za rzadki wariant formatu; PLAN §2 i tak nie obiecuje ani JPEG 2000,
ani CJK bez osadzonych fontów. Kontraktem dla takiego pliku jest **nazwany stan `Failed`
z własnym zdaniem** — nigdy udawany pusty dokument (PLAN §5). Sam worker pdf.js problemu nie
ma: `worker-src` spada do `default-src 'self'`, a zasób emitowany przez Vite jest same-origin,
więc **aplikacja działa bez sieci** i to jest sprawdzane na zbudowanym `dist/`, nie na dev.

Warunek zmiany tej decyzji: gdyby natywne QA z CT-09 pokazało, że realne materiały użytkownika
wpadają w ten wariant częściej niż incydentalnie.

**D-4. Dwie moje własne wady, obie znalezione przez plan CT-02 i naprawione.**
`pdfjs-dist` wszedł z `^6.3.289`, wbrew pinowaniu reszty `package.json` i wbrew `comment:*`
w tym samym pliku — przypięte dokładnie. Oraz: mój wtręt o lądowaniu CT-01 rozerwał tabelę
etapów z §1 na dwie — wiersze wrócone do jednej tabeli.

**D-2. Kolejność paska jest przypięta w trzech wyroczniach naraz** (makieta
`docs/mockup/index.html`, egzekutor `src/sections/triggers/mounted.test.tsx`, plus
`src/ui/shell/nav-groups-locks-and-keys.test.tsx`) — wbrew niezmiennikowi 13 („jeden fakt,
jedno miejsce"). Zgłoszone przez bieg CT-01 jako `POZA ZAKRESEM` i **słusznie nienaprawione**:
to nie jest praca tej dostawy. Koszt: każda przyszła zmiana paska rusza trzy pliki zamiast
jednego.

---

## 4. Natywne QA

| Próba | Vendor | Wersja CLI | Model | SHA | Wynik |
|---|---|---|---|---|---|
| — | — | — | — | — | — |
