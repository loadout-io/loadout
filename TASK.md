# T-89 — Kafelek „sprawdź" da się postawić i ustawić z płótna

D6 (2026-08-20) dopisała trzeci rodzaj kafelka: **sprawdzenie** — komenda należąca do Loadouta,
która sama wystawia wynik z kodu wyjścia **i** licznika (niezmiennik 19), zamiast zdania agenta.
Rust ma to w całości: `Step::Check(CheckStep { command, proof, folder, when_it_fails })`
(`workflow/mod.rs`), walidator odmawia zapisu bez `proof` (`check.rs`, `a_command_step_left_empty`),
sterownik `drivers/command.rs` liczy `passed = exit_code == Some(0) && proof_matches(...)`,
a `run_check` oddaje wyjście komendy jako przekazanie i werdykt do tras warunkowych i pętli.

**Okno tego nie ma.** Unia `Step` w `src/state/workflows.ts` świadomie pomija `check`
(„przychodzi wyłącznie z zaimportowanych plików"), płótno rysuje taki kafelek (`canvas.tsx`,
chip „checks project"), ale nie ma przycisku, który by go postawił, a `PanelForStep`
(`step-panel/panel.tsx`) nie ma dla niego gałęzi — klik wpada w „wybierz agenta". `proof`
nie da się wpisać nigdzie.

Skutek jest większy, niż wygląda: bez tego kafelka **każda** pętla, jaką człowiek zbuduje, jest
pętlą „co agent powiedział" (sędzia-agent z `outcome:`), a rozróżnienie z `00-SYNTHESIS.md` §2.1 —
jedyny powód istnienia produktu — nie ma na płótnie żadnego nośnika. Właściciel zbudował
dziesięć biegów z pętlami weryfikacyjnymi i w żadnym nie mógł użyć sprawdzenia, bo nie było
czym.

Wzorcem jest kafelek `serve` (T-73/T-75): `freshStep` w `canvas/connect.ts`, własny panel
`serve-panel.tsx`, gałąź w `editor.tsx` i w `PanelForStep`, test `start-and-leave-has-a-panel`.
Zrób dla `check` dokładnie to samo, z dwoma polami więcej.

**Read first:** `src/state/workflows.ts` (unia `Step`, komentarz o `check`) ·
`src/sections/workflows/canvas/connect.ts` (`addStep`, `freshStep`) ·
`src/sections/workflows/step-panel/serve-panel.tsx` i `panel.tsx` (`PanelForStep`) ·
`src/sections/workflows/editor.tsx` (gałęzie `serve`/`checkpoint` przy zapisie pól) ·
`src/sections/workflows/every-tile-opens-a-panel.test.tsx` (to kryterium **musi** dalej
przechodzić z nowym rodzajem) · `src-tauri/src/workflow/mod.rs` (`CheckStep`, pola i domyślne) ·
`src-tauri/src/workflow/check.rs` (`a_command_step_left_empty`, `DIGIT_RUN` — jedyny metaznak
w `proof`) · `docs/DECISIONS-LOCKED.md` D6 („Trzeci rodzaj: sprawdź") · `AGENTS.md`
niezmienniki 14, 16, 29.

Słownictwo na ekranie: „Run a check", „Command to run", „Counts as passed when the output
contains", „Where it runs". Nigdy „proof", „verdict", „gate".

## Kto to robi

- **Agent:** `frontend`
- **Druga opinia:** inny vendor niż pisarz (D3).

## AC-1 Przycisk stawia kafelek sprawdzenia z pustą komendą
check: npx --no-install vitest run src/sections/workflows/canvas/check-tile-can-be-placed.test.ts
expect: (\d+) passed

`addStep('check', file)` dodaje krok rodzaju `check` z pustą komendą, pustym wzorcem, domyślnym
`folder: same-copy` (ten sam powód, co przy `serve`: sprawdzenie stawia się po kroku, który coś
napisał) i `whenItFails: stop`. Unia `Step` w `src/state/workflows.ts` zna `check` z polami
`command`, `proof`, `folder`, `whenItFails`; zapis i odczyt pliku nie gubią żadnego z nich.

## AC-2 Panel sprawdzenia edytuje komendę, wzorzec, folder i co po porażce
check: npx --no-install vitest run src/sections/workflows/step-panel/check-panel-edits-every-field.test.tsx
expect: (\d+) passed

`PanelForStep` na kroku `check` renderuje panel z nazwą, komendą, wzorcem przejścia (z jednym
zdaniem pomocy: liczba we wzorcu to `(\d+)`, np. `(\d+) passed`), wyborem folderu
(`same-copy` / `project` / `fresh-copy`) i wyborem „If this check does not pass". Każda zmiana
przechodzi przez `onChange` z właściwym kluczem. Kryterium renderuje prawdziwy markup
(`renderToStaticMarkup`) i woła handlery wprost — tak jak `start-and-leave-has-a-panel`.

## AC-3 Zapis bez wzorca jest odmową z nazwą pola, nie cichym plikiem
check: npx --no-install vitest run src/state/workflows-check-step-needs-a-pattern.test.ts
expect: (\d+) passed

Stan edytora pokazuje problem zwrócony przez walidację Rusta dla kroku `check` bez `proof`
(zdanie nazywa krok i mówi, czego brakuje), a kafelek ma czerwoną kropkę — tą samą drogą, którą
dziś dostaje ją `serve` bez poprzednika (`problems.tsx`). Z `vi.mock` na module IPC, jak
w innych testach stanu.

## AC-4 Kafelek stawia się po prawdziwym kliknięciu
check: npx --no-install vitest run e2e/tests/check-tile-placed-by-a-click.spec.ts
expect: (\d+) passed

W prawdziwym oknie (`e2e/harness.ts`) człowiek otwiera workflow, klika przycisk dodania
sprawdzenia, widzi nowy kafelek z chipem mówiącym, że to sprawdzenie, klika go i widzi panel
z polem komendy. Kryterium nie woła żadnej funkcji stanu; jedynym wejściem jest kliknięcie.
Jeśli `e2e/harness.ts` nie umie dojść do płótna bez zmiany w sobie — **stój i zgłoś**
(`AGENTS.md` §7), nie rozszerzaj OWNS.

<!-- OWNS
tasks/T-89.md
src/state/workflows.ts
src/sections/workflows/canvas/connect.ts
src/sections/workflows/canvas/canvas.tsx
src/sections/workflows/canvas/map.ts
src/sections/workflows/canvas/tile.tsx
src/sections/workflows/canvas/check-tile-can-be-placed.test.ts
src/sections/workflows/step-panel/panel.tsx
src/sections/workflows/step-panel/check-panel.tsx
src/sections/workflows/step-panel/check-panel-edits-every-field.test.tsx
src/sections/workflows/editor.tsx
src/sections/workflows/index.tsx
src/state/workflows-check-step-needs-a-pattern.test.ts
e2e/tests/check-tile-placed-by-a-click.spec.ts
-->
