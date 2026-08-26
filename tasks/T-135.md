# T-135 — Sprzątanie przy starcie eskaluje i zostawia ślad

To zadanie jest świeżym, standalone następcą części starego T-106 o sprzątaniu po restarcie.
Idzie dopiero po wylądowaniu T-134 i nie wznawia T-106, nie używa jego filtrowanych targetów
ani commitów.

Dziś `reap_group` wysyła pojedynczy `SIGTERM` i natychmiast pyta sygnałem zerowym, czy grupa
jeszcze istnieje. Nie daje procesuowi łaski, nie eskaluje do `SIGKILL`, a `StillAlive` zostaje
wyłącznie licznikiem w logu. Człowiek widzi ogólne przerwanie kroku bez informacji, że proces
przeżył, i bez PID/PGID potrzebnych do ręcznego rozpoznania szkody.

**Read first:** `AGENTS.md` §2a i niezmienniki 3, 4, 5, 6, 13, 19, 20, 24 i 29 ·
`src-tauri/src/engine/supervisor.rs` (`Supervised::stop` jako wzór oraz `reap_group`) ·
`src-tauri/src/commands/reconcile.rs` (`reconcile_runs`, `with_reaper`, `write_back`) ·
`src-tauri/src/ipc.rs` (`settle_everything_left_behind`) ·
`src-tauri/src/commands/history.rs` (`read_run_inner`) · istniejące regresje
`supervisor_term_then_kill.rs`, `recovery_proof_of_death.rs` i
`runs_left_over_are_reconciled.rs`. Nie czytaj `docs/research/`.

## Kto to robi

- **Agent:** Codex przez jeden pełny bieg Harnessu, po wylądowaniu T-134.
- **Druga opinia:** osobny Codex na innym modelu, tylko do odczytu; jawny właścicielski
  wyjątek operacyjny od D3 z powodu kończącego się budżetu Claude'a.

## Uczciwy `before`

Oba nowe standalone targety istnieją w całości przed `./verify.sh before` i korzystają z
obecnych publicznych wejść. AC-1 uruchamia prawdziwe grupy procesów i na bieżącym trunku pada
na asercji, bo uparta grupa przeżywa pojedynczy `SIGTERM`, a `reap_group` zwraca `Alive` bez
łaski i KILL. AC-2 wykonuje prawdziwe `commands::reconcile::with_reaper` i pada, bo oba kroki
dostają dziś ten sam ogólny `STEP_CUT_OFF`; wynik `StillAlive` nie dociera do `run.json` jako
osobny widoczny fakt. Brak targetu, błąd kompilacji, `0 passed`, `#[ignore]` albo rc 124 nie
są czerwienią.

## AC-1 Sierota przechodzi TERM → łaska → KILL → dowód

check: cargo test --test t135_startup_cleanup_escalates
expect: (\d+) passed

Target uruchamia dwie prawdziwe, odrębne grupy procesów. Lider-launcher kończy się i jest
zebrany przed `reap_group`, żeby zombie dziecka testu nie fałszowało sondy `ESRCH`.

- Grzeczna grupa zapisuje znacznik wyłącznie w handlerze `SIGTERM`, schodzi przed końcem
  łaski, nie dostaje `SIGKILL` i nie czeka całego okna.
- Uparta grupa ignoruje `SIGTERM`, pozostaje żywa przez całe okno łaski, ginie dopiero od
  `SIGKILL`, a `reap_group` zwraca `Dead` dopiero po sondzie, która naprawdę dostała `ESRCH`.
- Każda droga ma jawny stały sufit. Natychmiastowe `ESRCH` wraca bez czekania, a brak dowodu
  po KILL wraca jako `Alive`, nigdy jako zgadnięte `Dead`.
- Wynik inny niż `ESRCH`, w szczególności odmowa uprawnień, nie jest dowodem śmierci i nie
  wolno po nim eskalować na ślepo do `SIGKILL` w potencjalnie cudzą grupę.

Cała polityka sygnałów pozostaje w `engine/supervisor.rs`; `commands/reconcile.rs` jedynie
mapuje wynik i nie dostaje platformowych `cfg` ani drugiej implementacji eskalacji.

## AC-2 Ocalały jest zapisany na żywej ścieżce historii

check: cargo test --test t135_startup_survivor_reaches_history
expect: (\d+) passed

Target tworzy prawdziwy `run.json` z dwoma krokami `running` w jednym biegu i osobnym,
zakończonym biegiem kontrolnym. Wstrzyknięty domykacz pierwszej grupy zwraca `ProvenDead`,
a drugiej `StillAlive`. Po `commands::reconcile::with_reaper`:

- bieg jest `interrupted`, oba rozpoczęte kroki są `failed`, mają czas końca, a nieznane pola
  naprawianego pliku przeżywają zapis;
- tylko krok ocalałej grupy ma angielskie zdanie mówiące, że proces przeżył zatrzymywanie,
  z osobno widocznymi dziesiętnymi PID i PGID; krok udowodniony martwy zachowuje zwykłe
  zdanie o odcięciu i nie dostaje fałszywego ostrzeżenia;
- `commands::history::read_run_inner` oddaje dokładnie to zdanie w `PastStepWire.error`, czyli
  jedynym polu błędu renderowanym przez panel historii; pełna bramka zachowuje istniejącą
  regresję prawdziwego markup ekranu;
- zakończony bieg kontrolny pozostaje bajt w bajt nietknięty, a ponowne uzgodnienie nie dopisuje
  drugiego ostrzeżenia ani nie traci PID/PGID.

Nie dodawaj drugiej kopii do SQLite: `run.json` jest prawdą, a indeks ma się dać odbudować.
Nie dotykaj `recovery.rs`, `lib.rs`, frontendu ani martwej tabeli `memory`; świeże zadania
sprzątania po D-6 powstaną dopiero po T-135.

<!-- OWNS
tasks/T-135.md
src-tauri/src/engine/supervisor.rs
src-tauri/src/commands/reconcile.rs
src-tauri/tests/t135_startup_cleanup_escalates.rs
src-tauri/tests/t135_startup_survivor_reaches_history.rs
-->
