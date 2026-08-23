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

## Poszerzenie zakresu — 2026-08-23, przeoczenie w bloku OWNS

`src-tauri/commands.golden.txt` dochodzi do OWNS, bo bez niego **AC-2 jest niewykonalne**, a nie
trudne. Nowa komenda musi tam mieć wiersz: `ipc_commands_registered.rs` asertuje RÓWNOŚĆ ZBIORÓW
między tym plikiem a `generate_handler!`, więc rejestracja bez wiersza jest czerwienią, a wiersz
bez rejestracji — martwą kontrolką (niezmiennik 16). Ten sam plik mają w OWNS wszystkie zadania,
które kiedykolwiek dokładały komendę (T-27, T-29, T-30, T-34, T-38, T-40, T-41, T-42, T-43, T-44);
jego brak tutaj był moim przeoczeniem przy pisaniu kontraktu, nie decyzją. Kryteria bez zmian —
dochodzi wyłącznie ścieżka.

### Dwa cudze pliki testowe — mandat na MIEJSCE ODCZYTU, nigdy na zdanie

*Decyzja właściciela 2026-08-23, po pierwszym biegu tego zadania.*

Sprzątanie z AC-1 przewraca trzy asercje w dwóch plikach, których to zadanie nie miało:
`trigger_run_is_accepted_once.rs` (**kryterium wylądowanego T-65**, ścieżka triggerów) oraz
`continue_from_a_past_run.rs` (test regresyjny, żadne kryterium go nie woła). Obie sprawy mają
ten sam kształt i to jest powód, dla którego wolno je ruszyć:

**Zdania tych asercji zostają prawdziwe.** „retry preserved HEAD but lost the human's dirty
tracked content" mówi o tym, co ponowienie **włożyło** do drzewa — a to leży teraz na gałęzi.
„retry registered zero or two worktrees" mówi o tym, że rejestracja w trakcie biegu była
**dokładnie jedna** — i nadal jest. Nieprawdziwe robi się wyłącznie **miejsce i chwila
obserwacji**: oba instrumenty czytają katalog PO biegu, żeby wnioskować o tym, co działo się
W TRAKCIE, a AC-1 kasuje ten katalog z założenia.

Wolno ci więc **wyłącznie przenieść obserwację tam, gdzie fakt nadal jest**: treść pliku czytać
z gałęzi (`git show <gałąź>:<ścieżka>`), a rejestrację drzewa liczyć w chwili, w której drzewo
jeszcze stoi. **Każde zdanie asercji zostaje słowo w słowo.**

Czego NIE WOLNO, bo każde z tego przechodzi bramkę i kasuje sens tamtych kryteriów:
skasować asercji; zamienić `assert_eq!(count, 1)` na porównanie, które przechodzi także przy
zerze i przy dwóch; zmienić komunikat asercji; „naprawić" test przez wyłączenie sprzątania
w tamtym scenariuszu. Reszta obu plików jest cudza — nie dopisuj tam asercji i nie tykaj
pozostałych testów.

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
src-tauri/commands.golden.txt
src-tauri/tests/it/main.rs
src-tauri/tests/it/finished_runs_leave_no_work_trees.rs
src-tauri/tests/it/same_copy_pairs_are_judged.rs
src-tauri/tests/it/trigger_run_is_accepted_once.rs
src-tauri/tests/it/continue_from_a_past_run.rs
src/sections/commands-wired.test.ts
src/ipc/run.ts
src/sections/run/io.ts
src/sections/run/past/panel.tsx
src/sections/run/past/store.ts
src/sections/run/past/branches-can-be-dropped.test.tsx
-->
