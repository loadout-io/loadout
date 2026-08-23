# T-88 — „Pick up here" niesie przekazania poprzedniego biegu

`commands/rerun.rs` ma dwa czasowniki: `again` (powtórz jeden kafelek, `Part::Just`) i `onward`
(wznów od kafelka i dalej, `Part::Onward`). Oba ustawiają `handoffs_from: Some(<stary bieg>)`,
a `seed_the_handoffs` (`commands/run.rs`) kopiuje pliki z `<stary>/handoffs/` do
`<nowy>/handoffs/`. Nagłówek `rerun.rs` mówi: „Wejściem są przekazania poprzedniego biegu".

**I tu kontrakt się urywa.** Indeks promptu buduje `handed_before` wyłącznie z `Live::handoffs`,
czyli z tego, co kroki **tego** biegu zdążyły oddać (`filed`). Zasiane pliki nigdy tam nie
trafiają. Do tego `Part::Just` zeruje wszystkie strzałki, a `Part::Onward` zostawia tylko te
z obydwoma końcami w wycinku — więc głowa wycinka nie ma poprzedników, a `prompt_for` wychodzi
wcześniej bez indeksu. `attachments/` nie jest kopiowane wcale.

Skutek: wznowiony krok dostaje gałąź gita z `where_it_left_off` (pliki pracy są), ale **zero
przekazań** w prompcie. Krok „Synteza" wznowiony po naprawie sędziego nie widzi ani jednego
researchu. Przycisk „Pick up here" w historii (`src/sections/run/past/pick-up.ts`) obiecuje
coś, czego bieg nie robi.

**Read first:** `src-tauri/src/commands/rerun.rs` (nagłówek, `again`, `onward`) ·
`src-tauri/src/commands/run.rs` (`seed_the_handoffs`, `Plan::seeded_from`, `Part`,
`plan_run_with_identity` — gdzie zeruje strzałki; `handed_before`, `filed`, `prompt_for`,
`extra_dirs` dla `attachments/`) · `src-tauri/src/memory/handoff.rs` (`scan_run_dir`,
front-matter `from`/`step` — po nich rozpoznasz, który plik jest czyj) · `tasks/T-87.md`
(etykiety w indeksie — przekazanie z poprzedniego biegu dostaje własną).

## Kto to robi

- **Agent:** `rust-core`
- **Druga opinia:** inny vendor niż pisarz (D3).

## Zanim napiszesz pierwszą specyfikację

`cargo test --test it <modul>::` plus `mod` w `tests/it/main.rs`. Wzorcem jest
`resume_starts_from_the_work_that_was_done.rs` i `continue_from_a_past_run.rs` — tam jest
gotowy sposób na zbudowanie „starego biegu" na dysku bez prawdziwego agenta.

## AC-1 Wznowiony krok dostaje przekazania swoich poprzedników ze starego biegu
check: cargo test --test it resume_carries_the_earlier_handoffs::
expect: (\d+) passed

Bieg `A → B → C` przeszedł do `B` i padł na `C`. `onward` od `C` daje nowy bieg, w którym prompt
`C` ma w indeksie przekazania `A` i `B` **z poprzedniego biegu**, z etykietą mówiącą, że pochodzą
z wcześniejszego biegu. Ścieżki wskazują na kopie w katalogu **nowego** biegu (stary bieg jest
niezmienny), a katalog jest w `extra_dirs`. Kryterium sprawdza zmontowany prompt przez
`FakeDriver`, nie zawartość katalogu.

## AC-2 Powtórzony kafelek dostaje to, co dostał za pierwszym razem
check: cargo test --test it again_carries_what_the_step_had::
expect: (\d+) passed

`again` na `B` daje bieg z jednym kafelkiem, którego prompt ma w indeksie przekazanie `A`
ze starego biegu — dokładnie ten zbiór, który `B` widział pierwotnie (po `reads:` z front-mattera
przekazania `B`, jeśli istnieje; inaczej po strzałkach ze snapshotu workflow w `run.json`).
Kafelek bez poprzedników nadal nie dostaje indeksu.

## AC-3 Załączniki jadą razem z przekazaniami
check: cargo test --test it resume_carries_the_attachments::
expect: (\d+) passed

Kiedy stary bieg ma `attachments/<stem>__full.md`, nowy bieg dostaje ich kopie obok
skopiowanych przekazań, a wskaźnik `Moved to attachments/…` w ciele przekazania rozwiązuje się
w nowym katalogu. Brak `attachments/` w starym biegu nie jest błędem.

<!-- OWNS
tasks/T-88.md
src-tauri/src/commands/run.rs
src-tauri/src/commands/rerun.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/resume_carries_the_earlier_handoffs.rs
src-tauri/tests/it/again_carries_what_the_step_had.rs
src-tauri/tests/it/resume_carries_the_attachments.rs
-->
