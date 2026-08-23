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

use serde::Serialize;

/// Pisanie do siebie: katalog pluginu biegu i fragment argv z `--plugin-dir`.
pub mod rewrite;

/// Czytanie gospodarza, zero zapisu: umiejętności, sekcja learnings, ciało podagenta.
pub mod scan;

/// Szew do biegu: co z tego, co przeczytaliśmy, jedzie argv, a co promptem.
pub mod wire;

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

/// Co ten folder ma do pożyczenia — trzy półki `<projekt>/.claude/`, same nazwy.
///
/// SAME NAZWY, ANI JEDNEGO BAJTU TREŚCI, i to jest cały powód, dla którego ten typ istnieje
/// obok [`HostSkill`]. Ta lista jedzie na ekran wyboru, a ekran wyboru odpowiada na pytanie
/// „co MOŻNA stąd wziąć"; treść czyta dopiero bieg, i to wyłącznie tę, którą człowiek zaznaczył.
/// Wysłanie treści na ekran byłoby wciągnięciem cudzego, nieaudytowanego tekstu do okna, zanim
/// ktokolwiek się na to zgodził.
///
/// Trzy listy, a nie jedna, bo to są trzy różne rzeczy o dwóch różnych drogach do procesu —
/// dokładnie ten sam podział, co w [`wire::Chosen`]. Jedna wspólna lista nazw zlepiłaby je
/// w jedno i pierwszy wybór trafiłby na niewłaściwą półkę.
///
/// Pusty wynik jest NORMALNĄ odpowiedzią o cudzym repozytorium, nie awarią (niezmiennik 5):
/// większość folderów nie ma `.claude/` w ogóle.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lendable {
    /// Nazwy katalogów spod `.claude/skills/`, posortowane.
    pub skills: Vec<String>,
    /// Nazwy plików ról spod `.claude/learnings/`, bez rozszerzenia, posortowane.
    pub learnings: Vec<String>,
    /// Nazwy podagentów spod `.claude/agents/`, bez rozszerzenia, posortowane.
    pub subagents: Vec<String>,
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
    /// Człowiek wybrał nazwę, której skan u gospodarza nie znalazł.
    ///
    /// TO JEST ODMOWA, A NIE POMINIĘCIE, i to jest jedyny powód, dla którego ten wariant
    /// istnieje obok [`Error::Io`]. Ciche pominięcie daje bieg, w którym człowiek zaznaczył
    /// pięć pozycji, agent dostał trzy, nic nie padło i nikt się o tym nie dowiedział —
    /// bo „agent nie zna tej umiejętności" jest z zewnątrz nieodróżnialne od „model nie
    /// uznał, że warto jej użyć". Ta sama cicha porażka, przed którą stoi całe to zadanie.
    ///
    /// Komunikat **wymienia nazwę I FOLDER**: odmowa, która nie mówi, której pozycji dotyczy,
    /// zamienia jedno odznaczenie w przeszukiwanie listy — a odmowa bez folderu zostawia
    /// człowieka z pytaniem, o który z jego projektów chodzi. Ta sama rola bywa w jednym
    /// repozytorium i nie ma jej w drugim, a wybór jedzie z kafelka, który folderu nie zna.
    #[error(
        "Loadout was told to bring in the {what} \"{name}\", and the project in {} does not \
         have one by that name. Nothing was copied and the run stopped here: leaving it out \
         quietly would give the agent less than you picked, and no screen would say so.",
        .folder.display()
    )]
    NotInTheHost {
        /// Czego dotyczy nazwa, po ludzku: `skill`, `learnings file`, `subagent`.
        what: &'static str,
        /// Nazwa, którą podał człowiek — dosłownie tak, jak ją podał.
        name: String,
        /// Korzeń repozytorium, w którym szukaliśmy.
        folder: PathBuf,
    },
}

/// Skrót modułu, tak samo jak w `skills`.
pub type Result<T, E = Error> = std::result::Result<T, E>;
