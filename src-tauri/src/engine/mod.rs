//! Silnik biegu: graf, planista, maszyna stanów kroku.
//!
//! **Ten katalog nie zna okna i nie ma prawa go poznać** (`docs/ARCHITECTURE.md` §3,
//! niezmiennik 1). Bez tej granicy silnika nie da się przetestować bez okna, a osobny daemon
//! nigdy nie powstanie. Sprawdzenie jest gerpem po NIEKOMENTOWANYCH liniach każdego pliku
//! w tym katalogu, więc granica łamie się tu także przez **literał** ze ścieżką repo, nie
//! tylko przez `use`. Dlatego ścieżki plików przychodzą tu argumentem, nigdy stałą.
//!
//! **Niezmiennik 27:** żaden etap biegu nie jest zaszyty w tym kodzie. Planista dostaje graf
//! i go wykonuje; nie zna pojęć „recenzja", „bramka" ani „poprawka". Skrót o jednej linii —
//! `if node.kind == Review` — na zawsze przypina ceremonię do kodu zamiast do konfiguracji
//! workflow, a wtedy użytkownik chcący jednego agenta bez niczego i tak dostanie recenzję,
//! bo nie ma jej jak wyłączyć (decyzja D7).
//!
//! # Stan tego pliku: SZKIELET (2026-08-15)
//!
//! Ciała funkcji w tym katalogu zwracają **świadomie złą wartość** i są tak oznaczone
//! komentarzem `SZKIELET`. To jest wymagany kształt fazy, w której powstają kryteria:
//! test ma się skompilować i paść **w czasie wykonania, na braku ZACHOWANIA** — test, który
//! się nie kompiluje, niczego nie uruchomił (`AGENTS.md` §2a p. 5). Każdy taki stub jest
//! dobrany tak, żeby żadnego kryterium nie dało się na nim przejść; rozpisane jest to przy
//! każdym ciele z osobna.

/// Numer kroku w grafie biegu. Jest indeksem do wektorów planisty, nie kluczem w bazie —
/// stabilny klucz węzła (`node_key`) mieszka w `store/` i nie ma prawa tu wejść: silnik ma
/// dać się przetestować bez bazy tak samo, jak daje się przetestować bez okna.
pub type StepId = usize;

pub mod dag;
pub mod drivers;
pub mod scheduler;
pub mod step;

// 2026-08-15 (T-04) — `#[path]` na dublera ZOSTAJE, choć `pub mod drivers;` jest już wyżej.
//
// Zapowiedź brzmiała „kiedy T-04 doda `pub mod drivers;`, ta deklaracja znika, a dubler wraca
// pod `drivers::fake`". Tej przeprowadzki nie da się zrobić w granicach T-04 i nie jest ona
// darmowa: adres `drivers::fake` wymaga dopisania `pub mod fake;` w `drivers/mod.rs`,
// poprawienia `super::StepId` i `super::step::StepReport` w samym `fake.rs`, a potem zmiany
// pięciu plików w `src-tauri/tests/`, które importują `loadout_lib::engine::fake` — a `fake.rs`
// ani żaden z tych pięciu testów nie leży w bloku OWNS tego zadania (AGENTS.md §7).
//
// Zostawienie `#[path]` niczego nie psuje, bo `drivers/mod.rs` **nie** deklaruje `fake`:
// plik ma dokładnie jedną deklarację, więc kompiluje się raz i jako jeden moduł. To ta sama
// pułapka, którą opisuje `lib.rs` przy `supervisor` — dwie deklaracje tego samego pliku dają
// dwa niezależne typy, których kompilator nie zamieni jeden w drugi. Tu jej nie ma.
//
// Przeniesienie jest zadaniem na jeden commit dla człowieka albo dla zadania, które i tak
// dotyka tamtych pięciu testów. Do tego czasu adres dublera to `engine::fake`.
#[path = "drivers/fake.rs"]
pub mod fake;
pub mod supervisor;

// 2026-08-15 — WIERSZE, KTÓRE DOŁOŻĄ KOLEJNE ZADANIA. Każdy z nich jest jednym wierszem
// poza blokiem OWNS tamtego zadania, więc każdy jest osobnym pytaniem do człowieka
// (AGENTS.md §7). Ta lista istnieje po to, żeby to pytanie dało się zadać w dziesięć sekund
// zamiast czytać cały plan:
//
//     pub mod stream;       — T-05 (NDJSON → zdarzenie)
//     pub mod line;         — T-05 (zdarzenie → linia; tu mieszka kuracja)
//
// `pub mod drivers;` z tej listy zszedł: człowiek odpowiedział na to pytanie, wpisując
// `src-tauri/src/engine/mod.rs` wprost do bloku OWNS zadania T-04.
