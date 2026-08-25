# T-121 — Indeks biegu jest jednym atomowym snapshotem

T-120 pozostaje dowodem, nie źródłem kodu. Zamknięto je bez lądowania po drugiej bramce
19/22. Jego AC-2 poprawnie znalazło brak idempotentnej wymiany czterech tabel, lecz własna
wyrocznia sortowała odczytane eventy po `body` i porównywała je z kolejnością wejściową.
Produkcyjna transakcja była już poprawna, a test pozostawał czerwony na odwróconych dwóch
wierszach. Po jedynej naprawie Harness nie ma piątej tury.

To małe zadanie wycina z T-120 wyłącznie niezależny kontrakt Store. Startuje z aktualnego
`main`. **Nie przenosi commitów, implementacji, speców ani testów z `task-T-120`.** Oba nowe
targety są globalnie unikalne; dopiero zielone lądowanie T-121 jest bazą dla refleksji.

**Read first:** nagłówek `docs/STATUS.md` i opis zamknięcia T-120 ·
`src-tauri/src/store/mod.rs` (`Store::rebuild_from`) · `src-tauri/src/store/writer.rs`
(`Rows`, `write`, transakcje i wyłączny pisarz) · `src-tauri/src/store/rebuild.rs` wyłącznie
jako źródło czterech kolekcji snapshotu · `AGENTS.md` niezmienniki 2, 4, 19 i 25.

## Kto to robi

- **Agent:** Codex przez jeden pełny bieg Harnessu.
- **Druga opinia:** osobny Codex na innym modelu, tylko do odczytu.

## Higiena speców

Każda funkcja nowego testu ma najwyżej 90 wierszy. Helpery zwracają `Result`; zakazane są
`panic!`, `unwrap`, `expect` i `#[allow(clippy::…)]`. Test ma paść w runtime na istniejącym
braku wymiany, nigdy na kompilacji. Żaden test nie może zależeć od kolejności inserta lub
niepełnego `ORDER BY`.

## AC-1 Ponowna odbudowa daje dokładny nowy multiset i nic starego
check: cargo test --test t121_exact_snapshot_multiset
expect: (\d+) passed

Test tworzy prawdziwy katalog biegu, odbudowuje indeks, a potem zmienia `run.json`, krok,
dwa różne eventy i artefakt przy zachowaniu tego samego `runs.id` oraz `steps.id`. Druga
odbudowa ma pokazać dokładny nowy stan wszystkich czterech tabel: nowy artefakt jest obecny,
stary jawnie nieobecny, żaden stary wiersz nie przeżywa. Trzecia odbudowa jest idempotentna,
a źródłowy `run.json` pozostaje bajtowo nietknięty.

Eventy są porównywane jako **dokładny multiset pełnych sześciu kolumn wraz z licznością**.
Actual i expected są osobno kanonizowane tym samym jawnym sortowaniem pełnego wiersza albo
licznikiem kanonicznych krotek. Zakazane: oczekiwanie kolejności wejściowej, sortowanie tylko
po `body`, zamiana multiset na set gubiący duplikaty, sprawdzanie samej liczby wierszy lub
samych markerów. Fikstura ma dwa eventy w kolejności wejściowej przeciwnej do leksykalnego
`body`, żeby stary błąd wyroczni nie mógł wrócić.

## AC-2 Błąd późnego artefaktu zostawia cały poprzedni snapshot
check: cargo test --test t121_snapshot_rollback_transaction
expect: (\d+) passed

Po pierwszej odbudowie test zapisuje pełny stary snapshot. Następnie zmienia wszystkie cztery
kolekcje źródłowe i instaluje w testowej bazie trigger, który odrzuca nazwany nowy artefakt —
czyli błąd zachodzi dopiero po rozpoczęciu wymiany. Po błędzie czytelnik widzi dokładnie cały
stary snapshot, nigdy stan pusty ani mieszany. Po usunięciu triggera ponowienie daje dokładnie
cały nowy snapshot i nadal nie zmienia plików źródłowych.

Produkcja wysyła jeden job do `store::writer`; usunięcie starego rodzica i wszystkie inserty
są jedną transakcją tego samego połączenia. Zakazane: `INSERT OR IGNORE`, zmiana id, drugi
produkcyjny writer/connection, osobne transakcje per tabela, zapis do `run.json` lub
przechwycenie błędu i raportowanie sukcesu.

<!-- OWNS
tasks/T-121.md
src-tauri/src/store/mod.rs
src-tauri/src/store/writer.rs
src-tauri/tests/t121_exact_snapshot_multiset.rs
src-tauri/tests/t121_snapshot_rollback_transaction.rs
-->
