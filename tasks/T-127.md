# T-127 — Prywatny stan każdego procesu Claude'a, także kopii i refleksji

T-109 pozostaje paragonem incydentu, ale nie jest wykonalnym ship-taskiem według
`AGENTS.md` §2a. Wszystkie trzy jego kryteria filtrują wspólny target `tests/it` po nazwie,
zamiast wskazywać jeden globalnie unikalny plik testu. Zadanie nie posiada też
`commands/run.rs` ani `engine/drivers/mod.rs`, choć pierwsze zna fizyczny klucz pracy, a drugie
jest jedynym vendor-neutralnym szwem ustawień.

To zadanie jest świeżym następcą T-109. Startuje wyłącznie z `main` po wylądowaniu T-126,
ponieważ T-126 najpierw podpina refleksję do `with_settings`, evidence i budżetu. Nie wolno
prowadzić T-126 i T-127 równolegle: ich `OWNS` przecinają się w trzech plikach produkcyjnych.
Nie przenoś commitów, testów ani filtrowanych checków T-109.

Incydent pozostaje ten sam: sześć równoległych procesów Claude'a dzieliło `HOME` i zapisywało
ten sam `~/.claude.json`; jeden padł na uszkodzonym JSON-ie po 273 ms. Serializacja jest
zakazana — dwie kopie nadal mają naprawdę biec naraz (niezmiennik 11). Oficjalnym szwem
Claude Code jest `CLAUDE_CONFIG_DIR`:
https://code.claude.com/docs/en/env-vars#environment-variables.

Każda fizyczna praca dostaje `<run>/claude/<work-key>`. Zwykły krok używa tego samego
`work-key`, którym bieg nazywa `work/<work-key>`: pierwsza kopia `s_parallel`, druga
`s_parallel~2`, a kolejne rundy tej samej kopii zachowują ten klucz. Prywatna tura Loadouta
używa wyłącznie `_reflection`.

**Read first:** `tasks/T-109.md` · wylądowane `tasks/T-126.md` · `AGENTS.md` §2a oraz
niezmienniki 9, 11, 19, 20, 21, 23 i 29 ·
`src-tauri/src/commands/run.rs` (`work_key_of`, `with_its_own_settings`,
`a_short_turn_about`, `never_started`, `run_agent`) ·
`src-tauri/src/engine/drivers/mod.rs` (`DriverConfiguration`, `StepSettings`,
`AgentDriver::with_settings`, `reflecting`) ·
`src-tauri/src/engine/drivers/claude.rs` (`RunSettings::for_step`,
`ClaudeDriver::with_settings`, `reflecting`, `start_after_evidence`) ·
`src-tauri/src/engine/supervisor.rs` (`PASSTHROUGH`, `spawn_with_environment`) wyłącznie
jako zamrożony kontrakt ·
`src-tauri/tests/it/claude_memory_stays_in_the_run.rs`.

## Kto to robi

- **Agent:** Codex przez jeden pełny bieg Harnessu.
- **Druga opinia:** osobny Codex na innym modelu, tylko do odczytu.

## Granice implementacji

`StepSettings` dostaje vendor-neutralne `pub work_key: String`. Nie wolno wkładać do tego typu
pola nazwanego `claude_*`, gotowego `CLAUDE_CONFIG_DIR` ani vendorowej ścieżki. Dla zwykłego
kroku `commands/run.rs` przekazuje `work_key_of(plan.steps[id].node_key)`, nie `tile_key`;
inaczej dwie kopie znowu dostaną jeden stan. Refleksja przekazuje literal `_reflection`.
T-127 nie zmienia decyzji T-126 o tym, kiedy refleksja biegnie, ile kosztuje ani jakie ma
evidence.

`RunSettings::for_step` nie wyprowadza już tożsamości procesu z końcówki `memory`.
`work_key` jest źródłem nazwy per-procesowego pliku ustawień oraz katalogu
`<run>/claude/<work-key>`. `memory` zachowuje dotychczasowe znaczenie i nie jest w tym zadaniu
przemianowywane ani przenoszone.

Katalog stanu powstaje idempotentnie w `with_settings`, przed jakimkolwiek wywołaniem
`AgentDriver::start`. Istniejącego katalogu nie czyścimy. Plik, niedostępny rodzic lub inna
odmowa systemu plików zatrzymuje proces; nie ma retry bez zmiennej, fallbacku do `HOME` ani
globalnego muteksa.

Nie zmieniaj `supervisor.rs` ani `PASSTHROUGH`. `spawn_with_environment` robi `env_clear()`,
następnie nakłada przekazaną listę. Claude składa więc finalne środowisko z Connection env,
usuwa lub zastępuje każdą hostile wartość `CLAUDE_CONFIG_DIR` i dokłada dokładnie jedną
prywatną wartość jako ostatnią. Pozostałe zatwierdzone zmienne oraz `HOME` przeżywają.
Ustawienie prywatnej wartości przez `Command::env` przed supervisorem jest błędem:
`env_clear()` ją skasuje.

Nie kopiuj ani nie czytaj `~/.claude.json`, `~/.claude/`, tokenów, sesji ani poświadczeń
człowieka. Prywatna ścieżka i wartość nie trafiają do argv, promptu, `run.json`, transkryptu
ani `Debug`. Sonda `--version`, która nie ma katalogu biegu ani `StepSettings`, zachowuje
dzisiejszy kształt i niczego nie tworzy.

Odmowa utworzenia prywatnego katalogu ma rozpoznawalną kategorię błędu; `run.rs` nie zgaduje
jej po `driver.id()`, ścieżce ani fragmencie tekstu `anyhow`. Pełny łańcuch IO zostaje
w diagnostyce, lecz zdanie publiczne jest dokładnie jedno:

`Loadout could not create this agent's private state folder, so it did not start the step.`

Dla zwykłego kroku zdanie idzie przez `AgentEvent::Notice`, staje się widocznym
`Line::Problem` i trafia bez prefiksu ani sufiksu do `run.json.steps[*].error`.

W istniejącym `claude_memory_stays_in_the_run.rs` wolno wyłącznie uzupełnić literały
`StepSettings` o właściwe `work_key` i zachować wszystkie dotychczasowe scenariusze oraz
asercje.

## Uczciwy `before`

Przed `./verify.sh before` muszą istnieć trzy nowe standalone targety oraz kompilowalny
szkielet `StepSettings.work_key`; wszystkie istniejące literały w zakresie `OWNS` muszą się
kompilować. Jeżeli nowy helper wymaga sygnatury, dostaje ciało `todo!("T-127 skeleton")`.

Przed `before` nie wolno jeszcze tworzyć katalogu stanu, nakładać prywatnego env, podpinać
`_reflection` ani mapować publicznej odmowy. Każdy target ma zostać znaleziony, uruchomić co
najmniej jeden test i paść w czasie wykonania z powodu brakującego zachowania. Brak targetu,
błąd kompilacji, `0 passed`, `#[ignore]` lub filtr nazwy nie są czerwienią.

Każdy nowy target ma jedną funkcję testową, jawne ograniczenia czasu i prawdziwy wykonywalny
skrypt zamiast `sh -c`. Testy nie czytają źródła i nie asertują obecności stringów.
Środowisko i ścieżki raportuje sam proces potomny.

## AC-1 Prywatna wartość przeżywa env_clear i wygrywa z hostile Connection env

check: cargo test --test t127_private_claude_state_paths
expect: (\d+) passed

Test tworzy dwa `StepSettings` z jednym katalogiem biegu i literalnymi kluczami `s_1` oraz
`s_1~2`. Przechodzi publiczną kolejnością produkcji:
`configured(DriverConfiguration)` → `with_settings` → `start`. Konfiguracja Connection
celowo niesie hostile `CLAUDE_CONFIG_DIR` wskazujące poza bieg oraz osobną, nieszkodliwą
zmienną kontrolną.

Jeszcze przed `start` test wymaga dwóch istniejących katalogów:
`<run>/claude/s_1` i `<run>/claude/s_1~2`. Następnie prawdziwa binarka-atrapa startuje przez
`ClaudeDriver` i supervisor, sama zapisuje środowisko, które zobaczyła, i emituje poprawny
stream Claude'a.

Oba procesy widzą dokładne, bezwzględne prywatne ścieżki, nigdy hostile wartość. Zmienna
kontrolna Connection oraz `HOME` nadal istnieją. Każdy proces zapisuje marker wyłącznie we
własnym katalogu. Hostile katalog i przygotowany fałszywy `HOME/.claude.json` pozostają bajt
w bajt bez zmian. Sonda tego samego drivera bez `StepSettings` nie tworzy katalogu i nie
dostaje `CLAUDE_CONFIG_DIR`.

Zakazane: sprawdzanie `Command::get_envs`, `Debug`, argv albo samego wyniku buildera bez
uruchomienia dziecka.

## AC-2 Dwie kopie naprawdę nakładają się, a refleksja jest trzecim izolowanym spawnem

check: cargo test --test t127_parallel_claude_state_isolation
expect: (\d+) passed

Test przechodzi przez `run_workflow_inner` i produkcyjną fabrykę zwracającą prawdziwy
`ClaudeDriver::with_binary`. Workflow ma jeden krok Claude'a z `copies: 2`,
`folder.use: fresh-copy`, limitem co najmniej dwóch miejsc i użytecznym przekazaniem, które
uruchamia wylądowaną refleksję T-126. Refleksja jest jawnie włączona przez publiczne żądanie
T-126.

Dwie zwykłe binarki-atrapy używają bariery lub pełnych przedziałów start–koniec i dowodzą
dodatniego przecięcia swoich okien. Refleksja biegnie później; nie jest zaliczana do tego
przecięcia. Cały bieg startuje dokładnie trzy procesy inne niż `--version`: dwie kopie i jedną
refleksję. Nie wolno zastąpić ich atrapą implementującą `AgentDriver`.

Procesy same meldują widziane `CLAUDE_CONFIG_DIR`. Pełny zbiór jest dokładnie równy:

- `<run>/claude/s_parallel`
- `<run>/claude/s_parallel~2`
- `<run>/claude/_reflection`

Każdy proces potwierdza, że jego katalog istniał już w chwili wejścia do skryptu, i zostawia
marker wyłącznie tam. Ścieżki są parami różne. Przygotowany fałszywy
`HOME/.claude.json` pozostaje bajt w bajt bez zmian. Nie ma czwartego procesu, retry bez
zmiennej, wspólnego `<run>/claude/s_parallel` dla obu kopii ani stanu refleksji pod kluczem
ostatniego kroku.

Zakazane: dwa sekwencyjne spawny nazwane równoległością, timestamp samego wejścia bez końca
okna, stały sen użyty jako jedyny synchronizator, fake `AgentDriver`, zgadywanie roli procesu
z promptu lub pominięcie refleksji w liczniku.

## AC-3 Awaria przygotowania stanu daje dokładną widoczną odmowę przed spawnem

check: cargo test --test t127_state_setup_refuses_before_spawn
expect: (\d+) passed

Test uruchamia publiczną drogę workflow z checkpointem przed jednym krokiem Claude'a.
Podczas prawdziwej pauzy odczytuje utworzony katalog biegu i zapisuje zwykły plik pod
`<run>/claude`, tak aby `<run>/claude/<work-key>` nie mógł powstać. Dopiero potem odpowiada
checkpointowi i pozwala schedulerowi dojść do kroku.

Fabryka zwraca prawdziwy `ClaudeDriver::with_binary`; jego skrypt utworzyłby jednoznaczny
marker przy każdym wejściu. Marker nie powstaje, więc liczba spawnów wynosi dokładnie zero.
Nie ma drugiej próby bez `CLAUDE_CONFIG_DIR`, a fałszywy `HOME/.claude.json` pozostaje
nietknięty. Ponieważ żaden model nie skończył kroku, refleksja także nie startuje.

Test odbiera publiczny strumień linii i wymaga dokładnie jednego widocznego
`Line::Problem`, pochodzącego z `AgentEvent::Notice`, z tekstem co do bajtu:

`Loadout could not create this agent's private state folder, so it did not start the step.`

Następnie czyta prawdziwy `run.json`, znajduje krok po jego kluczu i wymaga
`status == "failed"` oraz `error` równego dokładnie temu samemu zdaniu. Prefiks
`Loadout could not start this agent:`, ścieżka systemowa, surowy błąd IO, dodatkowe
wyjaśnienie albo samo `Err` bez widocznego wiersza pozostawiają kryterium czerwone.

<!-- OWNS
tasks/T-127.md
src-tauri/src/commands/run.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/src/engine/drivers/claude.rs
src-tauri/tests/it/claude_memory_stays_in_the_run.rs
src-tauri/tests/t127_private_claude_state_paths.rs
src-tauri/tests/t127_parallel_claude_state_isolation.rs
src-tauri/tests/t127_state_setup_refuses_before_spawn.rs
-->
