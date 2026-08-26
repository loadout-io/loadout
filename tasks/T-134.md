# T-134 — Live Stop ma sufit i zwalnia następny Start

Stare T-106 połączyło dwie niezależne ścieżki zabijania i użyło filtrowanych funkcji
wspólnego targetu `tests/it`, więc nie będzie uruchamiane. To zadanie jest świeżym,
standalone następcą wyłącznie jego części o żywym Stopie.

Dziś `prove_agent_dead` zachowuje uchwyt słusznie, ale ponawia `cancel()` bez końca, kiedy
supervisor za każdym razem zwraca `GroupProof::Alive`. W takim biegu Stop nigdy nie wraca,
`run_workflow` nie zapala `settled`, a zapadka `AppState.live` odmawia każdego następnego
Startu aż do restartu aplikacji. Brak dowodu śmierci nie może zostać nazwany śmiercią, ale
nie może też zamrozić całej aplikacji.

**Read first:** `AGENTS.md` §2a i niezmienniki 6, 7, 13, 19, 20, 24 i 29 ·
`src-tauri/src/commands/run.rs` (`stop_run_inner`, `stop_if_anything_is_going`,
`stop_cancelled_agent`, `prove_agent_dead`) · `src-tauri/src/ipc.rs`
(`AppState::begin_run`) · `src-tauri/src/commands/history.rs` (`read_run_inner`) ·
`src-tauri/tests/it/run_stop_waits_for_proof.rs` i
`src-tauri/tests/t126_late_stop_and_empty_handoff.rs` jako regresje, nie miejsca nowych
testów. Nie czytaj `docs/research/`.

## Kto to robi

- **Agent:** Codex przez jeden pełny bieg Harnessu, po T-133.
- **Druga opinia:** osobny Codex na innym modelu, tylko do odczytu; jawny właścicielski
  wyjątek operacyjny od D3 z powodu kończącego się budżetu Claude'a.

## Uczciwy `before`

Przed `./verify.sh before` istnieje kompletny standalone target i używa wyłącznie obecnych
publicznych wejść. Atrapowy `AgentHandle` ma prawdziwy `GroupId`, pozostaje w turze do Stopu,
zlicza każde `cancel()` i w scenariuszu negatywnym zawsze zwraca `GroupProof::Alive` — nie
uruchamia procesu systemowego i nie może zostawić sieroty testowej. Target owija prawdziwe
wywołanie Stopu własnym wysokim limitem czasu. Na bieżącym trunku ma się skompilować,
uruchomić i paść na asercji timeoutu, bo `prove_agent_dead` nie ma sufitu. Brak targetu,
błąd kompilacji, `0 passed`, `#[ignore]` albo rc 124 nie są czerwienią.

## AC-1 Stop kończy się uczciwie także bez dowodu śmierci

check: cargo test --test t134_live_stop_has_a_ceiling
expect: (\d+) passed

Standalone target zawiera dwa niezależne scenariusze i przechodzi rzeczywistą drogę
`AppState::begin_run` → `run_workflow_with_reflection(..., false)` →
`stop_run_inner`; nie woła `prove_agent_dead` ani prywatnych helperów wprost.

W scenariuszu uporczywym uchwyt przy każdym `cancel()` zwraca `GroupProof::Alive`:

- produkcja wykonuje dokładnie trzy pełne próby, po czym Stop i zadanie biegu wracają przed
  limitem testu; liczba prób jest stałą produkcyjną, nie parametrem testowym;
- cały bieg zachowuje wartość `Outcome::Cancelled`, ale krok kończy się `failed`, nie
  `cancelled`, bo Loadout nie ma dowodu, że należący do niego proces zszedł;
- `run.json` zachowuje dokładne dziesiętne `pid` i `pgid`, `death_proof == false` oraz jedno
  angielskie zdanie mówiące, że agent przeżył zatrzymywanie i może nadal działać; nie wolno
  podmienić tej prawdy na `Dead` ani zgubić jedynego adresu dla cleanupu przy starcie;
- `commands::history::read_run_inner` oddaje to samo zdanie w `PastStepWire.error`, czyli
  polu renderowanym przez istniejący panel historii; pełna bramka zachowuje regresję panelu,
  która dowodzi, że niepusty `step.error` naprawdę pojawia się na ekranie;
- po domknięciu pierwszego zadania ten sam `AppState` przyjmuje drugi Start, a drugi,
  natychmiast udany uchwyt rzeczywiście kończy bieg. Sam licznik `cancel()` bez drugiego Startu
  nie dowodzi zwolnienia zapadki.

Scenariusz kontrolny zwraca `GroupProof::Dead` przy pierwszym `cancel()`. Stop wywołuje go
dokładnie raz, krok pozostaje `cancelled`, `death_proof == true`, a `run.json` nie dostaje
zdania o ocalałym procesie. Sufit nie może pogorszyć dzisiejszej uczciwej drogi sukcesu.

Nie zmieniaj 30-sekundowej semantyki zamykania okna i nie ucz `AppState` zgadywania śmierci.
T-134 nie eskaluje startup cleanupu; ten zakres należy wyłącznie do T-135.

<!-- OWNS
tasks/T-134.md
src-tauri/src/commands/run.rs
src-tauri/tests/t134_live_stop_has_a_ceiling.rs
-->
