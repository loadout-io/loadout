# T-94 — Jedna pula na aplikację, budżet biegu, ciężki slot

Trzy limity, które ARCHITECTURE obiecuje, a bieg nie egzekwuje:

1. **„Ile naraz" jest per bieg.** `run_workflow_inner` (`commands/run.rs`) robi
   `Limiter::new(request.how_many_at_once)` na każdy bieg; to samo `run_agent_inner` dla `/ask`.
   `run_workflow_with_slots` i `workspace::Registry::slots` (jedna pula na aplikację, §6a) nie
   mają produkcyjnego wołającego — stwierdzone w komentarzu przy `run_workflow_inner`
   („to jest wada, nie wygoda: dwie karty dają `2 × limit`"). `limits_are_global_across_runs.rs`
   i `workspace_global_slots.rs` dowodzą właściwości typu, którego produkcja nie używa.
2. **Nie ma limitu kosztu.** Jedyny limit to `giveUpAfterMinutes`. 96-minutowy bieg właściciela
   kosztował ~$40 u Claude'a i nikt nie mógł powiedzieć „stop po $20". `claude --help` (2.1.241,
   zmierzone 2026-08-23) ma `--max-budget-usd <amount>` („only works with --print" — Loadout
   używa `-p`); spike S-2 jest tym samym rozstrzygnięty, a flaga nieużyta.
3. **`Weight::Heavy` żyje tylko w teście.** `heavy_step_takes_its_own_slot.rs` dowodzi, że
   `Limiter::with_heavy` i `dispatch_as(Weight::Heavy)` działają; produkcja woła `dispatch()`
   dla każdego rodzaju kroku, a komentarz przy `a_slot_for_this_step` przyznaje, że krok
   „sprawdź" (`cargo`, `rustc`) bierze zwykłe miejsce razem z agentami. Niezmiennik 26 (jeden
   ciężki `cargo` naraz na tym Macu) nie jest egzekwowany przez produkt, który ma go egzekwować.

**Read first:** `src-tauri/src/commands/mod.rs` (`RunDeps`, `RunRequest` — budżet wchodzi tu,
`Option<f64>`, `#[serde(default)]`) · `src-tauri/src/commands/run.rs` (`run_workflow_inner`,
`run_workflow_with_slots`, `run_agent_inner`, `a_slot_for_this_step`, `one_turn` — gdzie wpada
`cost_usd` z tury; `close_the_book`) · `src-tauri/src/engine/limits.rs` (`Limiter`, `Run`,
`dispatch_as`, `Weight`, `Refusal` — ma jeden wariant i miejsce na drugi) ·
`src-tauri/src/ipc.rs` (`AppState`, `begin_run`, `run_workflow`, `run_agent` — tu żyje jedyna
pula) · `src-tauri/src/lib.rs` (konstrukcja `AppState`) · `src-tauri/src/engine/drivers/claude.rs`
(`command` — `--max-budget-usd` obok `--model`; komentarz o S-2 do poprawienia) ·
`src/sections/run/limits/at-once.tsx`, `chosen.ts`, `start.tsx`, `io.ts` ·
`src/sections/commands-wired.test.ts` · `AGENTS.md` niezmienniki 11, 26.

## Kto to robi

- **Agent:** `rust-core` na AC-1…AC-4, potem `frontend` na AC-5 — jeden worktree, jedna bramka.
- **Druga opinia:** inny vendor niż pisarz (D3).

## AC-1 Dwa biegi w dwóch folderach dzielą jedną pulę
check: cargo test --test it two_folders_share_one_pool::
expect: (\d+) passed

`AppState` trzyma jeden `Limiter`, a `run_workflow_inner` i `run_agent_inner` biorą go z `RunDeps`
zamiast tworzyć własny. Dwa biegi (dwa foldery, `FakeDriver` z opóźnieniem, limit 2) mają
łącznie **nigdy** więcej niż dwa kroki w `running` naraz — dowód nakładania i dowód sufitu,
jak w `engine_concurrency_limit.rs`. Suwak „ile naraz" z drugiego Startu przestawia wspólny
limit (w dół bez zabijania — `owed`, jak dziś). `run_workflow_with_slots` zostaje jako jedyna
implementacja, a `run_workflow_inner` jako cienkie opakowanie z pulą z `deps`.

## AC-2 Bieg ma budżet i staje na nim
check: cargo test --test it a_run_stops_at_its_budget::
expect: (\d+) passed

Budżet jedzie **osobnym argumentem**, nie polem `RunRequest` — i to nie jest szczegół stylu.
Zmierzone 2026-08-23: literał `RunRequest { … }` stoi w **55 plikach**, a ten typ nie ma
`Default`, więc nowe pole przewraca każdy z nich naraz i większość leży poza tym zadaniem.
Argument przy `run_workflow_inner` / `run_agent_inner` psuje wyłącznie prawdziwych wołających,
których jest kilku i wszyscy są w OWNS. Kiedy suma `cost_usd` kroków, które się skończyły,
przekroczy budżet, żaden nowy krok nie dostaje slotu: kroki czekające przechodzą w `skipped`
ze zdaniem nazywającym budżet i kwotę, kroki biegnące **kończą się normalnie** (nie zabijamy
pracy, za którą już zapłacono), a `run.json` dostaje `budget_usd` i `spent_usd`. Bez budżetu
nic się nie zmienia. Krok bez `cost_usd` (Codex do T-97) liczy się jako zero i to jest
nazwane w zdaniu o budżecie w UI (AC-5).

## AC-3 Claude dostaje resztę budżetu jako własny limit
check: cargo test --test it claude_gets_the_remaining_budget::
expect: (\d+) passed

Krok Claude'a w biegu z budżetem dostaje `--max-budget-usd <reszta>` (budżet minus
dotychczasowa suma, zaokrąglone w dół do centa), żeby vendor sam zatrzymał turę, która
przekroczyłaby resztę. Bieg bez budżetu nie ma tej flagi. Przelotka z T-90 podająca
`--max-budget-usd` jest kolizją — odmowa z nazwą flagi.

## AC-4 Krok „sprawdź" bierze ciężki slot
check: cargo test --test it checks_take_the_heavy_slot::
expect: (\d+) passed

`Job::Check` woła `dispatch_as(Weight::Heavy)`; dwa kroki „sprawdź" gotowe naraz biegną
**po kolei** przy `heavy_at_once = 1`, a krok agenta obok nich biegnie równolegle. Pula
z AC-1 jest budowana z `with_heavy(1)` — to jest jedyne miejsce, w którym stoi ta jedynka,
z komentarzem o niezmienniku 26.

## AC-5 Obok „How many at once" stoi budżet
check: npx --no-install vitest run src/sections/run/limits/budget-is-sent-with-start.test.tsx
expect: (\d+) passed

Pole „Spend at most $" obok suwaka; puste = bez limitu; wartość jedzie w `run_workflow`
i `run_agent` jako `budgetUsd`. Zdanie pomocy mówi, że kroki Codeksa nie raportują kosztu
i liczą się jako zero (do T-97). Pasek biegu pokazuje `$3.41 of $20` zamiast samej kwoty,
kiedy budżet jest ustawiony. Kontrolka jest wygaszona w trakcie biegu tym samym zdaniem,
co suwak.

## Sprzątanie po drodze

Komentarze w `claude.rs` (ok. 921, 1091) o nierozstrzygniętym S-2 — popraw: rozstrzygnięte
pomiarem 2026-08-23. `workspace::Registry` zostaje (dalej bez wołającego) — to jest decyzja
człowieka, nie tego zadania; dopisz jedno zdanie w nagłówku `workspace.rs`, że pula przeniosła
się do `AppState`.

<!-- OWNS
tasks/T-94.md
src-tauri/src/commands/mod.rs
src-tauri/src/commands/run.rs
src-tauri/src/engine/limits.rs
src-tauri/src/engine/drivers/claude.rs
src-tauri/src/ipc.rs
src-tauri/src/lib.rs
src-tauri/src/workspace.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/two_folders_share_one_pool.rs
src-tauri/tests/it/a_run_stops_at_its_budget.rs
src-tauri/tests/it/claude_gets_the_remaining_budget.rs
src-tauri/tests/it/checks_take_the_heavy_slot.rs
src/sections/commands-wired.test.ts
src/ipc/run.ts
src/ipc/types.ts
src/sections/run/io.ts
src/sections/run/start.tsx
src/sections/run/limits/at-once.tsx
src/sections/run/limits/budget.tsx
src/sections/run/limits/chosen.ts
src/sections/run/limits/budget-is-sent-with-start.test.tsx
src/sections/run/strip/model.ts
src/sections/run/strip/strip.tsx
-->
