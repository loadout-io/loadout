# T-145 — Recovery zachowuje wszystkie regresje i ma jedno wyjście

Świeży standalone następca zamkniętego T-144. T-144 po jedynej naprawie miało zielone oba
nowe AC i pełny clippy, lecz końcowa bramka pozostała 17/18 na niezależnym timeoutcie pełnej
suity. Ważniejsza odmowa Harnessu wykazała, że migracja trzech starych speców zmniejszyła
liczbę asercji z 16 do 10, z 13 do 10 oraz z 10 do 9. Gałąź jest dowodem, nie źródłem do
lądowania w całości.

Po własnym uczciwym `before` wolno selektywnie zastosować wyłącznie produkcyjne commity
T-144: `d8e5ca4` oraz `7fef6fc`. Nie przejmuj `TASK.md`, commitów speców `f8211e2`,
`f2951d1`, `e4ea343`, migracji starych testów `cc2e102` ani całej gałęzi. Uwagi recenzenta
T-144 — pełny kompozytor argv i rekurencyjny zakaz martwych pól — są częścią tego kontraktu,
więc nowe targety dowodzą ich samodzielnie.

`RunSpec.resume` zostaje jawnym transportem adapterów, nie recovery.

**Read first:** `src-tauri/src/recovery.rs` · `src-tauri/src/commands/reconcile.rs` ·
`src-tauri/src/engine/drivers/{mod,claude,codex}.rs` · wskazane `recovery_*.rs` ·
`tasks/T-144.md` · `runs/T-144/{review,repair,assertions-certified.tsv,assertions-now.tsv}`.

## Kto to robi

- **Agent:** Codex przez jeden pełny bieg Harnessu, po zamknięciu T-144.
- **Druga opinia:** osobny Codex na innym modelu, tylko do odczytu; właścicielski wyjątek D3.

## AC-1 Recovery sprząta i oznacza wyłącznie faktycznie przerwane kroki
check: cargo test --test t145_recovery_cleanup_preserves_all_regressions
expect: (\d+) passed

Target przez prawdziwe `rows_to_judge` obejmuje pełny zakres T-144:

- brak/pusta sesja i `attempt = -1`/`i64::MAX` nie wpływają na decyzję;
- tylko `ready`/`running` dostają `failed`/`interrupted`, a ich run `interrupted`;
- running run zawierający wyłącznie skończony krok z pozostawionym PGID nie dostaje statusu;
- na starym boocie `pgid = NULL`, `0`, liczba ujemna i własny PGID Loadouta prowadzą do
  oznaczenia kroku/runu bez reap i bez `unreadable`;
- te same nieużywalne PGID-y na bieżącym boocie są odmową w `unreadable`, nie celem sygnału
  ani zgodą na oznaczenie;
- bezpieczne współdzielone PGID-y są deduplikowane i sprzątane raz; nieznany status pozostaje
  `unreadable`;
- serializowany plan ma wyłącznie `reap`, `run_status`, `step_status`, `unreadable`, a
  rekurencyjne przejście wszystkich kluczy odrzuca każdą zagnieżdżoną sesję, próbę, opcję,
  pytanie, efekt albo resume;
- drugi przebieg nad rozstrzygniętymi wierszami niczego nie robi.

Usuń z `RecoveryRow` i SELECT-a recovery tylko `session_id`/`attempt`; kolumny `steps`
zostają. Run wolno oznaczyć dopiero po co najmniej jednym `RowVerdict::CutOff`. Dla starego
bootu nie ma reap, więc PGID nie jest walidowany; dla bieżącego bootu walidacja pozostaje.

## AC-2 Resume jest jawnym transportem obu adapterów i nie wraca z recovery
check: cargo test --test t145_resume_is_explicit_driver_transport
expect: (\d+) passed

Przerwany wiersz z `agent_session_id` nie oddaje z `decide()` sesji, pytania ani efektu
wznowienia. Osobno `RunSpec { resume: Some(...) }` prowadzi przez pełne produkcyjne
kompozytory argv Claude'a i Codeksa do właściwego identyfikatora, a `None` wybiera pierwszą
turę. Dla Codeksa target wywołuje `codex::exec_argv` z rzeczywistą konfiguracją i sentinelem,
nie sam pomocniczy `build_exec_argv`. Target czyta nagłówek `drivers/mod.rs` i wymaga pełnego
sensu: jawny transport adaptera oraz recovery, które go nie konstruuje.

## Zachowanie starych regresji

Po zmianie API sześć istniejących modułów recovery musi nadal kompilować się i pytać o
odpowiedniki swoich historycznych własności. Nie wolno usuwać pliku ani zastępować asercji
jedną zbiorczą tautologią. Każda nieaktualna asercja ma dostać konkretny odpowiednik w nowym
kontrakcie. W szczególności końcowy odcisk nie może spaść poniżej certyfikowanego `main`:

- `recovery_asks_never_resumes.rs`: co najmniej **16** linii asercji;
- `recovery_boot_guard.rs`: co najmniej **13** linii asercji;
- `recovery_unreadable_rows.rs`: co najmniej **10** linii asercji.

Niezależny timeout `trigger_editor_writes_safe_file` z końca T-144 nie należy do tego zadania.
Nie zmieniaj tego testu ani jego limitów; zwykła pełna bramka rozstrzyga, czy był flakiem.

## Uczciwe `before`

Oba nowe targety muszą kompilować się na dzisiejszym API i paść na asercjach zachowania przed
zastosowaniem któregokolwiek commita T-144. Żaden SQL bez `#` nie używa raw-string hashy.
Brak symbolu, targetu, kompilacji albo lint nie jest prawidłową czerwienią.

## Wyłączenia

Nie usuwać `RunSpec.resume`, `SessionRef`, argv resume ani kolumn `steps.agent_session_id` i
`steps.attempt`. Nie zmieniać eskalacji startup reaper, tabeli `memory`, `Absent`, `supersede`
ani `Kind`. Nie kopiować targetów T-141/T-144 i nie lądować ich gałęzi. Nie zmieniać
`trigger_editor_writes_safe_file.rs`, Harnessu, checks ani limitów pełnej suity.

<!-- OWNS
tasks/T-145.md
src-tauri/src/recovery.rs
src-tauri/src/commands/reconcile.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/tests/t145_recovery_cleanup_preserves_all_regressions.rs
src-tauri/tests/t145_resume_is_explicit_driver_transport.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/recovery_asks_never_resumes.rs
src-tauri/tests/it/recovery_boot_guard.rs
src-tauri/tests/it/recovery_reap_targets.rs
src-tauri/tests/it/recovery_records_boot_time.rs
src-tauri/tests/it/recovery_status_table.rs
src-tauri/tests/it/recovery_unreadable_rows.rs
-->
