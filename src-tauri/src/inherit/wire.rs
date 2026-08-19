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

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use super::{Error, Result, rewrite, scan};
use crate::engine::drivers::RunSpec;

/// Podkatalog katalogu biegu, w którym staje katalog pluginu.
///
/// Miejsce jest **pod katalogiem biegu** i to jest cała treść tej stałej: katalog pluginu jest
/// wyjściem builda (niezmiennik 4), więc ma zniknąć razem z biegiem. `$TMPDIR` zostawiałby
/// artefakt biegu poza biegiem (`docs/ARCHITECTURE.md` §8), a `.claude/` gospodarza łamałoby
/// jedyną obietnicę, jaką temu repozytorium złożyliśmy: czytamy je i niczego w nim nie ruszamy.
const PLUGIN_DIR: &str = "plugin";

/// Katalog, w którym cudze repozytorium trzyma to, co bierzemy.
///
/// Ścieżkę do umiejętności wydaje [`scan::skill_file`] i ta stała nie jest jego kopią: tamta
/// funkcja odpowiada na pytanie „gdzie leży `SKILL.md` tej umiejętności", a tu składamy dwie
/// **inne** półki tego samego katalogu. Nazwy plików ról i podagentów nie mają odpowiednika po
/// stronie [`scan`], bo [`scan::recurring_patterns`] i [`scan::agent_body`] są funkcjami nad
/// **tekstem** — plik czyta ten, kto wie, którą pozycję wybrał człowiek, czyli ten plik.
const HOST_DIR: &str = ".claude";

/// Półka z plikami ról: `<projekt>/.claude/learnings/<rola>.md`.
const LEARNINGS_DIR: &str = "learnings";

/// Czego dotyczy odmowa — po ludzku, bo to zdanie czyta człowiek ([`Error::NotInTheHost`]).
const A_SKILL: &str = "skill";
const A_LEARNINGS_FILE: &str = "learnings file";

/// Zdanie, po którym model wie, **czyje** są reguły stojące pod nim.
///
/// Bez nagłówka odziedziczony tekst zlewa się z zadaniem kroku w jedno polecenie, a wtedy cudza
/// reguła czyta się jak nasza instrukcja. Zdanie, nie słowo — ten sam wybór, z tego samego
/// powodu, stoi przy `commands::run::TASK_HEADING`.
const PATTERNS_HEADING: &str = "Rules this project keeps, in its own words:";

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
        // Wycinek, nie kopia: sterownik ma dostać to, co przyszło, i ani jednej decyzji więcej.
        // Puste znaczy „nie było czego odziedziczyć" i po tej stronie granicy nie ma nikogo,
        // kto by z tego zrobił flagę bez wartości.
        &self.flags
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
    pub fn applied_to(&self, mut spec: RunSpec) -> RunSpec {
        if self.text.is_empty() {
            // CO DO BAJTU TEN SAM KROK. Nagłówek nad pustką uczy model, że ta sekcja bywa pusta,
            // i kosztuje długość za nic — ten sam powód stoi przy `commands::run::with_what_we_know`.
            return spec;
        }

        // ODZIEDZICZONE STOI NAD ZADANIEM, i to jest ta sama kolejność, co w `with_what_we_know`:
        // kontekst nad polem pracy, pole pracy nad robotą. Zadanie kroku zostaje **na końcu**, bo
        // to ono jest tym, o co człowiek poprosił; wstawione w środek cudzych reguł czyta się jak
        // ich część.
        //
        // Doklejenie, nigdy podmiana: prompt zastąpiony odziedziczonym tekstem daje krok, który
        // nie dostał swojego zadania, a widać to wyłącznie po odpowiedzi „jakoś nie o to".
        spec.prompt = format!("{}\n\n{}", self.text, spec.prompt);
        // `system_append` wraca nietknięty i to jest asercja o niezmienniku 9, nie oszczędność:
        // to pole staje się `--append-system-prompt`, czyli argumentem, a argumenty widzi `ps`
        // każdego użytkownika maszyny.
        spec
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
    // NAJPIERW CZYTAMY I ODMAWIAMY, DOPIERO POTEM PISZEMY. Bieg, który przepisał połowę wyboru
    // i dopiero na drugiej pozycji odmówił, zostawia katalog pluginu w kształcie, którego nikt
    // nie zamawiał — a katalog, który powstał, prędzej czy później zostanie komuś podany. Ta
    // kolejność jest jedynym powodem, dla którego cały ten blok stoi przed `plugin_dir`.
    every_name_is_really_there(project, &chosen.skills)?;

    // Bloki, nie jeden rosnący napis: każdy z nich ma własny nagłówek i własny powód, a blok,
    // z którego nic nie wyszło, po prostu nie wchodzi na tę listę. Nagłówek nad pustką uczy
    // model, że ta sekcja bywa pusta, i kosztuje długość za nic.
    let mut blocks: Vec<String> = Vec::new();

    if let Some(role) = &chosen.learnings {
        // WYCINA `scan::recurring_patterns`, NIE MY. Naiwne `text.find("## Recurring patterns")`
        // trafia w cytat blokowy z trzeciej linii każdego pliku roli u gospodarza i oddaje 131
        // bajtów zdania o tym, że reguły są wiążące, zamiast 1701 bajtów reguł [2026-08-19].
        // Przepisanie tego cięcia tutaj byłoby drugim znaczeniem słowa „sekcja" (niezmiennik 23).
        let file = host_text(project, LEARNINGS_DIR, A_LEARNINGS_FILE, role)?;
        let rules = scan::recurring_patterns(&file)?;
        if !rules.is_empty() {
            // Reszta pliku — u gospodarza do 73 KB `## Run journal` — nie wchodzi do budżetu
            // tokenów ani razu, i to jest cała różnica między wstrzykiwaczem a wklejeniem pliku.
            blocks.push(format!("{PATTERNS_HEADING}\n\n{rules}"));
        }
    }

    let text = blocks.join("\n\n");

    // Katalog pluginu powstaje TYLKO wtedy, gdy jest co do niego włożyć — pilnuje tego
    // `rewrite::plugin_dir`, który czyta wszystko przed pierwszym `create_dir_all`. Pusty
    // katalog przekazany vendorowi to plugin, który ładuje się i rejestruje zero umiejętności,
    // czyli dokładnie ta cicha zieleń, przed którą stoi całe to zadanie.
    let rewritten = rewrite::plugin_dir(project, &chosen.skills, &run_dir.join(PLUGIN_DIR))?;

    Ok(Inherited {
        // Fragment **dwuelementowy albo pusty, nigdy jednoelementowy** — rozstrzyga to
        // `rewrite::plugin_argv` po `names`, czyli po tym, co NAPRAWDĘ pojechało, a nie po tym,
        // o co poproszono.
        flags: rewrite::plugin_argv(&rewritten),
        text,
    })
}

/// Odmawia, jeśli człowiek wybrał umiejętność, której skan u gospodarza nie znalazł.
///
/// ODMOWA, NIE POMINIĘCIE, i to jest cały powód, dla którego ta funkcja istnieje osobno od
/// zapisu: `rewrite::plugin_dir` sam z siebie **pomija** nazwę, której u gospodarza nie ma
/// (dla niego to normalny stan cudzego repozytorium — ktoś mógł ją odznaczyć przed chwilą
/// w innym narzędziu), a różnicę widać wyłącznie w `Rewritten::names`, czyli w polu, którego
/// nikt nie czyta. Wtedy człowiek zaznacza pięć pozycji, agent dostaje trzy i nie ma jak się
/// o tym dowiedzieć: „agent nie zna umiejętności" jest z zewnątrz nieodróżnialne od „model nie
/// uznał, że warto jej użyć".
///
/// Pytamy [`scan::skills`], a nie systemu plików: to ta funkcja wyznacza, co człowiek widział
/// na ekranie wyboru (katalog bez `SKILL.md` nie ma tam wpisu), więc to samo pytanie musi
/// rozstrzygać tutaj. Drugi warunek dopisany przy zapisie byłby drugą definicją słowa
/// „znalezione" (niezmiennik 23).
fn every_name_is_really_there(project: &Path, selected: &[String]) -> Result<()> {
    if selected.is_empty() {
        // Pusty wybór nie ma czego nie znaleźć — i nie czytamy wtedy cudzego katalogu w ogóle,
        // bo bieg bez dziedziczenia nie ma powodu dotykać `.claude/` gospodarza.
        return Ok(());
    }

    let found = scan::skills(project)?;
    for name in selected {
        if !found.iter().any(|skill| skill.name == *name) {
            return Err(Error::NotInTheHost {
                what: A_SKILL,
                name: name.clone(),
            });
        }
    }
    Ok(())
}

/// Plik roli albo podagenta u gospodarza — albo odmowa **z nazwaniem pozycji**.
///
/// `name` przychodzi z ekranu wyboru, czyli z drutu, a wyznacza czytaną ścieżkę: `..`, `/etc`
/// albo `a/b` w tym polu znaczyłyby odczyt poza katalogiem, którego dotyczy ta operacja. Pytanie
/// zadajemy raz, w miejscu, w którym ścieżka się składa — to samo zdanie stoi przy
/// [`scan::skill_file`], które robi to dla umiejętności.
///
/// WYBRANA I NIEISTNIEJĄCA POZYCJA JEST ODMOWĄ, dokładnie jak przy umiejętnościach. Brak wyboru
/// to co innego i nikt tu wtedy nie zagląda: bieg bez doklejki jest wtedy poprawną odpowiedzią,
/// a nie stratą (niezmiennik 5). Odmowa dotyczy stanu, w którym człowiek **wskazał** plik,
/// którego u gospodarza nie ma — cicho pominięty daje bieg nieodróżnialny od tego, w którym
/// model po prostu nie skorzystał z reguł.
///
/// Bajty spoza UTF-8 nie kasują pliku: plik JEST tam, więc rola jest tam. Odczyt stratny oddaje
/// to, co człowiek wybrał; odmowa oddałaby mu bieg bez reguł z powodu, którego nie ma na żadnym
/// ekranie — to samo rozstrzygnięcie, z tego samego powodu, stoi w `scan::first_line`.
fn host_text(project: &Path, shelf: &str, what: &'static str, name: &str) -> Result<String> {
    let missing = || Error::NotInTheHost {
        what,
        name: name.to_owned(),
    };
    let path = one_file_named(name)
        .map(|file| project.join(HOST_DIR).join(shelf).join(file))
        .ok_or_else(missing)?;

    match fs::read(&path) {
        Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(missing()),
        // Awaria dysku to trzeci stan i o niej człowiek ma się dowiedzieć jako o awarii.
        Err(error) => Err(error.into()),
    }
}

/// `<nazwa>.md`, jeśli `nazwa` jest nazwą **jednego** pliku — inaczej `None`.
fn one_file_named(name: &str) -> Option<PathBuf> {
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        // Sprawdzamy nazwę PODANĄ, a rozszerzenie dokładamy po sprawdzeniu: `with_extension`
        // na nazwie `a.b` zjadłoby `.b` i przeczytało plik o innej nazwie niż ta, którą człowiek
        // zaznaczył — bez jednego słowa o podmianie.
        (Some(Component::Normal(_)), None) => Some(PathBuf::from(format!("{name}.md"))),
        _ => None,
    }
}
