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
| CT-05 | `h-ct-05` | — | `todo!()` uruchomione, 3 rundy weryfikatora | **17 Rust + 120 frontu** | — | — | naprawiony ręcznie, ląduje |
| CT-06 | `h-ct-06` | — | kontrola starego warunku mostu, patrz §1e | 12 zielonych (4+5+3); dwie mutacje po 4/4 | clippy `too_many_lines` + dwa defekty fikstury, naprawione ręcznie po STOP | — | zielony lokalnie, czeka na pełne CI przy lądowaniu |
| CT-07 | — | — | — | — | — | — | nie rozpoczęty |
| CT-08 | `h-ct-08` | — | izolowany stary `7992cadd`: 1/1 pada na braku replay Context, patrz §1f | 8 replay + 2 historii + 4 raportu + 126 frontu | — | — | implementacja lokalna; wymagane świadki zielone |
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

### 1e. CT-06 — prywatny pakiet biegu i naprawa po czerwonej bramce

Stan wejściowy worktree `loadout-h-ct-06` na `0286cb46` zawierał już niezatwierdzony szkic
implementacji i trzech nowych modułów testowych, bez zapisu wcześniejszego RED. Żeby nie
przedstawiać tego jako dowodu, kontrola negatywna przywróciła na chwilę dokładną starą semantykę
`service_bridge_for`: brak usług, wiadomości i planu wyłączał most także wtedy, gdy krok miał
Context. `different_parallel_steps_receive_only_their_selected_material` uruchomił się i padł
na zachowaniu: dwa kontrolowane kroki zgłosiły `the context-only step did not receive a bridge`.
Po pomiarze warunek został przywrócony do wersji uwzględniającej przydział Context.

Zaimplementowane: jeden resolver wejść per fizyczny `node_key`, prywatny pakiet z odciskami,
strumieniowym kopiowaniem i sprawdzeniem miejsca, dokładny zakres stron PDF, dodatek do finalnego
promptu przez stdin, most uruchamiany przez sam Context, osobne adresy i rachunki kopii/rund,
łączny zapis udanych odczytów oraz zdania w istniejącym panelu `What this step knew`. Test
`a_step_with_only_context_reads_its_own_set_through_the_bridge` używa prawdziwego
`ClaudeDriver`, kontrolowanego procesu CLI i produkcyjnej binarki mostu MCP; pozostałe scenariusze
Startu mierzą różne przydziały, nakładanie w czasie, `copies`, pętlę, fan-in, zamrożenie po
edycji/usunięciu biblioteki, obcy zakres i brak globalnego `extra_dirs`.

Pierwsza pełna bramka po implementacji przeszła 26 z 29 zawężonych testów. Nie była to porażka
granic systemowych: w tym samym przebiegu inne scenariusze z gniazdem i chronionym krokiem były
zielone. Dwie czerwienie kończyły się dopiero na historii `Opened`, a trzecia na odczycie finalnego
promptu za prawdziwym `ClaudeDriver`.

Naprawa rachunku przestała odzyskiwać tożsamość odczytanej pozycji z tekstowej ścieżki dowodu.
Prywatny manifest wiąże teraz rekord `Available` bezpośrednio z logicznym `Address`, waliduje ten
adres względem przydziału fizycznego `node_key` i po nim sumuje bajty skutecznych odpowiedzi.
Ścieżka względna `context-sources/...` pozostaje osobnym adresem dowodu dla
`SafeInputManifest`. Dzięki temu kopie i rundy nadal mają osobne pliki odczytów, a promocja do
`Opened` nie zależy od ponownego zgadywania tożsamości z nazwy pliku.

Trzecia czerwień była błędem świadka: atrapa Claude'a zapisywała stdin operatorem `>`, więc
późniejsza prywatna refleksja nadpisywała prompt właściwego kroku. Test zapisuje teraz wszystkie
fizyczne uruchomienia, wybiera kopertę kroku po jego markerze i wybiera manifest wejścia po
`referenceMaterial`, nie po przypadkowej kolejności katalogu. Na tej kopercie wymagania różniące
się wyłącznie warunkiem pozostają dwoma wpisami, stoją przed indeksem, zachowują dokładne brzmienie
i nie trafiają do argv.

Zarządzana piaskownica używana do ręcznej naprawy nadal odrzuca lokalny `UnixListener::bind`,
więc nie jest podstawą statusu etapu. Autorytatywny stan to poprzednia czerwona bramka; CT-06
pozostaje w naprawie do jej ponownego, zielonego przebiegu. Lokalnie zielone są obie odmowy
budżetowe, formatowanie oraz kontrola finalnego promptu wykonana z tymczasowo wyłączonym tylko
transportem gniazda; wyjątek diagnostyczny nie pozostał w kodzie.

#### Zamknięcie po zatrzymaniu biegu (2026-09-08, ręcznie)

Bieg stanął po trzech rundach na **`clippy::too_many_lines`** (102/100 w `prepare()`). Pod
`-D warnings` to błąd kompilacji `lib` i `lib test`, więc `rust-test` poleciał jako POMINIĘTY
i **ani jeden** z dwunastu napisanych testów nie wykonał się w tej rundzie — stąd werdykt
„żaden punkt akceptacji nie ma dowodu wykonania". To **czwarte** zatrzymanie tej samej klasy
(po CT-04, CT-05 i WP-02) i pierwsze na `too_many_lines`, a nie na braku `allow` w module.

Naprawa lintu: ciało pętli `for set in resolved` wyszło z `prepare()` do
`account_for_set()`. Adnotacja `#[allow]` nie wchodziła w grę — `checks/suppressions.sh`
gerpuje ten wzorzec po całym `src-tauri/src`, więc zamieniłaby czerwień clippy na czerwień
suppressions. W `context_budget_preserves_requirements.rs` (103 linie w jednym teście)
`#![allow(clippy::too_many_lines)]` **jest** w porządku i ma zapisany powód: bramka
suppressions nie skanuje `tests/`, a dzielenie tego testu rozerwałoby jedną narrację dowodu
na dwie połówki, z których żadna nie dowodzi kryterium.

Po zieleni clippy wyszły **dwa defekty fikstury**, oba niespełnialne niezależnie od produktu:

1. `greeting["tools"]` — pomocnik `call()` oddaje **już samą tablicę** `greeting.tools`,
   więc indeks po kluczu na tablicy dawał `Null` i lista narzędzi wychodziła pusta zawsze.
   Most naprawdę wystawia cztery czytelniki.
2. `"what the step before this one left"` — zdanie, którego produkt **nigdy nie emituje**:
   zlepek nagłówka indeksu („Steps before this one left what they found in these files:")
   i znacznika przy pozycji („(what the step before left)"). Fan-in działał przez cały czas.
   Asercja stoi teraz na **wskaźniku** `handoffs/`, tak jak wyrocznia tego zachowania
   (`handoff_index_for_fan_in.rs`), bo proza ma jedno miejsce zamieszkania i wolno jej się
   zmienić, a wskaźnik jest kontraktem dla obu rodziców.

**Wynik: 12 testów zielonych** — `step_receives_selected_context` 4,
`context_does_not_change_during_run` 5, `context_budget_preserves_requirements` 3.

**Dowód mutacyjny**, żeby zieleń po naprawie fikstury nie była pusta:

| Mutacja w kodzie produktu | Skutek |
|---|---|
| `StepDesk::tools()` przestaje dokładać `context.tools()` | **4/4 padają** |
| `service_bridge_for` znów oddaje `Ok(None)` bez usług, wiadomości i planu | **4/4 padają** |

Druga mutacja przywraca dokładnie to założenie, które ten etap miał zdjąć. Pierwsza pokazuje
przy okazji, że lista i rozdzielnik są **jednym** uprawnieniem: zdjęcie nazw z listy zabija
też odczyty, bo `bridge::host::talk` przepuszcza wyłącznie czasowniki z powitania.

### 1f. CT-07 — Lead planuje z kontekstem, a wybór rozmowy dojeżdża do Startu

Bieg zatrzymał się po trzech rundach na czerwonym `web-test`. Skutek kaskadowy jest tu ważniejszy
niż sama czerwień: `rust-clippy` i `rust-test` poszły jako POMINIĘTE, więc moduł
`lead_context_reaches_run.rs` **nie uruchomił się ani razu przez całe trzy rundy** — pięć punktów
akceptacji nie miało dowodu wykonania, choć testy były napisane. To **piąte** zatrzymanie tej
klasy.

Domknięte ręcznie. Trzy wady produktu i jeden defekt fikstury:

1. **`invoke<ChatPinsView>` to rzutowanie, nie sprawdzenie.** `T` znika przy kompilacji, więc
   `null` z drutu wjeżdżał do stanu Reacta jako „wybór, o którym nic nie wiadomo": picker mówił
   w kółko `Context choices are being read.` i **milczał o starych wiadomościach**. Ekran wyglądał
   na wczytujący się zamiast powiedzieć, że nie umie odczytać.
2. **`Keep previous selection and start` nie miał czego zachować.** Odmowa Startu przenosi żądanie
   pod kartę biegu (`forget` + `remember`), więc podgląd się **przemontowuje**, a wybór
   dostarczenia żył wyłącznie w `useState` i wracał do domyślnego „cały workflow". Człowiek klikał
   „zachowaj poprzedni wybór" i wysyłał **inny zakres, niż widział** — dokładnie ta cicha podmiana,
   której zabrania kryterium 3.
3. **Nakładka `run_only` schodziła na zestaw, ale nie na jego pozycje.** Rachunek dostarczenia miał
   `runOnly: true`, a pakiet biegu brał `item.run_only` sprzed nakładki, czyli zawsze `false`.
   Zapis biegu twierdził, że materiał „tylko dla tego uruchomienia" jest zwykłym materiałem
   workflow — różnica, której pilnuje kryterium 4.
4. **Kolejka atrapy `what_this_chat_pinned` miała cztery wpisy przy pięciu odczytach.** Liczba
   zmierzona sondą, nie zgadnięta. Doszła asercja, że panel **nie zawiera** zdania odmowy: bez niej
   poprzednie asercje przechodziły **mimo** odmowy, bo szukały swojego zdania w tekście niosącym
   oba naraz.

Cztery mutacje, każda przewraca dokładnie swój test. Walidacja kształtu dostała **własną**
specyfikację (`chat-context-answer-is-checked.test.ts`), bo jako jedyna nie miała wyroczni:
po jej usunięciu ekran wracał do „being read" i wszystkie pozostałe testy zostawały zielone.

**Wynik: 9 testów rustowych, 124 frontowe.**

#### Scalenie z WP-04b: dwie funkcje o tej samej nazwie

Obie gałęzie nazwały swoją funkcję `compose`, a znaczą co innego — CT-07 **rozwiązuje** przypięcia
rozmowy w pakiet biegu, WP-04b **składa** prompt z Planu, kontekstu i indeksu przekazań. Git
zgłosił jeden mały konflikt na linii wołania i „scalił" resztę w kod, który nie ma prawa się
skompilować; kolizję pokazał dopiero kompilator. Nazwy rozdzielone: `prepare_pins` i `compose`.

Przy okazji: `chat.rs` wklejał blok kontekstu **wprost przed prompt**, omijając wspólny przydział
— czyli tę samą wadę, którą WP-04b naprawił dla kroków. Rozmowa Leada idzie teraz przez ten sam
kompozytor.

#### Gęstość: 57 → 55, zapadka 54 → 55 za zgodą właściciela

Pomiar po CT-07 dał 57 przy zapadce 54. **Dwa z trzech nowych elementów okazały się wadami**, nie
gęstością: sprzeczne zdania obok siebie oraz odmowa niewidoczna w zwiniętym bloku. Trzecie
odkrycie było ogólniejsze — **zwinięty `<details>` trzyma dzieci w drzewie i kolektor je liczy**,
więc bez leniwego renderowania całe pole szukania, katalog i każdy zestaw wchodziłyby do pomiaru
widoku domyślnego i rosłyby z każdym zestawem w bibliotece.

Po naprawach zostało **+1**: uchwyt `Context · None` przy polu rozmowy, czyli sedno etapu. Zapadka
podniesiona osobnym commitem, po pomiarze, za zgodą właściciela — tak samo jak 52 → 54.
### 1g. CT-08 — startowalny pakiet, historia, Lab i retencja

Stan wejściowy worktree zawierał niezatwierdzony szkic CT-08. RED został więc wykonany na
izolowanym eksporcie dokładnego `HEAD` `7992caddbd5a13641abf774d9118317a6d2b5602`, bez zmiany
gita: test skompilował się, uruchomił i padł 1/1 dopiero wtedy, gdy Recorded po skasowaniu całej
biblioteki próbował rozwiązać przypiętą wersję z dzisiejszych plików. Odmowa nazywała brakujący
zestaw i kończyła się `Restore it or choose another ready version.` — czyli czerwień dotyczyła
braku zachowania, nie importu ani kompilacji.

Pakiet Context przechowuje teraz dokładne bloki promptu, kopiuje tylko pliki związane odciskami
i jest ponownie sprawdzany przy podglądzie, zatwierdzeniu oraz kopiowaniu. Ten sam snapshot zasila
pełne i częściowe Recorded, fizyczne kroki oraz komórki Labu. Wspólny zamek czytelnika żyje przez
podgląd/kopiowanie, a sprzątanie trzyma zamek wyłączny od ostatniego sprawdzenia do
`remove_dir_all`, więc nie zostaje okno TOCTOU. Archive zmienia wyłącznie półkę; Delete wymienia
workflow, zachowuje kopie biegów i odmawia przy aktywnym budowaniu. Historia pomija odczyty
zerobajtowe, a raport wsparcia zapisuje tylko trzy liczby materiałów kroku.

Lokalnie zielone: `recorded_replay_uses_frozen_context` **8/8**, historia dostarczenia **2/2**,
`support_report_excludes_private_content` **4/4**, odmowa w strumieniu **3/3** i wiring
**123/123**. `cargo fmt --all --check` oraz `cargo clippy --lib --tests -- -D warnings` są zielone.
Pierwszy świadek obrazu był czerwony, bo zarządzana piaskownica odrzucała `UnixListener::bind`
przed startem sterownika. 2026-09-08 (CT-08): świadek przechodzi teraz bez transportu przez ten
sam produkcyjny `ContextDesk` i `Recorder`, których bieg używa za mostem. Asertuje rzeczywistą
odpowiedź `Answer::Image`, jej liczbę bajtów, widoczny w historii wiersz `was opened (N bytes
returned)` oraz licznik raportu `[1, 1, 1]`; filtrowany moduł kończy się **2/2**.

#### Luka wyroczni znaleziona mutacją PO zielonym biegu

Kryterium 2 („uszkodzony albo podmieniony pakiet daje **odmowę**, nie pusty kontekst") było
zielone, a jego **centralny strażnik nie miał pokrycia**. Dwie mutacje przechodziły całą suitę:
zamiana `Err` na `Ok(None)` przy niezgodności wiązania z `run.json` oraz to samo przy porównaniu
w oknie między odczytem a blokadą.

Przyczyna: istniejący test psuje **bajty** pliku pakietu i przestawia dwa pliki miejscami, więc
obie te drogi wpadają w kontrolę odcisków wewnątrz `read_package`. Pakiet **wewnętrznie spójny,
ale należący do innego biegu** nie miał ani jednego świadka — a to jest mocne znaczenie słowa
„podmieniony": nie uszkodzony, tylko cudzy.

Dopisany `a_whole_foreign_package_refuses_even_though_every_digest_matches` zmienia **wyłącznie
identyfikator manifestu**; wszystkie odciski plików zostają zgodne, więc jedyną kontrolą zdolną to
złapać jest porównanie z zapisem biegu. Test ma własną przesłankę — nietknięty pakiet **musi** dać
podgląd — bo bez niej przechodziłby także dla funkcji odmawiającej zawsze. Mutacja powtórzona po
dopisaniu świadka przewraca dokładnie ten jeden test.

**Druga luka zostaje, opisana w kodzie.** To wyścig między dwoma odczytami w jednej synchronicznej
funkcji: test nie ma jak wejść pomiędzy nie bez szwu wstawionego wyłącznie dla niego. Sąsiednie
okno, między podglądem a potwierdzeniem, **jest** pokryte
(`package_changes_after_preview_or_confirmation_refuse_before_a_process`).

**Wniosek na CT-09:** zielone kryterium nie jest dowodem, dopóki nie wiadomo, **którą** ścieżką
biegnie jego test. Trzy z sześciu kryteriów tego etapu mają po jednym teście — te warto sprawdzić
mutacją, zanim wpiszę im `passed`.

### 1h. Natywne QA: pięć wad, których nie widziały żadne testy

Właściciel przeszedł ścieżkę w **prawdziwym oknie** na izolowanym `HOME`. Bramka była wtedy
zielona na 412 plikach testowych. Wyszło pięć rzeczy — **cztery poprawione, jedna okazała się
poprawnym zachowaniem**.

| Co | Rozstrzygnięcie |
|---|---|
| `Save` udawał się w milczeniu (`draftRevision` doszedł do 8, zero odmów) | naprawione: zdanie mówi, **co jest następne**, nie „Saved" |
| Katalog projektu, który aplikacja wybrała sama, **nie istniał** | naprawione: `project_dir_ready` zakłada go na starcie |
| `No such file or directory (os error 2)` jako zdanie dla człowieka | naprawione: sprawdzenie tam, gdzie znane są **nazwy**; granica dalej odmawia |
| `Rebuild context` po **nieudanej** pierwszej próbie | naprawione: trzy stany, trzy zdania (`Build` / `Try building again` / `Rebuild`) |
| Zestaw bez gotowej wersji **nie da się zaznaczyć** w panelu kroku | **poprawne zachowanie** — i ekran mówi dlaczego |

Dwie rzeczy z tej listy zasługują na osobne zdanie.

**Pliki są prawdą, baza jest indeksem — dowiedzione na żywej instancji, nie w teście.**
Korzystając z okazji usunąłem `loadout.db` przy zamkniętej aplikacji. Wstała, **odtworzyła
indeks z plików**, a zestaw i wklejony obraz były na miejscu, bajt w bajt (SHA-256 przed
i po zamknięciu identyczny). To niezmiennik 4 sprawdzony na produkcie.

**Wklejony obraz to zrzut ekranu właściciela i nie oglądałem go ani razu** — tożsamość
sprawdzona po odcisku i wymiarach (2588×1458 RGBA), pochodne czytelnika powstały
(`for-the-agent.png` 1568×883, `thumbnail.png`). Kryterium §14 mówi wprost „wklejenie tekstu
i **screenshotu**", więc prawdziwy zrzut jest tu właściwym materiałem, lepszym od mojego
obrazu kontrolnego.

### 1i. Trzy uwagi właściciela o UI, wszystkie wykonane

| Uwaga | Co zrobione |
|---|---|
| „kontekst jest w chuj schowany w opcjach a powinien być na wierzchu, tak samo plan" | **UX-1**: piątka zostaje piątką — Context i Plan weszły, `Ile naraz` i `Gdy się nie uda` zeszły. Wyrocznia pięciu pól dostała **nową listę, nie zniesienie** |
| „mega chujowe checkboxy, trzeba je przepisać i podmienić wszędzie" | **UX-2**: jedna kontrolka w `src/ui/primitives/tick.tsx`; **jeden** `type="checkbox"` w całym kodzie produktu, asercji na roli z 14 do 19 |
| „UX tworzenia kontekstu jest jakiś zjebany, uprość go" | **UX-3**: trzy stany przycisku, placeholder modelu zależny od vendora, jedno zdanie prowadzące od materiału do gotowej wersji |

Osobno, z tego samego zgłoszenia: **`Use` dziedziczy się po krokach potomnych** (WP-08),
materializowane do pliku, żeby build nieznający dziedziczenia nie wykonał go po cichu jako
`Off`. Ręczne `Off` jest chronione — „nie ustawione" i „ustawione na `Off`" to **dwa różne
stany**.

**Regresja złapana dopiero pełnym CI po scaleniu UX-2:** `position: relative` na nowej
kontrolce sprawiało, że próbne kliknięcie Playwrighta przewijało listę importu o 41 px, mimo
że pole było w całości widoczne. Znalezione bisekcją; ptaszek jest teraz elementem siatki bez
żadnego `position`. Komentarz pierwszego podejścia obwiniał `inline-grid` — mylnie, bo stało
ono tam **razem** z `position: relative`.

## 2. Kryteria odbioru (plan §14)

Status jest **per ścieżka dowodu**, nie per wrażenie. `passed` znaczy: istnieje test na
produkcyjnym szwie, uruchomiłem go i widziałem licznik przejść. `not-tested` znaczy, że
kryterium ma pokrycie **częściowe albo żadne** — i wtedy kolumna mówi dokładnie, czego brakuje.

| Scenariusz | Status | Dowód / czego brakuje |
|---|---|---|
| Trwały paste | `not-tested` | Deterministycznie pokryte: `context_source_import::a_mixed_paste_keeps_the_text_the_image_and_what_joins_them`, `::the_same_picture_pasted_twice_keeps_a_link_for_each_caption`, `context_library_survives_restart` (4/4). **Brakuje wyłącznie prawdziwego schowka macOS** — patrz D-5. |
| Wiele źródeł | `not-tested` | Kontrakt pokryty na **pięciu** plikach (`five_files_give_five_named_results_and_one_refusal_keeps_the_rest`). Skala **≥50 mieszanych z dużym tekstem** nie ma świadka; fikstura 56 źródeł czeka w `scratchpad/ct09/sources`. |
| PDF | `passed` | `context_source_import` (10/10), `context_reader_is_scoped::one_selected_pdf_page_has_no_road_to_the_whole_original`, e2e: wszystkie strony, dokument padający na drugiej stronie, wznowienie od brakującej strony. Fikstura `three-pages.pdf` jest w całości ASCII i recenzowalna okiem. |
| Obaj vendorzy | `passed` | **Żywe CLI, 2026-09-08:** `context_image_reaches_vendor::both_clis_name_the_detail_that_exists_only_in_pixels`. `claude 2.1.263` i `codex-cli 0.153.4` **same wywołały** `view_context_image` i oddały „Q-6284, teal, hexagon" — kolor i kształt nie stoją ani w nazwie pliku, ani w podpisie. 29 s. |
| Dobór per krok | `passed` | `step_receives_selected_context` (4/4); asercje na **bajtach w kopercie stdin** za prawdziwym `ClaudeDriver`, nie na wyniku resolvera. |
| Plan przed wykonaniem | `passed` | `lead_context_reaches_run` (9/9), `work_plan_graph_is_unambiguous` (9/9), `work_plan_review_shares_the_version` (8/8) — w tym dwa lustrzane testy tożsamości planu i pracy. |
| Izolacja | `passed` | `context_does_not_change_during_run` (5/5) i `context_reader_is_scoped` (6/6) — sfałszowany, obcy, wygasły i poza zakresem to **cztery różne zdania**, nie jedno. |
| Równoległość | `passed` | `step_receives_selected_context::different_parallel_steps_receive_only_their_selected_material` — dowód przez **nakładanie się w czasie**, nie przez liczbę kroków (niezmiennik 11). |
| Anulowanie | `passed` | Analiza i publikacja: `stop_returns_only_after_the_real_driver_group_is_dead`, `a_saved_running_build_is_interrupted_after_restart`, `a_measured_spending_limit_stops_the_real_codex_group`. **Oczekiwanie na slot: świadek dopisany w CT-09** — `stop_while_waiting_for_a_slot_never_starts_a_vendor` asertuje brak pliku `context.pid`, czyli że proces vendora **nigdy nie powstał**, a nie tylko że bieg skończył się jako `Cancelled`. Import **nie ma czego przerywać**: `import_context_sources_inner` jest synchroniczny i kończy się warunkowym zapisem szkicu, więc gwarancją tej ścieżki jest brak spóźnionego zapisu — `a_late_save_does_not_undo_newer_bytes` (2/2) i `context_library_survives_restart::a_stale_revision_is_refused_and_the_newer_text_stays`. Przerwane **przygotowanie** dokumentu ma swojego świadka w `context-sources.spec.ts` (7/7). |
| Powtarzalność wejść | `passed` | `recorded_replay_uses_frozen_context` (9/9) i `work_plan_survives_replay` (5/5), w tym dwaj świadkowie dopisani po mutacji: cudzy pakiet przy zgodnych odciskach i pokwitowanie sprzeczne z wersją. |
| Uczciwy ekran | `not-tested` | Siedem stanów jest osiągalnych w e2e, ale **przez atrapę IPC**, nie po prawdziwej akcji. §14 wymaga „osiągalne po prawdziwej akcji", więc przeglądarka tego nie zamyka. |
| Prywatność danych | `passed` | `support_report_excludes_private_content` (4/4) z **zamkniętą** listą 43 kluczy plus sentinele `PRIVATE_*`, oraz `context_history_reports_delivery::available_stays_available_and_diagnostics_keep_only_counts`. Mutacja dokładająca pole `String` do raportu przewraca test. |

**Bilans: 9 z 12 `passed`, 3 `not-tested`.** Trzy niezamknięte to dokładnie to, czego nie da się
dowieść bez prawdziwego okna albo bez skali: **paste**, **50+ źródeł** i **siedem stanów ekranu
po prawdziwej akcji**. Każde z nich blokuje deklarację gotowości i żadnego nie przepiszę na
`passed` bez próby.

**Anulowanie przeszło z `not-tested` na `passed` w CT-09**, i to nie przez rozluźnienie
kryterium: doszedł świadek dla oczekiwania na slot, a dla importu wykazałem, że nie ma tam
długiej operacji do przerwania — jest jedna synchroniczna z warunkowym zapisem, więc pilnuje jej
brak spóźnionego zapisu, a nie Stop. Mutacja zdejmująca ramię anulowania z `tokio::select!`
w `prepare_turn` przewraca nowy test **nazwanym zdaniem** o tym, że anulowanie nie ściga się ze
slotem — pierwsza wersja tego testu zabijała tę mutację przez zawieszenie, czyli sygnał, po
którym nikt nie wie, co się stało.

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

**ROZSTRZYGNIĘTE 2026-09-08: droga (b).** Właściciel wykonuje **jedno ręczne wklejenie**
obrazu do biblioteki przy CT-09; ja zapisuję próbę w §4 razem z wersją aplikacji, SHA
i tym, co widać na ekranie. Zgoda Accessibility dla binarki testowej **nie będzie
proszona** — jeden gest człowieka jest tańszy niż uprawnienie systemowe przypięte do
binarki, która i tak zmienia hasz przy każdej przebudowie.

**D-6. `context_ready_to_start` nie ma już wołającego w produkcie — ROZSTRZYGNIĘTE przy merge'u.**
CT-06 zastąpił przedstartową bramkę z CT-05 jednym resolverem (`freeze_context_inputs`)
i **celowo** zmienił znaczenie limitu 24 KiB: ogranicza on dodatek do promptu, a nie prywatny
pakiet, bo agent ma zawsze móc doczytać pełny materiał przez most. CT-05 zmienił pod to swój
własny test (`removed_topics_need_a_new_choice_but_large_material_stays_available`).

Przy scalaniu na trunk kolidowało to z blokiem, do którego WP-02 dołożył `plan_ready_to_start`.
Rozstrzygnięcie: **zostaje sprawdzenie planu** (ma tam jedynego wołającego, a jego usunięcie
cofnęłoby wyładowany etap), **znika wołanie kontekstu** (drugie sprawdzenie egzekwowałoby stare
znaczenie limitu). Parametr `home` zszedł z `nothing_stops_this_run`, bo istniał wyłącznie dla
tej bramki.

**Koszt:** `commands::workflow_context::context_ready_to_start` jest teraz wołany **wyłącznie
z testu** CT-05. Kryterium „brak źródła zatrzymuje Start przed pierwszym procesem" **jest
egzekwowane** — przez `freeze_context_inputs`, z własnym testem CT-06
(`a_missing_referenced_source_refuses_start_before_the_first_process`) — ale funkcja została
martwym kodem produkcyjnym i zielony test przy niej **wygląda** jak dowód działającej bramki.
Do usunięcia razem z przepisaniem tamtego testu; nie robię tego w rozstrzygnięciu merge'a,
bo to zmiana zakresu CT-05, nie scalenie.

**D-7. Zlecenie i produkt nazywają ten sam panel inaczej — do rozstrzygnięcia w CT-09.**
Zgłoszone przez planistę CT-08 jako `POZA ZAKRESEM` i **słusznie nienaprawione**.
`docs/context-library/PLAN.md` §11 pisze o panelu **`What this agent was told`**, a produkt
renderuje **`What this step knew`** (`src/sections/run/past/panel.tsx:110`, stała
`WHAT_THIS_STEP_KNEW`).

Sprawdzone: nazwa produktu jest **starsza** niż to zlecenie i przypięta testami, więc zmiana
napisu to zmiana widzianego tekstu i wyroczni, a nie literówka w dokumencie. Obie nazwy są przy
tym poprawne po angielsku i obie mówią prawdę o zawartości.

Rozstrzygnięcie należy do **CT-09**, którego zakres wprost obejmuje doprowadzenie dokumentacji do
zgodności z **rzeczywiście dostarczonym** produktem. Dwa napisy na jedną rzecz to naruszenie
niezmiennika 13 — ale w dokumencie, nie w kodzie, więc kosztuje uwagę czytelnika, a nie działanie
aplikacji.

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
