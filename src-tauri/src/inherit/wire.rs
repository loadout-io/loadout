//! Szew między „co gospodarz ma" a „co proces agenta naprawdę dostaje".
//!
//! T-54 zbudowało cztery czyste funkcje — [`super::scan::skills`],
//! [`super::scan::recurring_patterns`], [`super::scan::agent_body`],
//! [`super::rewrite::plugin_dir`] — i ani jednego wołającego poza `tests/`. Ten plik jest tym
//! wołającym. Nie dokłada ani jednej reguły czytania cudzego repozytorium: bierze to, co tamte
//! funkcje zwracają, i **rozstrzyga, którą drogą to jedzie do procesu**.
//!
//! DWIE DROGI I ANI JEDNEJ TRZECIEJ (niezmiennik 9). Ścieżka katalogu pluginu jedzie
//! **argv**, bo vendor nie umie jej przyjąć inaczej, a ścieżka nie jest treścią. Tekst —
//! sekcja `## Recurring patterns` i ciało podagenta — jedzie **wyłącznie promptem**, czyli
//! stdinem. Nigdy `--append-system-prompt`: to jest argument, a argumenty widzi `ps` każdego
//! użytkownika maszyny. Te dwa zdania są całą treścią tego modułu i dlatego stoją w jednym
//! typie ([`Inherited`]), a nie w dwóch miejscach wywołania.
//!
//! POLITYKA MIESZKA TUTAJ, NIE W ADAPTERZE (niezmiennik 23). Sterownik dostaje **gotową listę
//! flag** i nie zna słowa „umiejętność"; ten plik nie zna nazwy żadnego sterownika. Drugi
//! zestaw reguł po stronie adaptera jest dokładnie tym, jak w repo źródłowym po cichu umarło
//! skanowanie sekretów [raport 05 §4].
//!
//! DOMYŚLNIE NIE DZIEDZICZYMY NICZEGO, i to jest zachowanie, nie ostrożność. Repozytorium
//! gospodarza to cudzy tekst, którego nikt nie audytował; wciągnięcie go „bo był" znaczy, że
//! człowiek płaci za kontekst, o który nie prosił, i czyta odpowiedzi oparte na regułach,
//! których nie widział. [`Chosen::default`] jest więc pustym wyborem, a nie pełnym.

use std::path::Path;

use super::Result;
use crate::engine::drivers::RunSpec;

/// Co człowiek wybrał z repozytorium gospodarza. **Pusty wybór jest domyślny.**
///
/// Trzy pola, bo to są trzy różne rzeczy o trzech różnych drogach: umiejętności jadą
/// katalogiem pluginu (argv), a learnings i podagent — tekstem w prompcie. Jedna wspólna
/// lista nazw zlepiłaby je w jedno i pierwsza pomyłka wsadziłaby treść do argv.
///
/// `Default` jest tu **treścią kryterium**, nie wygodą dla wołających: bieg, który nie dostał
/// jawnego wyboru, ma złożyć argv bez `--plugin-dir` i prompt bez ani jednego dodatkowego
/// bajtu, także wtedy, gdy gospodarz ma pełne `.claude/`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chosen {
    /// Nazwy katalogów spod `<projekt>/.claude/skills/`, dokładnie takie, jakie zwrócił
    /// [`super::scan::skills`]. Nazwa spoza tego, co skan naprawdę znalazł, jest odmową
    /// ([`super::Error::NotInTheHost`]), a nie cichym pominięciem.
    pub skills: Vec<String>,
    /// Nazwa pliku roli spod `<projekt>/.claude/learnings/`, bez rozszerzenia. Do promptu
    /// wchodzi z niego **wyłącznie** sekcja `## Recurring patterns`.
    pub learnings: Option<String>,
    /// Nazwa podagenta spod `<projekt>/.claude/agents/`, bez rozszerzenia. Do promptu wchodzi
    /// z niego **wyłącznie ciało**; front-matter jest granicą maszynerii.
    pub subagent: Option<String>,
}

/// Co ten bieg odziedziczył: gotowy fragment argv i gotowy tekst do promptu.
///
/// Jeden typ na dwa wyjścia, bo to jest jedno rozstrzygnięcie: **ścieżka wolno do argv, treść
/// nie**. Dwa osobne typy dawałyby dwa miejsca wywołania i dwie okazje, żeby pomylić drogę —
/// a pomyłka w tę stronę jest niewidoczna, bo wszystko działa, tylko treść stoi w `ps`.
///
/// Pusty [`Inherited`] jest normalnym wynikiem, nie awarią: tak wygląda bieg bez wyboru
/// człowieka i bieg w repozytorium bez `.claude/` (niezmiennik 5).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inherited {
    /// `["--plugin-dir", <katalog>]` albo nic — dokładnie to, co oddał
    /// [`super::rewrite::plugin_argv`]. **Nigdy sama flaga**: flaga bez wartości połknęłaby
    /// następny argument sterownika.
    flags: Vec<String>,
    /// Tekst doklejany do promptu kroku. Pusty, kiedy nie było czego odziedziczyć.
    text: String,
}

impl Inherited {
    /// Gotowa lista flag dla sterownika — i cała wiedza, jaką adapter ma o dziedziczeniu
    /// (niezmiennik 23).
    #[must_use]
    pub fn flags(&self) -> &[String] {
        todo!("T-57 AC-1: fragment argv jeszcze nie powstaje ({self:?})")
    }

    /// Ten sam krok, ale z odziedziczonym tekstem w prompcie — i **nigdzie indziej**.
    ///
    /// Bierze i oddaje cały [`RunSpec`], a nie sam prompt, i to jest jedyny powód, dla którego
    /// da się dowieść niezmiennika 9 z zewnątrz: to ta funkcja decyduje, że `system_append`
    /// wraca nietknięty. Gdyby decydowało o tym miejsce wywołania, każde nowe miejsce
    /// decydowałoby od nowa.
    ///
    /// Pusty [`Inherited`] oddaje krok **co do bajtu** takim, jaki przyszedł: nagłówek nad
    /// pustką uczy model, że ta sekcja bywa pusta, i kosztuje długość za nic (ten sam powód
    /// stoi przy `commands::run::with_what_we_know`).
    #[must_use]
    pub fn applied_to(&self, spec: RunSpec) -> RunSpec {
        todo!(
            "T-57 AC-2/AC-3: odziedziczony tekst jeszcze nie wchodzi do promptu kroku {}",
            spec.run_id
        )
    }
}

/// Zbiera to, co człowiek wybrał u gospodarza, do katalogu **tego** biegu.
///
/// `project` to korzeń repozytorium gospodarza — ten sam folder, w którym pracuje bieg —
/// a `run_dir` to katalog biegu (`<projekt>/.loadout/runs/<ts>__<id>/`). Katalog pluginu
/// powstaje **pod nim** i nigdzie indziej: wymyślone miejsce byłoby `$TMPDIR`, czyli
/// artefaktem biegu poza biegiem (`docs/ARCHITECTURE.md` §8), a zapis do `.claude/`
/// gospodarza łamałby jedyną obietnicę, jaką temu repozytorium złożyliśmy — czytamy je
/// i niczego w nim nie ruszamy.
///
/// Nazwa spoza tego, co [`super::scan::skills`] naprawdę znalazł, jest **odmową z nazwaniem
/// pozycji** ([`super::Error::NotInTheHost`]), a nie cichym pominięciem: człowiek zaznaczył
/// pięć umiejętności, dostał trzy i nie ma jak się o tym dowiedzieć — „agent nie zna
/// umiejętności" jest nieodróżnialne od „model nie uznał, że warto jej użyć".
pub fn from_the_host(project: &Path, run_dir: &Path, chosen: &Chosen) -> Result<Inherited> {
    todo!(
        "T-57: {chosen:?} z {} jeszcze nie dojezdza do biegu w {}",
        project.display(),
        run_dir.display()
    )
}
