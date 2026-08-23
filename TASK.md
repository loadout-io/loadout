# T-95 — Po biegu nie zostają kopie ani gałęzie bez pracy

`isolate::finish` robi połowę roboty: krok, który **nic** nie zmienił, traci drzewo i gałąź.
Krok, który zmienił cokolwiek, dostaje commit na `loadout/<bieg>/<kafelek>` — i **drzewo zostaje
na dysku**, pod `<projekt>/.loadout/runs/<bieg>/work/<kafelek>/`, razem z pełnym checkoutem
repozytorium. Zmierzone u właściciela 2026-08-23: dziesięć biegów „Deep reaserch" na
`urc-monorepo` zostawiło kilkadziesiąt katalogów `work/s_*` z pełną kopią monorepo każdy, dla
zadania o mieszkaniach w Gdańsku, które nie dotyka repo — bo `look-only` nie znaczy „nie
zapisze zrzutu ekranu", a jeden nowy plik to „dirty". Gałęzie zostają na zawsze; nic ich nie
listuje, nic nie umie ich zdjąć poza ręcznym `git branch -D`.

Obietnica z T-52 brzmi: praca jest po biegu **osiągalna z gita**. Gałąź ją spełnia; katalog
nie dokłada nic poza miejscem na dysku i wpisem w `git worktree list`. Drugi brak jest
w walidatorze: `check.rs` (`one_folder_two_steps`) przyznaje wprost, że pary z `same-copy`
„wpadają tu bez odpowiedzi" — dwa kroki na tej samej kopii, gotowe naraz, to dokładnie kolizja
z niezmiennika 12, i dziś nikt jej nie sądzi.

**Read first:** `src-tauri/src/commands/isolate.rs` (`finish`, `Kept`, `branch_for`,
`names_a_commit`, `make_from` — `git worktree add`) · `src-tauri/src/commands/run.rs`
(`close_the_trees`, `where_it_left_off` — wznowienie bierze gałąź, nie katalog; `.isolation/`
markery) · `src-tauri/src/commands/history.rs` (`list_runs_inner`, `read_run_inner`, `RunWire`) ·
`src-tauri/src/workflow/check.rs` (`one_folder_two_steps`, `the_same_files`, komentarz
o `same-copy`, `trees_before` w `run.rs` — jak `same-copy` rozwiązuje się na drzewo) ·
`src/sections/run/past/panel.tsx`, `store.ts` · `src/sections/commands-wired.test.ts` ·
`.claude/settings.json` — `Bash(git worktree remove:*)` jest na liście deny **dla agenta
w pętli**; produkt woła gita przez `Command`, nie przez Bash, więc to nie dotyczy kodu, ale
testy nie mogą tego robić przez powłokę z `sh -c` · `AGENTS.md` niezmienniki 4, 6, 12.

## Kto to robi

- **Agent:** `rust-core` na AC-1, AC-3, potem `frontend` na AC-2 — jeden worktree, jedna bramka.
- **Druga opinia:** inny vendor niż pisarz (D3).

## AC-1 Po biegu praca jest na gałęzi, a katalogu roboczego nie ma
check: cargo test --test it finished_runs_leave_no_work_trees::
expect: (\d+) passed

Po `close_the_trees` biegu w repozytorium: dla kroku, który zmienił pliki, istnieje commit na
`loadout/<bieg>/<kafelek>` z pełną pracą (także nowe pliki), a `work/<kafelek>/` **nie istnieje**
i nie ma go w `git worktree list`. Dla kroku bez zmian — jak dziś, nic. Jeśli commit się nie
uda, katalog zostaje (jedyna operacja, która mogłaby stracić pracę, nadal jej nie traci) i bieg
dostaje jedno zdanie o tym w `run.json` kroku. Wznowienie (`where_it_left_off`) działa z samej
gałęzi — kryterium wznawia krok po sprzątaniu i sprawdza, że widzi plik z poprzedniego biegu.
Projekt bez repozytorium (kopia plikowa) nie jest sprzątany — tam katalog **jest** pracą.

## AC-2 Historia pokazuje gałęzie biegu i umie je zdjąć
check: npx --no-install vitest run src/sections/run/past/branches-can-be-dropped.test.tsx
expect: (\d+) passed

Panel biegu w historii listuje gałęzie, które ten bieg zostawił (z `read_run`: nazwa i krok),
oraz ma przycisk „Forget the branches" wołający `forget_run_branches(folder, run)`. Rust zdejmuje
wyłącznie gałęzie o prefiksie `loadout/<ten bieg>/`, odmawia ze zdaniem, jeśli któraś jest
w `git worktree list` (ktoś na niej pracuje), i zwraca listę zdjętych; po odpowiedzi panel
pokazuje „No branches left". Bieg bez gałęzi nie ma przycisku. Nowa komenda w lustrze komend.

## AC-3 Dwa kroki na tej samej kopii, gotowe naraz, są odmową przed Startem
check: cargo test --test it same_copy_pairs_are_judged::
expect: (\d+) passed

Walidator rozwiązuje `same-copy` na drzewo poprzednika (tą samą regułą, co `trees_before`
w biegu) i sądzi pary tak jak `project`/`project`: dwa kroki `same-copy` po tym samym
`fresh-copy`, nieosiągalne wzajemnie, to `warning` przy zapisie i `problem` przy Run ze zdaniem
nazywającym oba kroki. Łańcuch `fresh-copy → same-copy → same-copy` (każdy po poprzednim)
pozostaje poprawny. Komentarz „wpada tu bez odpowiedzi" znika razem z tym zadaniem.

## Sprzątanie po drodze

`history.rs` ok. linii 176 twierdzi, że `logs/agent-<step>.jsonl` nie jest produkowany przez
żaden bieg — jest (`evidence.rs`, T-34); popraw, plik jest w OWNS.

<!-- OWNS
tasks/T-95.md
src-tauri/src/commands/isolate.rs
src-tauri/src/commands/run.rs
src-tauri/src/commands/history.rs
src-tauri/src/commands/mod.rs
src-tauri/src/workflow/check.rs
src-tauri/src/ipc.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/finished_runs_leave_no_work_trees.rs
src-tauri/tests/it/same_copy_pairs_are_judged.rs
src/sections/commands-wired.test.ts
src/ipc/run.ts
src/sections/run/io.ts
src/sections/run/past/panel.tsx
src/sections/run/past/store.ts
src/sections/run/past/branches-can-be-dropped.test.tsx
-->
