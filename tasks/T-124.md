# T-124 — Auto-pamięć kroku: pełny Markdown i dowiedziona atomowa podmiana

T-122 pozostaje dowodem, nie źródłem kodu. Zamknięto je bez lądowania po jedynej rundzie
naprawy. Oba AC i pełne testy zachowania były zielone, lecz `full-clippy` odsłonił kolejno
dwa infallible helpery testowe opakowane w `Result`; planner naprawił tylko pierwszy. Ostatnia
bramka dostała też wtórne `ENOSPC`, ale wcześniejsza autorytatywna bramka miała zielony
`full-test`, więc brak miejsca nie jest przyczyną zamknięcia.

Recenzent znalazł osobną lukę wyroczni: test T-122 odróżniał atomowy zapis od bezpośredniego
write-then-append, ale mógł przepuścić temp-then-copy-over. To zadanie jest pełnym, świeżym
zastępstwem H15. Startuje z aktualnego `main`, po T-121. **Nie przenosi commitów,
implementacji, speców ani testów z `task-T-122`.** Trzy targety są globalnie unikalne.

**Read first:** nagłówek `docs/STATUS.md` i opis zamknięcia T-122 ·
`src-tauri/src/commands/run.rs` (`what_the_steps_wrote_down`, `what_the_agent_wrote`,
`what_this_step_left_in`) · `src-tauri/src/memory/notes.rs` (`record_candidate_for`, `record`,
`write_note`, `Scope`) · `src-tauri/tests/it/claude_memory_stays_in_the_run.rs` jako
**zamrożony** kontrakt `ThisAgent + agent` · `AGENTS.md` niezmienniki 4, 19, 20, 24 i 29.

## Kto to robi

- **Agent:** Codex przez jeden pełny bieg Harnessu.
- **Druga opinia:** osobny Codex na innym modelu, tylko do odczytu.

## Granica i uczciwy `before`

- Plik z `mem/<kafelek>/` napisał konkretny agent: kandydatka zachowuje `Scope::ThisAgent`
  oraz jego nazwę. Nie wolno awansować jej do `ThisProject`.
- Odpowiedź osobnej refleksji po całym biegu pozostaje `Scope::ThisProject`; to zadanie nie
  zmienia tej drogi.
- Nowe API pełnego ciała ma wariant właścicielski, równoważny
  `record_candidate_for(..., Some(agent))`. Nie wolno zastąpić go run-only API gubiącym agenta.
- Kontrakt najpierw dodaje kompilowalne sygnatury/szkielety z `todo!()` tam, gdzie są potrzebne.
  Enforced `before` ma uruchomić testy i polec na asercjach brakującego zachowania, nie na
  braku targetu, imporcie albo kompilacji.

Każda funkcja nowego testu rustowego ma najwyżej 90 wierszy. Helper, który może zawieść,
zwraca `Result`; helper infallible zwraca konkretny typ. Bez `panic!`, `unwrap`, `expect`,
`#[allow(clippy::…)]` i sztucznej operacji tylko po to, żeby uzasadnić `Result`.

## AC-1 Zwykły Markdown zachowuje pierwszy akapit, całe ciało, powód i właściciela
check: cargo test --test t124_step_memory_owner_and_full_body
expect: (\d+) passed

Test uruchamia prawdziwą drogę skończonego kroku, którego dubel zapisuje w
`mem/<kafelek>/` dwa zwykłe pliki Markdown bez front matter oraz `MEMORY.md`. Pierwszy plik
ma wielowierszowy pierwszy akapit, dalsze akapity i dokładne `**Why:**`; drugi nie ma `Why`.

Po biegu powstają dokładnie dwie kandydatki. Każda ma `Suggested`, `ThisAgent` i nazwę
agenta. Reguła jest całym pierwszym akapitem, nie pierwszą linią. Ciało pliku źródłowego
przeżywa bajtowo i w tej samej kolejności. `because` pierwszej notatki jest dokładną treścią
`**Why:**`; druga dostaje zdanie pochodzenia nazywające agenta, krok i bieg. `MEMORY.md` jest
pominięty. Zamrożony `claude_memory_stays_in_the_run.rs` musi pozostać zielony bez zmiany.

Zakazane: `ThisProject` dla auto-pamięci kroku, brak pola agent, pierwsza linia jako reguła,
ponowne otwarcie gotowej notatki w `run.rs`, syntetyczne ciało zamiast źródła lub modyfikacja
zamrożonego testu.

## AC-2 Błąd i retry zachowują dokładny listing oraz stare albo nowe pełne bajty
check: cargo test --test t124_atomic_owned_note_retry
expect: (\d+) passed

Test woła produkcyjny wariant zapisu pełnego ciała z `Scope::ThisAgent` i jawnym agentem.
Najpierw tworzy notatkę i zapisuje pełny listing katalogu oraz bajty. Sam plik pozostaje
zapisywalny, ale katalog traci możliwość utworzenia sąsiada. Ponowienie tego samego slugu z
nowym pełnym Markdownem musi zwrócić błąd oraz zostawić listing i stare bajty dokładnie bez
zmian. Po przywróceniu uprawnień zapis przechodzi; test porównuje końcowy pełny listing,
front matter, właściciela i **nowe pełne bajty/body**, nie tylko nazwę pliku.

`memory::notes` składa front matter i body w jednym pliku tymczasowym w katalogu celu,
`sync_all`/persistuje go atomowo i nie zostawia żadnego sąsiada. Istniejące API bez body
zachowuje dotychczasowe zachowanie. Zakazane: write-then-append, temp poza katalogiem,
heurystyka nazw tempów albo test samego listingu.

## AC-3 Podmiana działa nad plikiem tylko do odczytu, więc copy-over nie przechodzi
check: cargo test --test t124_atomic_note_replacement
expect: (\d+) passed

Test tworzy starą notatkę, po czym ustawia **wyłącznie plik docelowy** jako tylko do odczytu;
katalog rodzica nadal pozwala utworzyć sąsiada i atomowo podmienić wpis katalogowy. Zapis
nowego pełnego Markdownu tego samego slugu musi się udać i pozostawić dokładnie nowy kompletny
plik oraz niezmieniony zbiór nazw w katalogu. Test przywraca uprawnienia w RAII również po
błędzie, żeby nie zostawić nieusuwalnego fixture.

To jest deterministyczna mutacja wyroczni z uwagi recenzenta T-122: `persist`/rename w tym
samym katalogu zastępuje wpis mimo read-only starego inode, natomiast otwarcie celu do zapisu,
`fs::copy` over target albo truncate-then-write dostaje odmowę. Zakazane: asercja stringa
`persist` w źródle, chmod celu w kodzie produkcyjnym, usunięcie starego celu przed zapisem,
test-only przełącznik wybierający implementację lub rozluźnienie błędu AC-2.

<!-- OWNS
tasks/T-124.md
src-tauri/src/commands/run.rs
src-tauri/src/memory/notes.rs
src-tauri/tests/t124_step_memory_owner_and_full_body.rs
src-tauri/tests/t124_atomic_owned_note_retry.rs
src-tauri/tests/t124_atomic_note_replacement.rs
-->
