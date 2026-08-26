# T-133 — Próba błędu IO refleksji jest obserwowalna, zanim receipt wyląduje

T-132 zbudowało pełny zamrożony receipt pamięci i przeszło trzy hostowe bramki 19/19, lecz
nie może wylądować. Recenzent wykazał jedną lukę medium w AC-1: fixture tworzy katalog pod
ścieżką trzeciej kandydatki refleksji, a końcowa asercja sprawdza tylko, że ten katalog nadal
istnieje. Implementacja przetwarzająca wyłącznie dwie pierwsze kandydatki nadal zachowałaby
jedną notatkę, policzyła tombstone i przeszła bez próby niezależnego zapisu IO. Jedyna runda
naprawcza potwierdziła wadę oracle, ale nie zmieniła testu.

To zadanie jest pełnym następcą T-132. Zachowuje mocną granicę `AgentDriver::start -> Err`,
zamrożony drut historii i prawdziwy Enter/kliknięcie Chromium z T-132. Dodaje jeden świeży,
globalnie unikalny target, który obserwuje faktyczną trzecią próbę zapisu przez istniejący
produkcyjny warning. Nie dodaje nowego zachowania produkcyjnego, licznika ani testowego haka.

**Read first:** `tasks/T-132.md` · `runs/T-132/review.txt` · `runs/T-132/repair.txt` ·
`AGENTS.md` §2a i niezmienniki 4, 5, 13, 19, 20, 21, 23, 24 i 29 ·
`src-tauri/src/commands/run.rs` (`what_this_run_taught_us`, `keep_reflection_notes`,
`ReflectionReceipt`) · `src-tauri/src/memory/notes.rs`
(`record_project_candidate_from_run`, `Error::PreviouslyDiscarded`) · istniejący wzorzec
przechwytywania `tracing` w testach Rusta. Nie czytaj `docs/research/`.

## Kto to robi

- **Agent:** Codex przez jeden pełny bieg Harnessu.
- **Druga opinia:** osobny Codex na innym modelu, tylko do odczytu; jawny właścicielski
  wyjątek operacyjny od D3 z powodu kończącego się budżetu Claude'a.

## Pełny zakres, który ma wylądować

Produkt zachowuje cały kontrakt T-132:

- `run.json` zapisuje addytywny, posortowany receipt z pełnym adresem i typowanym pochodzeniem;
- `recipients`/`leftOutFor` dostają wyłącznie fizyczne UUID kroków, których `start` zwrócił
  uchwyt; odmowa przed uchwytem i krok pominięty przez graf nie trafiają do list;
- historia filtruje zamrożony receipt po UUID, bez odczytu bieżącego katalogu ani SQLite;
- prawdziwy ekran po `/history` + Enter + kliknięciu wiersza pokazuje receipt pod właściwym
  krokiem oraz nie zgaduje po kształcie identycznego UUID w `project` i `from`;
- `discardedAgain` rośnie wyłącznie przy `PreviouslyDiscarded`, `kept` wyłącznie po udanym
  zapisie, a niezależny błąd IO nie zwiększa żadnego z tych liczników.

Trzy targety T-132 pozostają pełnymi regresjami i mają wejść do trunku razem z produkcją.
Nowy target T-133 nie zastępuje ich ani nie przepisuje.

## Przejęcie pracy T-132

Dopiero po uczciwym, enforced `./verify.sh before` wolno selektywnie zastosować dokładnie te
commity z `task-T-132`:

- `5932154` — trzy zaakceptowane specy T-132;
- `ab71cfc` — rzeczywiści odbiorcy i licznik refleksji;
- `635d6f1` — zamrożony drut historii;
- `48a7fed` — prawdziwy ekran historii.

Nie przenoś `e2bdc0e`, branchowego `TASK.md`, całej gałęzi ani żadnego innego commita.
Nie zmieniaj `tasks/T-132.md`, żeby ukryć jego lukę. Jeśli konflikt z nowszym trunkem wystąpi,
rozwiąż go według tego kontraktu i zachowaj oba niezależne pokrycia.

## Uczciwy `before`

Przed `./verify.sh before` istnieje świeży standalone target T-133 i korzysta wyłącznie z
obecnych publicznych wejść. Uruchamia prawdziwy workflow z atrapą sterownika, czyta receipt
jako `serde_json::Value` i przechwytuje istniejący produkcyjny strumień `tracing`; nie wymaga
nowego pola, helpera ani testowego `cfg` w produkcji. Na bieżącym trunku test ma się
skompilować, uruchomić i paść na asercji: `discardedAgain` nie rozróżnia jeszcze tombstone'a,
a niezależna próba IO jest logowana jak każdy inny błąd. Brak targetu, błąd kompilacji,
`0 passed`, `#[ignore]` albo niedopasowany filtr nie są czerwienią.

## AC-1 Trzecia kandydatka naprawdę dochodzi do niezależnej gałęzi IO

check: cargo test --test t133_reflection_io_attempt_is_observable
expect: (\d+) passed

Standalone target uruchamia prawdziwy workflow, który kończy co najmniej jeden krok agenta i
naprawdę uruchamia prywatną refleksję przez istniejący driver. Odpowiedź refleksji zawiera
dokładnie trzy poprawne pary `rule`/`because`, w tej kolejności:

1. kandydatkę, którą da się zapisać;
2. kandydatkę z dokładnym tombstonem, więc `record_project_candidate_from_run` zwraca typowane
   `PreviouslyDiscarded`;
3. kandydatkę, której docelowa ścieżka `.md` jest katalogiem i dlatego prawdziwa próba
   `record_project_candidate_from_run` kończy się niezależnym błędem IO.

Test używa runtime Tokio `current_thread` i instaluje lokalny, zakresowy subscriber `tracing`
wokół całego `await`; nie używa globalnego `try_init`, który kolidowałby z równoległymi
testami. Po biegu wymaga jednocześnie:

- `reflection.ran == true`, `kept == 1`, `discardedAgain == 1` i braku innych zaliczonych
  zapisów;
- dokładnie jednego warningu produkcyjnego
  `this run had something to remember and it could not be written down`, z UUID faktycznie
  utworzonego biegu i błędem IO;
- braku warningu `PreviouslyDiscarded`: tombstone jest policzoną, typowaną odmową, nie
  nierozpoznanym błędem;
- braku pliku dla trzeciej kandydatki oraz zachowania katalogu, który wywołał odmowę.

Sam stan katalogu nie jest dowodem próby. Sam receipt również nie wystarcza. Asercja warningu
jest konieczna: usunięcie trzeciej kandydatki, zatrzymanie iteracji po dwóch pierwszych,
zaliczenie IO do `discardedAgain` albo potraktowanie tombstone'a jak zwykłego błędu musi
przewrócić target. Test nie wywołuje `keep_reflection_notes` ani
`record_project_candidate_from_run` wprost i nie sprawdza źródłowych stringów jako substytutu
zachowania.

<!-- OWNS
tasks/T-133.md
src-tauri/src/commands/run.rs
src-tauri/src/commands/history.rs
src-tauri/tests/t132_memory_receipt_actual_recipients.rs
src-tauri/tests/t132_memory_receipt_history_wire.rs
src-tauri/tests/t133_reflection_io_attempt_is_observable.rs
src/sections/run/io.ts
src/sections/run/past/panel.tsx
e2e/tests/t132-memory-receipt-real-history-controls.spec.ts
-->
