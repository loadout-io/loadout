# T-140 — Świeży indeks nie udaje, że pamięć mieszka w SQLite

Samodzielny następca części schematowej zamkniętego T-108. Martwa tabela `memory` nie ma
ani pisarza, ani czytelnika, a jej kolumny nie opisują dzisiejszych plików notatek.

Jest tu ważna granica wyższego rzędu. Niezmiennik 25 w `AGENTS.md` zakazuje `DROP`,
przepisywania wierszy i migracji destrukcyjnych. Dlatego zadanie usuwa tabelę ze **świeżego
i odbudowanego indeksu**, ale istniejącą starą tabelę toleruje bez dotykania do chwili, gdy
człowiek skasuje odtwarzalny `loadout.db`. To nie jest utrata danych: pliki są prawdą, SQLite
jest wyłącznie indeksem. Nie dodawaj `DROP TABLE`, nie przebudowuj bazy po cichu i nie zmieniaj
tej granicy w kontrakcie.

**Read first:** `src-tauri/src/store/{schema,migrate,mod,rebuild}.rs` ·
`src-tauri/src/memory/notes.rs` (nagłówek) ·
`src-tauri/tests/it/{store_migrate_idempotent,store_strict_schema}.rs`.

## Kto to robi

- **Agent:** Codex przez jeden pełny bieg Harnessu, po T-135.
- **Druga opinia:** osobny Codex na innym modelu, tylko do odczytu; właścicielski wyjątek D3.

## AC-1 Świeży i odbudowany indeks mają tylko żywe tabele
check: cargo test --test t140_fresh_index_has_only_live_tables
expect: (\d+) passed

Standalone target otwiera prawdziwy `Store` na świeżej ścieżce i wymaga dokładnie czterech
tabel użytkownika: `runs`, `steps`, `events`, `artifacts`; nie istnieją `memory` ani
`idx_memory_scope`.

Osobna fixture starej bazy zawiera historyczny kształt `memory`, jego indeks i wiersz oraz
kontrolne wiersze żywych tabel. Dwa kolejne otwarcia nie usuwają ani nie przepisują żadnego
z nich — to dowód zgodności z niezmiennikiem 25, nie zgoda na dalsze używanie tabeli. Po
zamknięciu i skasowaniu wyłącznie odtwarzalnego pliku indeksu, istniejące pliki biegu są
ponownie indeksowane produkcyjną drogą; nowy indeks zachowuje fakty biegu i nie odtwarza
`memory`. Test nie może zazielenić się przez samo sprawdzenie stringa DDL.

## AC-2 Notatki mają wyłącznie plikową prawdę
check: cargo test --test t140_notes_have_no_sqlite_shadow
expect: (\d+) passed

Target zapisuje kandydatkę publicznym API do tymczasowego korzenia, odczytuje pełny Markdown
i dowodzi, że zapis nie tworzy `loadout.db` ani drugiej kopii treści. Następnie sprawdza
nagłówek modułu `memory::notes`: znika fałszywe twierdzenie o wierszu zapisywanym przez
`store::writer`, a komentarz mówi wprost, że pliki biblioteki i projektu są jedynym miejscem
zapisu oraz źródłem prawdy. Sprawdzenie tekstu jest tu dozwolone, bo przedmiotem tej połowy
kryterium jest dokumentacja; obok stoi wykonany dowód zachowania.

## Uczciwe `before`

Oba standalone targety muszą istnieć i kompilować się przed zmianą produkcji. AC-1 ma paść
na asercji, bo świeży dzisiejszy schemat nadal tworzy `memory`; setup starej fixture nie może
sam się wywrócić. AC-2 najpierw przechodzi zapis i odczyt pliku, potem pada na fałszywym
nagłówku. Brak targetu, modułu, kompilacji albo timeout nie jest czerwienią.

## Wyłączenia

Bez recovery, `RunSpec`, writer API, formatu notatek, frontendu i destrukcyjnej migracji.
Stare testy schematu zmieniają model pięciu tabel na cztery, ale nowe AC niezależnie dowodzi
braku tabeli; samo rozluźnienie listy nie przejdzie.

<!-- OWNS
tasks/T-140.md
src-tauri/src/store/schema.rs
src-tauri/src/memory/notes.rs
src-tauri/tests/t140_fresh_index_has_only_live_tables.rs
src-tauri/tests/t140_notes_have_no_sqlite_shadow.rs
src-tauri/tests/it/store_migrate_idempotent.rs
src-tauri/tests/it/store_strict_schema.rs
-->
