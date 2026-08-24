# T-100 — Werdykt jest polem, sędzia widzi próby

Nośnikiem wyniku pętli jest dziś jedna literalna linia prozy: `outcome: pass` / `outcome:
fail`, a brak linii czyta się jako fail. Zmierzone na biegach właściciela: 21× fail, 3× pass;
w biegu `20260823-011240` sędzia napisał „## Werdykt: **PASS** … przyjąć" i run.json dostał
`failed` — pod nim zginął cały produkt biegu. T-86 dodał zdanie z prośbą (`OUTCOME_ASKED_FOR`),
ale mechanizm dalej stoi na jednej linii w prozie modelu piszącego po polsku.

Repo ma już mocniejszy nośnik: pola odpowiedzi (`FIELDS_ASKED_FOR` / `FIELDS_ARE_REQUIRED`,
czytane z odpowiedzi i sądzone jako `evidence_complete` — T-90 AC-4,
`required_fields_are_required.rs`). Werdykt przesiada się na ten nośnik; linia w prozie
zostaje fallbackiem, żeby stare workflow nie zmieniły zachowania.

Druga połowa: sędzia rundy k dostaje dziś w indeksie pracę rundy k, wejście pętli i WŁASNE
wcześniejsze werdykty — ale nie wcześniejsze próby implementera (`run.rs` ok. 7764–7767).
Nie umie więc odróżnić „poprawił dokładnie to" od „ten sam błąd inaczej opisany" — a o to
prosi instrukcja właściciela w każdym jego workflow.

**Read first:** `src-tauri/src/commands/run.rs` (`OUTCOME_ASKED_FOR` ok. 439,
`ask_for_an_outcome` ok. 7569, `verdict_after` ok. 5749, `what_this_try_already_knows`
ok. 7723, `fields`/`evidence_complete` w `one_turn`) · `src-tauri/src/memory/handoff.rs`
(`verdict_in` ok. 201, `said_an_outcome`) · `src-tauri/tests/it/the_tester_remembers_what_it_said.rs`,
`a_passed_loop_reaches_the_next_step.rs`, `required_fields_are_required.rs` (istniejące
wyrocznie — rozszerz, nie dubluj) · `docs/PLAN-HARDENING.md` §3.

## Kto to robi

- **Agent:** `rust-core`. Po T-99 (wspólny `run.rs`).
- **Druga opinia:** inny vendor niż pisarz (D3).

## AC-1 Sędzia dostaje pole `outcome` jako wymagane
check: cargo test --test it the_tester_gets_an_outcome_field::
expect: (\d+) passed

Prompt kafelka zamykającego pętlę zawiera blok pól z pozycją `outcome` oznaczoną jako
wymaganą (wartości `pass` / `fail`), niezależnie od ustawienia `handover` kafelka;
`OUTCOME_ASKED_FOR` zostaje po nim jako zdanie o fallbacku. Kryterium porównuje ZMONTOWANY
tekst promptu (jak T-86 AC-1), nie stałą. Kontrola: krok niebędący sędzią bloku `outcome`
nie dostaje.

## AC-2 Pole `outcome` rozstrzyga rundy, linia w prozie zostaje zapasem
check: cargo test --test it an_outcome_field_settles_the_rounds::
expect: (\d+) passed

Odpowiedź sędziego z polem `outcome: pass` kończy pętlę (dalsze rundy jak dziś przy
`pass` z prozy); pole `fail` w ostatniej rundzie idzie przez `when_this_one_fails`.
Przy braku pola działa dzisiejsza linia z prozy (kontrola na obu kierunkach); brak obu
w rundzie nieostatniej = fail jak dziś. Pole wygrywa z linią, kiedy się różnią —
i ta preferencja stoi w komentarzu przy kodzie.

## AC-3 Sędzia widzi każdą wcześniejszą próbę
check: cargo test --test it the_tester_sees_every_earlier_try::
expect: (\d+) passed

Sędzia rundy k dostaje w indeksie, poza tym co dziś, wszystkie wcześniejsze próby
implementera (`try 1 of N` …) z istniejącymi etykietami. Kolejność odtwarzalna (numer
kroku, potem runda). Kontrola: sędzia rundy 0 i kroki poza pętlą — indeks co do bajta
jak dziś.

## AC-4 run.json pamięta, co powiedział sprawdzający w każdej rundzie
check: cargo test --test it run_json_records_what_the_tester_said::
expect: (\d+) passed

Wiersz kroku-sędziego w `run.json` dostaje addytywne pole z rozstrzygnięciem rundy
(`#[serde(default)]`, `skip_serializing_if` przy braku — stare pliki czytają się bez
zmian, `store::rebuild` ich nie traci). Runda nieostatnia, która nie przepuściła, ma
w tym polu odmowę, choć stan kroku pozostaje jak dziś — to jest nośnik dla przyszłego
ekranu, nie zmiana maszyny stanów.

<!-- OWNS
tasks/T-100.md
src-tauri/src/commands/run.rs
src-tauri/src/memory/handoff.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/the_tester_gets_an_outcome_field.rs
src-tauri/tests/it/an_outcome_field_settles_the_rounds.rs
src-tauri/tests/it/the_tester_sees_every_earlier_try.rs
src-tauri/tests/it/run_json_records_what_the_tester_said.rs
-->
