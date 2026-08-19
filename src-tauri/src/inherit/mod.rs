//! Dziedziczenie wiedzy repo gospodarza: przenosimy TEKST, nigdy MASZYNERIĘ.
//!
//! Zasada, z której bierze się cały ten katalog: harness jest nasz. Z cudzego repozytorium
//! bierzemy umiejętności, learnings i podagentów **przez przepisanie do siebie** — czytamy
//! jego pliki, kopiujemy bajty do katalogu, który sami stworzyliśmy, i podajemy sesji **nasz**
//! katalog. Nie ładujemy `.claude/settings.json` gospodarza, nie uruchamiamy jego haków i nie
//! przenosimy ani jednej linii, która potrafi wystartować proces.
//!
//! DLACZEGO to jest niepodważalne, a nie estetyczne [zmierzone 2026-08-19]: hak `PreToolUse`
//! gospodarza startuje proces we własnej grupie procesów, jego dziecko dostaje `ppid=1`
//! i przeżywa wyjście `claude` — jeden bieg zostawił 14 sierot, eksperymenty łącznie 30. Przy
//! załadowanych ustawieniach gospodarza niezmiennik 6 jest nie do utrzymania: nie ma czego
//! zabić, bo grupa nie jest nasza, i nie ma czego dowieść, bo `kill(-pgid, 0)` pyta o cudzą
//! grupę. Drugi wypadek tej samej klasy: podagent repo gospodarza wystartował jako osobny
//! proces i spalił 38–41 tys. tokenów poza widokiem i rozliczeniem Loadouta.
//!
//! Ten plik trzyma **dane**: dwa typy i enum błędu. Zachowanie mieszka obok — czytanie
//! gospodarza w [`scan`], pisanie do siebie w [`rewrite`]. Ten sam podział, co `skills/mod.rs`
//! wobec `skills/place.rs`.

use std::path::PathBuf;

/// Pisanie do siebie: katalog pluginu biegu i fragment argv z `--plugin-dir`.
pub mod rewrite;

/// Czytanie gospodarza, zero zapisu: umiejętności, sekcja learnings, ciało podagenta.
pub mod scan;

/// Jedna umiejętność zobaczona w cudzym repozytorium: nazwa katalogu i pierwszy wiersz jego
/// `SKILL.md`, **zacytowany dosłownie**.
///
/// DLACZEGO dosłownie: to jest zdanie, po którym człowiek rozpoznaje, czyj to plik, zanim
/// zdecyduje, czy wpuścić go do biegu. Zdanie wymyślone w zastępstwie zostałoby mu pokazane
/// tak, jakby stało w cudzym pliku. Ta sama reguła, z tego samego powodu, stoi przy
/// `skills::place::first_line`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSkill {
    /// Nazwa katalogu pod `<projekt>/.claude/skills/`. Jest też nazwą, pod którą umiejętność
    /// ląduje w katalogu pluginu.
    pub name: String,
    /// Pierwszy wiersz `SKILL.md`, dosłownie. Dla pliku z front-matterem jest to `---`.
    pub first_line: String,
}

/// Co powstało po przepisaniu: katalog pluginu biegu i nazwy, które do niego weszły.
///
/// `names` jest pusta, kiedy nie było czego odziedziczyć — i to jest **jedyne** miejsce,
/// w którym kompozytor argv sprawdza, czy flaga `--plugin-dir` ma w ogóle powstać. Pusty
/// katalog przekazany vendorowi to plugin, który ładuje się i rejestruje zero umiejętności,
/// czyli dokładnie ta cicha zieleń, przed którą stoi całe to zadanie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewritten {
    /// Katalog pluginu biegu — ten, który dostanie `claude --plugin-dir`. Przy pustym
    /// dziedziczeniu ta ścieżka jest znana, ale **nie powstała na dysku**.
    pub dir: PathBuf,
    /// Nazwy umiejętności, których `SKILL.md` naprawdę trafił pod `skills/<nazwa>/`.
    pub names: Vec<String>,
}

/// Błędy dziedziczenia.
///
/// Świadomie krótki enum. Katalog bez `SKILL.md`, plik learnings bez sekcji, podagent bez
/// front-mattera i brak całego `.claude/` są **normalnym stanem cudzego repozytorium**, nie
/// błędem (niezmiennik 5) — każde z nich wraca jako pusty wynik i `Ok`. Wariant błędu istnieje
/// dla awarii dysku, czyli dla stanu, o którym człowiek naprawdę ma się dowiedzieć.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Skrót modułu, tak samo jak w `skills`.
pub type Result<T, E = Error> = std::result::Result<T, E>;
