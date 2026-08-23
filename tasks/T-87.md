# T-87 — Pętla pamięta swoje rundy, a fan-in dostaje to, co przeszło

Pętla `max_turns` jest rozwijana **przed** biegiem na literalne rundy (`workflow/unroll.rs`):
`Research → Verify` z trzema próbami to sześć kroków `s_2, s_7, s_2#1, s_7#1, s_2#2, s_7#2`.
Każda runda to nowy proces, nowa sesja, nowy worktree. Kontekst kroku buduje `Live::handed_before`
(`commands/run.rs`), które bierze **wyłącznie bezpośrednich poprzedników po strzałce** w rozwiniętym
grafie — a jedynym poprzednikiem `s_2#1` jest sędzia `s_7`.

Zmierzone w biegu `20260823-145648` (`~/Projects/urc-monorepo/.loadout/runs/`, pliki
`logs/*.input.json`):

| krok | co dostał |
|---|---|
| `s_2` | plan (`00__plan-steps`) |
| `s_2#1` | **tylko** `12__verification-1` |
| `s_2#2` | **tylko** `13__verification-1` |
| `s_5` (synteza, trzy strzałki wchodzące) | `14__verification-1`, `20__verification-3` — **zero researchu** |

Czyli: agent poprawiający w rundzie 2 nie widzi własnej rundy 1, którą ma poprawić, ani planu,
od którego zaczął; sędzia w rundzie 2 nie widzi, co zarzucił w rundzie 1; a synteza dostała dwie
negatywne krytyki i nic z gałęzi, która **przeszła** — bo `s_8#1` dał `pass`, `s_8#2` został
`NOT_NEEDED` i **nie oddał przekazania** (`already_settled` w `run.rs`). Skutek: w czterech
biegach dwie z trzech pętli nie zbiegły się ani razu (9 rund, 0 przejść), a produkt biegu
(Design, Implementation) powstał na syntezie, która widziała same odmowy.

To zadanie **nie zmienia rozwijania pętli ani cyklu życia procesu** (ta sama sesja przez wiele
rund to osobna decyzja, nie ta faza). Zmienia wyłącznie to, **co runda widzi** i **co pętla
oddaje dalej**.

**Read first:** `src-tauri/src/workflow/unroll.rs` (`Unrolled { nodes, arrows, loops }`,
`Node { step, turn }`, `Loop` — kto jest sędzią, kto wejściem, ile rund; to jest jedyne źródło
prawdy o przynależności do pętli) · `src-tauri/src/commands/run.rs` (`handed_before`,
`prompt_for`, `hand_over`, `filed`, `judging`, `settle`, `already_settled`, `verdict_after`,
`when_this_one_fails`, `run_check`, `run_agent` — gałąź `Ended::Turn(Err(_))`,
`stop_overdue_agent`) · `tasks/T-86.md` (blok „jak odpowiadać" — indeks z etykietami stoi
**przed** nim) · `AGENTS.md` niezmienniki 14, 27.

## Kto to robi

- **Agent:** `rust-core`
- **Druga opinia:** inny vendor niż pisarz (D3).

## Zanim napiszesz pierwszą specyfikację

`cargo test --test it <modul>::` plus linia `mod` w `tests/it/main.rs`. Do grafu z pętlą użyj
tego samego pliku, co `runcmd_loop.rs` i `workflow_loop_back_edge.rs`; do zatrzymania promptu
`FakeDriver`. Żadne z kryteriów nie czyta transkryptu.

## AC-1 Runda k+1 widzi własną rundę k i wejście pętli
check: cargo test --test it a_round_sees_its_own_past::
expect: (\d+) passed

Krok `s_2#1` dostaje w indeksie przekazań, w tej kolejności: to, co dostał `s_2` (wejście pętli —
np. plan), przekazanie `s_2` (własna poprzednia odpowiedź), przekazanie sędziego `s_7`.
Krok `s_2#2` dostaje wejście pętli, `s_2` **i** `s_2#1`, oraz `s_7` **i** `s_7#1`. Kolejność
jest odtwarzalna (numer kroku, potem runda), nie zależy od czasu zakończenia. Krok spoza pętli
dostaje dokładnie to, co dziś — kryterium ma kontrolę na kroku bez powrotu.

## AC-2 Sędzia widzi swoje poprzednie werdykty
check: cargo test --test it the_tester_remembers_what_it_said::
expect: (\d+) passed

`s_7#1` dostaje przekazanie `s_2#1` (to, co ocenia) **i** własne przekazanie z `s_7`. `s_7#2`
dostaje `s_2#2`, `s_7` i `s_7#1`. Sędzia, który w rundzie 1 zgłosił trzy zarzuty, w rundzie 2
ma je przed sobą — kryterium sprawdza obecność ścieżki w indeksie, nie treść zarzutów.

## AC-3 Pętla, która przeszła, oddaje do fan-inu swoje ostatnie przekazanie
check: cargo test --test it a_passed_loop_reaches_the_next_step::
expect: (\d+) passed

Kiedy sędzia rundy k mówi `outcome: pass`, rundy po k są pomijane bez sterownika (tak jak dziś),
ale krok **za** pętlą dostaje w indeksie ostatnie **wyprodukowane** przekazanie każdej gałęzi:
przekazanie pracy z rundy k i przekazanie sędziego z rundy k. Przy trzech gałęziach wchodzących
do syntezy, z których jedna przeszła w rundzie 1, druga w rundzie 2, a trzecia padła w rundzie 3
z `carry-on`, synteza dostaje **sześć** ścieżek (praca + werdykt z każdej gałęzi), nie dwie.
Kontrola: dzisiejsze zachowanie (synteza widzi tylko rundę ostatnią) ma być czerwone.

## AC-4 Indeks mówi, czym jest każdy plik
check: cargo test --test it the_index_says_what_each_file_is::
expect: (\d+) passed

Każdy wiersz indeksu niesie krótką etykietę po angielsku, z zamkniętej listy, np.
„what you were given at the start", „your own earlier answer (try 1 of 3)", „what the tester
said last time (try 1 of 3)", „what the step before left". Etykiety są stałymi w jednym miejscu
obok `HANDOFF_INDEX_OPENS`, bez słów z drutu (niezmiennik 14: nie „handoff", nie „verdict",
nie „judge", nie „loop"). Krok spoza pętli widzi tylko ostatnią z tych etykiet.

## AC-5 Każda porażka przechodzi przez jedno miejsce i zostawia po sobie to, co agent zdążył powiedzieć
check: cargo test --test it every_failure_leaves_its_last_words::
expect: (\d+) passed

Dziś trzy ścieżki porażki omijają `when_this_one_fails` (błąd startu komendy w `run_check`,
`CheckHow::Overdue`, `Ended::Turn(Err(_))` w `run_agent`) — tam `carry-on` i `ask-me` nie
działają. Po tym zadaniu każda z nich przechodzi przez tę samą funkcję, a krok, który padł
z `carry-on`, **oddaje przekazanie z tym, co zdążył powiedzieć** (może być puste), oznaczone
w indeksie następnego kroku etykietą mówiącą, że ten krok nie przeszedł. Następny krok nie
dostaje więc milczącej luki w indeksie, tylko wiersz „the step before did not pass; this is
what it said". Kontrola: krok z `stop` nadal nie oddaje nic dalej, bo nic po nim nie biegnie.

## Sprzątanie po drodze

`run.rs` ok. linii 124 mówi „Nie rozwija `copies`" — to dalej prawda do T-90; nie ruszaj.
Zdanie `OUTCOME_ASKED_FOR` zostaje tam, gdzie jest; indeks z etykietami stoi przed blokiem
z T-86, a zdanie sędziego po nim.

<!-- OWNS
tasks/T-87.md
src-tauri/src/commands/run.rs
src-tauri/src/workflow/unroll.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/a_round_sees_its_own_past.rs
src-tauri/tests/it/the_tester_remembers_what_it_said.rs
src-tauri/tests/it/a_passed_loop_reaches_the_next_step.rs
src-tauri/tests/it/the_index_says_what_each_file_is.rs
src-tauri/tests/it/every_failure_leaves_its_last_words.rs
-->
