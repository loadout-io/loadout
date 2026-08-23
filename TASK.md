# T-93 — Dziedziczenie z repo gospodarza ma nośnik

`src-tauri/src/inherit/` umie wszystko, co obiecuje: skanuje `<projekt>/.claude/skills/`,
`.claude/learnings/<rola>.md` (sekcja `## Recurring patterns`) i `.claude/agents/<rola>.md`
(samo ciało), przepisuje skille do katalogu pluginu biegu, składa `--plugin-dir` i tekst do
promptu (`Inherited::applied_to`). Wszystko to jest przetestowane (`inherit_*` w `tests/it/`).

I wszystko to dostaje **zawsze pusty wybór**: `what_the_host_lends` w `commands/run.rs` woła
`from_the_host(project, run_dir, &Chosen::default())`, a komentarz obok mówi wprost, że brakuje
jednej rzeczy — pola, którym ekran powie, co człowiek zaznaczył. `Chosen { skills, learnings,
subagent }` istnieje; nie ma go tylko gdzie wpisać.

Skutek dla właściciela: jego `.claude/learnings/` w `urc-monorepo` (pisane przez własny krok
„Learings" i agenta `learnings-extractor`) jest jedynym realnym zasobem learningów, jaki ma —
i Loadout go nie czyta. `inherit_is_opt_in.rs` pilnuje, żeby to było opt-in, i to zostaje:
nośnikiem jest **krok**, nie bieg, bo wybór „pożycz rolę `backend-dev` z tego repo" jest
własnością kafelka tak samo jak wybór agenta.

**Read first:** `src-tauri/src/inherit/wire.rs` (`Chosen`, `from_the_host`, `Error::NotInTheHost`)
· `src-tauri/src/inherit/scan.rs` (`skills`, `recurring_patterns`, `agent_body`) ·
`src-tauri/src/commands/run.rs` (`what_the_host_lends`, `carrying_what_we_inherited`, `plan_agent`)
· `src-tauri/src/workflow/mod.rs` (`AgentStep` — nowy klucz wchodzi obok `skills`, z `#[serde(default,
skip_serializing_if)]`, żeby stare pliki czytały się bez zmian) · `src-tauri/src/workflow/check.rs`
(walidacja przy zapisie: wzór `skills_that_do_not_exist`, jeśli jest; nazwa spoza skanu to
`Problem` przy Run, `Warning` przy zapisie — skan gospodarza zależy od folderu, którego plik
workflow nie zna) · `src/sections/workflows/step-panel/skills-row.tsx` (wzór wiersza z listą
wyboru) · `src/sections/commands-wired.test.ts` · `tests/it/inherit_is_opt_in.rs` (ma dalej
przechodzić).

## Kto to robi

- **Agent:** `rust-core` na AC-1, AC-2, potem `frontend` na AC-3 — jeden worktree, jedna bramka.
- **Druga opinia:** inny vendor niż pisarz (D3).

## AC-1 Krok z wyborem dostaje to, co wybrał, i nic więcej
check: cargo test --test it a_step_borrows_from_the_host::
expect: (\d+) passed

`AgentStep` ma klucz `borrow` (`{ skills: [...], learnings: "<rola>", agent: "<rola>" }`,
każde pole opcjonalne). Krok z `borrow.learnings = "backend-dev"` dostaje w prompcie sekcję
`## Recurring patterns` z `<projekt>/.claude/learnings/backend-dev.md`, a krok z
`borrow.skills = ["x"]` dostaje `--plugin-dir` z przepisanym skillem. Krok **bez** `borrow`
ma argv i prompt co do bajtu jak dziś (to jest `inherit_is_opt_in`, powtórzone w tym samym
kryterium jako kontrola). Dwa kroki w jednym biegu z różnymi wyborami dostają różne rzeczy.

## AC-2 Nazwa, której gospodarz nie ma, zatrzymuje bieg przed pierwszym procesem
check: cargo test --test it borrowing_what_is_not_there_refuses::
expect: (\d+) passed

`borrow.learnings = "nobody"` przy braku `<projekt>/.claude/learnings/nobody.md` jest
`RunError::Refused` ze zdaniem nazywającym rolę i folder — przed utworzeniem katalogu biegu,
jak `skills_missing_stops_the_run`. Folder bez `.claude/` przy pustym `borrow` nie jest błędem.
Agent Codex z niepustym `borrow.skills` jest odmową z nazwą vendora (Codex nie ma `inheriting`),
a z samym `borrow.learnings` — nie, bo tekst do promptu nie potrzebuje vendora.

## AC-3 Panel kroku pokazuje, co ten folder ma do pożyczenia
check: npx --no-install vitest run src/sections/workflows/step-panel/borrow-row-lists-the-host.test.tsx
expect: (\d+) passed

Panel kroku agenta ma wiersz „Borrow from this project" z listą tego, co komenda
`list_host_material(folder)` znalazła w aktywnym workspace (skille, role z learnings, podagenci),
z checkboxami zapisującymi się do `borrow` kroku. Folder bez `.claude/` → wiersz nie renderuje
się wcale (kontrolka bez skutku nie wchodzi, niezmiennik 16). Krok z zapisanym `borrow`, którego
folder już nie ma, pokazuje nazwę jako „not in this folder" zamiast ją cicho zdejmować. Nowa
komenda ma wiersz w lustrze komend.

<!-- OWNS
tasks/T-93.md
src-tauri/src/inherit/mod.rs
src-tauri/src/inherit/wire.rs
src-tauri/src/inherit/scan.rs
src-tauri/src/workflow/mod.rs
src-tauri/src/workflow/check.rs
src-tauri/src/commands/run.rs
src-tauri/src/commands/mod.rs
src-tauri/src/ipc.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/a_step_borrows_from_the_host.rs
src-tauri/tests/it/borrowing_what_is_not_there_refuses.rs
src/sections/commands-wired.test.ts
src/state/workflows.ts
src/sections/workflows/step-panel/panel.tsx
src/sections/workflows/step-panel/borrow-row.tsx
src/sections/workflows/step-panel/borrow-row-lists-the-host.test.tsx
src/sections/workflows/io.ts
-->
