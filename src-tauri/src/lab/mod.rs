//! Lab: zestaw przypadków, warianty i wszystko, co z nich powstaje.
//!
//! # Czym ten moduł JEST, w jednym akapicie
//!
//! Człowiek stoi w projekcie, ma w bibliotece agenta albo umiejętność i chce wiedzieć, czy
//! jego zmiana coś poprawiła. Odpowiedź wymaga trzech rzeczy naraz: **tego samego wejścia**
//! (inaczej porównuje dwie różne pytania), **werdyktu, którego nie wystawia model** (inaczej
//! mierzy sam siebie) i **historii** (inaczej wie tylko, jak jest teraz, a nie czy jest
//! lepiej). Ten moduł trzyma pierwsze i trzecie; drugie wystawia krok „sprawdź", który już
//! istnieje (`engine::drivers::command::passed`, niezmiennik 19).
//!
//! # Czego tu NIE MA i mieć nie będzie
//!
//! **Drugiego silnika.** Uruchomienie zestawu składa zwykły plik workflow ([`plan`]) i oddaje
//! go tej samej drodze, którą idzie każdy inny bieg: `commands::run`. Stąd bierze się za darmo
//! pula miejsc jedna na aplikację, sufit wydatku, dowód śmierci grupy, odzyskiwanie po awarii
//! aplikacji i historia per projekt. Własna pętla po przypadkach byłaby drugą implementacją
//! „ile naraz" — czyli dokładnie tym, jak umarł poprzedni prototyp (`AGENTS.md` §1).
//!
//! **Niezmiennik 27 zostaje nietknięty.** W `engine/` nie przybywa ani jedno słowo o Labie:
//! planista dostaje graf i go wykonuje, a to, że kroki tego grafu powstały z zestawu, jest
//! faktem wyłącznie po tej stronie.
//!
//! **Pliku z wynikami nie ma** — i to jest rozstrzygnięcie, nie brak. Wynik przebiegu daje się
//! wyliczyć z dwóch plików, które i tak istnieją: z **planu** (który krok należy do którego
//! przypadku i wariantu, plus czym ten wariant był w chwili przebiegu) i z `run.json` (co się
//! z tym krokiem stało). Trzeci plik byłby trzecim stanem do rozjechania się z dwoma
//! pozostałymi, pisanym po zakończeniu biegu — czyli dokładnie wtedy, kiedy aplikacja może
//! zginąć. Niezmiennik 21 mówi o tym wprost: nie pisz pliku, którego nikt nie czyta, a ten
//! czytałby wyłącznie sam siebie.
//!
//! # Dlaczego JSON, a nie front-matter jak notatka
//!
//! Bo przypadek jest zagnieżdżony: niesie listę oczekiwanych pól, własną komendę i wzorzec
//! dowodu. Płaski czytnik `klucz: wartość` z [`crate::memory::FrontMatter`] tego nie wyrazi,
//! a rozszerzenie go o zagnieżdżenie zamieniłoby go w drugi parser YAML-a dla całego repo.
//! Bliższym krewnym zestawu jest plik workflow — i stąd wzięty jest cały kształt: numer
//! formatu jako pierwszy klucz, odmowa-w-przód i `extra` na każdym poziomie, żeby starszy
//! build nie skasował po cichu pracy nowszego (`workflow::file`, T3 §8.4).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub mod cases;
pub mod file;
pub mod fix;
pub mod plan;
pub mod results;

/// Katalog zestawów wewnątrz projektu: `<projekt>/.loadout/evals/`.
///
/// **Tylko projektowy, nigdy biblioteczny**, i to jest treść, nie oszczędność. Przypadek
/// powstaje z materiału tego repozytorium — z jego zadań, jego komendy sprawdzającej, jego
/// plików. Ten sam plik w innym projekcie opisuje pracę, której tam nie ma, a jego komenda
/// nie ma czego uruchomić. Workflow ma dwie półki, bo kształt pracy bywa przenośny
/// (`commands::workflows::WorkflowPlace`); zestaw przenośny nie jest.
pub const EVALS_DIR: &str = "evals";

/// Katalog planów: `<projekt>/.loadout/evals/plans/`.
///
/// Plan jest **zwykłym plikiem workflow** i leży obok zestawu, a nie w bibliotece człowieka,
/// z dwóch powodów. Pierwszy jest mechaniczny: `commands::RunRequest::workflow` bierze ścieżkę
/// pliku, więc graf złożony w pamięci musi zejść na dysk, zanim ruszy bieg. Drugi jest
/// produktowy: plan da się otworzyć na płótnie i zobaczyć dokładnie to, co pobiegło — a plan
/// wrzucony do `~/.loadout/workflows/` zaśmiecałby listę workflow człowieka jedną pozycją na
/// każde uruchomienie zestawu.
pub const PLANS_DIR: &str = "plans";

/// Wersja formatu zestawu, którą pisze ten build.
///
/// Jedna wersja, dopóki nie ma drugiej (niezmiennik 25). Migracja „na przyszłość" jest tu
/// zakazana tak samo, jak w `workflow::file`.
pub const CURRENT: u32 = 1;

/// `<projekt>/.loadout/evals/`.
#[must_use]
pub fn project_evals(project: &Path) -> PathBuf {
    project.join(".loadout").join(EVALS_DIR)
}

/// `<projekt>/.loadout/evals/plans/`.
#[must_use]
pub fn project_plans(project: &Path) -> PathBuf {
    project_evals(project).join(PLANS_DIR)
}

/// Cały zestaw: `<projekt>/.loadout/evals/<slug>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalSet {
    /// Wersja formatu. **Pierwszy klucz**, czytany przed resztą — odmowa-w-przód
    /// ([`file::load`]) musi zadziałać także wtedy, gdy nowszy build zmienił kształt
    /// przypadku tak, że ten build nie umie go wczytać.
    pub format: u32,
    /// Stabilny identyfikator, zarazem nazwa pliku bez rozszerzenia. Nie zmienia się przy
    /// zmianie nazwy — dokładnie jak identyfikator agenta.
    pub id: String,
    /// To, co człowiek wpisał: „Review rubric".
    pub name: String,
    /// Czego ten zestaw dotyczy.
    pub subject: Subject,
    /// Przypadki w kolejności wstawiania, nigdy przesortowane: wiersz, który przeskakuje
    /// w tabeli po zapisie, jest wierszem, którego człowiek szuka od nowa.
    #[serde(default)]
    pub cases: Vec<Case>,
    /// Kolumny macierzy. Pusta lista jest poprawnym stanem świeżego zestawu.
    #[serde(default)]
    pub variants: Vec<Variant>,
    /// Klucze, których ta wersja nie zna — powód przy [`EvalSet::format`].
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl EvalSet {
    /// Przypadki, które naprawdę biegną: wyłącznie [`CaseStatus::InUse`].
    ///
    /// Filtr stoi **tutaj**, w jednym miejscu, i z tego samego powodu, dla którego
    /// `memory::notes::what_you_know` filtruje sam: gdyby wołający podawał już przefiltrowaną
    /// listę, filtr istniałby w dwóch miejscach, a drugie jest tym, do którego ktoś dopisuje
    /// „a na końcu jeszcze kandydatki, żeby było ich więcej". Kandydatka wpuszczona do pomiaru
    /// zamienia ewaluację w mierzenie samej siebie.
    #[must_use]
    pub fn running_cases(&self) -> Vec<&Case> {
        self.cases
            .iter()
            .filter(|case| case.status == CaseStatus::InUse)
            .collect()
    }

    /// Czy ten zestaw da się uruchomić — i **czego mu brakuje**, kiedy nie da się.
    ///
    /// Zdanie, nie `bool`: „nie da się uruchomić" bez powodu wysyła człowieka szukać po
    /// dziewięciu polach, który z nich zatrzymał bieg (ten sam powód stoi przy
    /// `commands::run::WRITE_RESULTS_TO`).
    #[must_use]
    pub fn why_it_cannot_run(&self) -> Option<String> {
        if self.variants.is_empty() {
            return Some(
                "This set has no columns yet, so there is nothing to compare. Add one and \
                 Loadout will run every case against it."
                    .to_owned(),
            );
        }
        // TRZY ODPOWIEDZI, NIE DWIE, i trzecia jest tu z pomiaru na żywym ekranie
        // (2026-08-31). „Every case here is still a suggestion" nad PUSTĄ listą jest zdaniem
        // o przypadkach, których nie ma — a człowiek czyta je jako „gdzieś tu są propozycje,
        // znajdź je" i szuka czegoś, czego nikt nie napisał. Zestaw bez ani jednego przypadku
        // i zestaw z samymi propozycjami to dwie różne rzeczy do zrobienia, więc mają dwa
        // różne zdania — a każde nazywa NASTĘPNY RUCH, nie stan.
        if self.cases.is_empty() {
            return Some(
                "This set has no cases yet. Press Write cases and an agent will draft some \
                 from this project."
                    .to_owned(),
            );
        }
        if self.running_cases().is_empty() {
            return Some(
                "Every case here is still a suggestion. Accept at least one and it will be \
                 part of the next run."
                    .to_owned(),
            );
        }
        None
    }
}

/// Czego ten zestaw dotyczy — i **czym różni się jego kolumna od kolumny obok**.
///
/// Dwa warianty, bo to są dwie różne rzeczy do zmieniania. Przy agencie kolumna zmienia
/// **kogo** pytamy; przy umiejętności kolumna zmienia **co ten ktoś ma pod ręką**, a agent
/// zostaje ten sam. Jedno pole „co testujemy" zlepiłoby te dwa pytania, a wtedy porównanie
/// dwóch kolumn nie mówi, która zmiana za nie odpowiada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Subject {
    /// Identyfikator agenta z biblioteki (`library::agents::Agent::id`).
    Agent { id: String },
    /// Nazwa katalogu umiejętności — ta sama, którą niesie `workflow::Skills`.
    Skill { name: String },
}

impl Subject {
    /// Identyfikator albo nazwa, jednym napisem — do złożenia domyślnej nazwy zestawu.
    #[must_use]
    pub fn said(&self) -> &str {
        match self {
            Self::Agent { id } => id,
            Self::Skill { name } => name,
        }
    }
}

/// Jeden przypadek: zadanie plus to, po czym poznać, że wyszło.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Case {
    /// Stabilny w obrębie zestawu. Wchodzi do identyfikatora kroku w planie, więc dwa
    /// przypadki o jednym identyfikatorze dałyby dwa kroki o jednym kluczu — i wynik jednego
    /// z nich przepadłby bez śladu. Pilnuje tego [`file::save`].
    pub id: String,
    /// Zdanie, które człowiek czyta w wierszu tabeli.
    pub name: String,
    /// Zadanie dla agenta, dosłownie. Jedzie do kroku jako `instructions`.
    pub task: String,
    /// Pola, które odpowiedź ma nieść — i czego się w nich spodziewamy.
    #[serde(default)]
    pub expect: Vec<Expect>,
    /// Komenda, która orzeka. Pusta znaczy „ten przypadek sądzą wyłącznie pola".
    #[serde(default)]
    pub command: String,
    /// Wzorzec dowodu do [`Case::command`] — ta sama notacja, co w kroku „sprawdź"
    /// i w linii `expect:` bramki repo: jeden metaznak `(\d+)`.
    #[serde(default)]
    pub proof: String,
    /// `suggested`, dopóki człowiek nie powie inaczej.
    #[serde(default)]
    pub status: CaseStatus,
    /// Skąd ten przypadek się wziął: `plik:linia`, ścieżka albo zdanie agenta.
    ///
    /// **Wymagane od kandydatki** ([`cases::read`] odrzuca kandydatkę bez tego pola), i to
    /// z tego samego pomiaru, co przy notatkach: reguła bez uzasadnienia jest regułą, której
    /// nikt nie umie ocenić, więc człowiek klika „accept" na wszystkim albo na niczym.
    #[serde(default)]
    pub because: String,
    /// Klucze, których ta wersja nie zna.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Case {
    /// Czy ten przypadek ma czym orzec.
    ///
    /// Koniunkcja, nie alternatywa: komenda bez wzorca spadłaby na sam kod wyjścia, a suita,
    /// która nie uruchomiła ani jednego testu, wychodzi zerem (niezmiennik 19). Przypadek bez
    /// komendy jest w porządku, dopóki ma choć jedno oczekiwane pole — wtedy orzeka
    /// `commands::run::missing_a_required_field`, po stronie biegu, tą samą drogą co zawsze.
    #[must_use]
    pub fn has_something_to_judge_it(&self) -> bool {
        let by_command = !self.command.trim().is_empty() && !self.proof.trim().is_empty();
        by_command || !self.expect.is_empty()
    }
}

/// Oczekiwanie wobec jednego pola odpowiedzi.
///
/// `contains`, nie równość: odpowiedź modelu jest niedeterministyczna, więc dosłowna równość
/// napisu świeciłaby na czerwono zawsze, a sprawdzenie, które zawsze jest czerwone, przestaje
/// być czytane po trzecim przebiegu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Expect {
    /// Nazwa pola, którą krok ma oddać w wierszu `nazwa: wartość`.
    pub field: String,
    /// Czego w nim szukamy. Pusty znaczy „wystarczy, że pole w ogóle jest".
    ///
    /// **Do promptu to nie wchodzi nigdy** — powód w całości stoi przy
    /// [`plan::handover_for`]: prompt mówiący „w tym polu ma paść słowo X" mierzy, czy model
    /// umie przepisać X.
    #[serde(default)]
    pub contains: String,
    /// Zdanie, którym prompt prosi o to pole. Puste znaczy „powiedz mu tylko, jak pole ma się
    /// nazywać".
    #[serde(default)]
    pub describe: String,
}

/// Dwa stany przypadku — te same dwa i z tego samego powodu, co przy notatce.
///
/// Awansuje wyłącznie człowiek. Materiał, który wpuszcza się do pomiaru sam, zamienia
/// ewaluację w mierzenie samej siebie; przy kandydatkach pisanych przez model to nie jest
/// ryzyko teoretyczne, tylko domyślne zachowanie.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaseStatus {
    /// Zaproponowana, jeszcze nie mierzy niczego.
    #[default]
    Suggested,
    /// Przyjęta przez człowieka. Biegnie.
    InUse,
}

/// Jedna kolumna macierzy: kto to robi i czym się różni od sąsiada.
///
/// **Wariant nie jest nowym bytem w bibliotece.** To jest agent plus ten sam patch RFC 7396,
/// którym nadpisuje się krok na płótnie (`workflow::AgentStep::overrides`) — więc „ten sam
/// agent, inny model" i „ten sam agent, jedna umiejętność więcej" wyrażają się bez ani jednego
/// nowego pojęcia. Drugi typ nadpisania byłby drugą implementacją scalania, a scalanie mieszka
/// w `library::agents::resolve` i ma tam zostać (niezmiennik 23).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    /// Stabilny w obrębie zestawu; wchodzi do identyfikatora kroku w planie.
    pub id: String,
    /// Nagłówek kolumny: „Reviewer · opus · deepest".
    pub name: String,
    /// Identyfikator agenta z biblioteki.
    pub agent: String,
    /// Patch nad jego definicją. `{}` znaczy „ten wariant bierze agenta, jaki jest".
    #[serde(default)]
    pub overrides: Map<String, Value>,
    /// Klucze, których ta wersja nie zna.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Nazwa pliku z tego, co człowiek wpisał — ta sama zasada, co przy workflow i umiejętności.
///
/// Wynik jest zawsze niepusty: nazwa złożona z samych znaków, które tu odpadają, dałaby pustą
/// nazwę pliku, a plik `.json` bez nazwy jest plikiem ukrytym, którego nikt nie znajdzie.
#[must_use]
pub fn slugify(said: &str) -> String {
    let mut out = String::with_capacity(said.len());
    let mut hyphen_is_pending = false;
    for character in said.chars() {
        if character.is_ascii_alphanumeric() {
            if hyphen_is_pending && !out.is_empty() {
                out.push('-');
            }
            hyphen_is_pending = false;
            out.extend(character.to_lowercase());
        } else {
            hyphen_is_pending = true;
        }
    }
    if out.is_empty() {
        // Nazwa złożona wyłącznie ze znaków spoza zestawu — cyrylica, emoji, same myślniki.
        // Napis zastępczy jest lepszy niż odmowa: człowiek nazwał zestaw tak, jak chciał,
        // a nazwa pliku jest naszym problemem, nie jego.
        return "set".to_owned();
    }
    out
}
