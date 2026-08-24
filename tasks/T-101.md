# T-101 — Każda porażka przechodzi przez jedno miejsce — naprawdę

T-87 AC-5 obiecało, że każda ścieżka porażki przechodzi przez `when_this_one_fails`
i zostawia po sobie to, co agent zdążył powiedzieć. Trzy ścieżki nadal są obok
(zweryfikowane w trunku 2026-08-24):

1. **`CONTEXT_NOT_PROVEN`** (`run.rs` ok. 6815–6825): błąd montowania promptu wraca jako
   `StepReport::Failed` wprost — `carry-on` i `ask-me` nie działają.
2. **`Route::Blocked`**: krok, który zameldował sukces, a żaden warunek krawędzi nie pasuje,
   jest w planiście po cichu zamieniany na `Failed` ze ściętym stożkiem
   (`scheduler.rs` ok. 218–223). `when_it_fails` nie jest pytane, okno zostaje z zieloną
   linią (`StepState succeeded` poszło przed decyzją), a książka po `close_the_book` mówi
   `failed` — dwa źródła prawdy się rozjeżdżają i nikt nie wysyła korekty.
3. **Stożek budżetu**: krok zatrzymany sufitem wraca jako `StepReport::Cancelled`
   (`run.rs` ok. 6438–6454), więc planista maluje jego potomków na `cancelled` bez powodu —
   na ekranie „nacisnąłeś Stop", `error: null`. Komentarz przy `name_what_the_budget_stopped`
   (ok. 8139–8144) twierdzi, że jest inaczej — kod ma dogonić własny komentarz. Dzisiejsza
   fikstura budżetu nie ma ani jednego kroku PONIŻEJ zatrzymanego.

**Read first:** `src-tauri/src/commands/run.rs` (`when_this_one_fails` ok. 5792,
`refuse_route` ok. 5738, `the_budget_stops_this_one` ok. 6438, `announce` — wszyscy wołający) ·
`src-tauri/src/engine/scheduler.rs` (`execute_routed`, `mark_cone`, obsługa `Route`) ·
`src-tauri/tests/it/every_failure_leaves_its_last_words.rs`,
`conditional_edges_choose_one_branch.rs`, `a_run_stops_at_its_budget.rs`,
`engine_cone_reason.rs` (istniejące wyrocznie — rozszerz, nie dubluj) · `AGENTS.md`
niezmiennik 11 (jeden autorytet stanu).

## Kto to robi

- **Agent:** `rust-core`. Po T-100 (wspólny `run.rs`).
- **Druga opinia:** inny vendor niż pisarz (D3).

## AC-1 Nieudowodniony kontekst idzie wybraną ścieżką
check: cargo test --test it context_failures_take_the_chosen_path::
expect: (\d+) passed

Krok, którego kontekstu nie dało się udowodnić, z `carry-on` oddaje przekazanie ostatnich
słów (może być puste) i puszcza potomków z etykietą „nie przeszedł"; z `ask-me` czeka na
człowieka; ze `stop` pada jak dziś (kontrola). Powód w `run.json` pozostaje ten sam, co
dzisiejszy tekst odmowy.

## AC-2 Zablokowana droga wyjścia idzie wybraną ścieżką i mówi jedno
check: cargo test --test it a_blocked_way_out_takes_the_chosen_path::
expect: (\d+) passed

Krok z warunkami na krawędziach, którego wynik nie pasuje do żadnej (albo do dwóch),
przechodzi przez `when_this_one_fails` zgodnie ze swoim ustawieniem; przy `stop` pada
z dzisiejszym powodem. Strumień dostaje po decyzji linię stanu zgodną z książką — test
zbiera wyemitowane linie i porównuje ostatni stan kroku z wierszem w `run.json`
(dziś: strumień mówi `succeeded`, plik `failed`).

## AC-3 Kroki poniżej wyczerpanego budżetu mówią dlaczego stoją
check: cargo test --test it steps_below_a_spent_budget_say_so::
expect: (\d+) passed

Fikstura z krokiem POD krokiem zatrzymanym sufitem: potomek kończy jako `skipped` ze
zdaniem o budżecie (tym samym, które dostaje krok zatrzymany), bieg nie jest `cancelled`,
a żaden wiersz nie zostaje z `error: null` i stanem „nacisnąłeś Stop". Kontrola: prawdziwy
Stop człowieka dalej daje `cancelled` (rozróżnienie z ARCHITECTURE §5 zostaje).

## AC-4 Wszystkie trzy ścieżki zostawiają ostatnie słowa
check: cargo test --test it every_failure_shares_one_door::
expect: (\d+) passed

Dla każdej z trzech ścieżek wyżej: przy `carry-on` następny krok ma w indeksie wiersz
z etykietą „the step before did not pass; this is what it said" wskazujący plik ostatnich
słów. Kontrola: `stop` nie oddaje nic dalej, bo nic po nim nie biegnie (jak w T-87 AC-5).

<!-- OWNS
tasks/T-101.md
src-tauri/src/commands/run.rs
src-tauri/src/engine/scheduler.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/context_failures_take_the_chosen_path.rs
src-tauri/tests/it/a_blocked_way_out_takes_the_chosen_path.rs
src-tauri/tests/it/steps_below_a_spent_budget_say_so.rs
src-tauri/tests/it/every_failure_shares_one_door.rs
-->
