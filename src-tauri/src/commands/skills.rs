//! Komendy umiejętności: przeczytaj link i zainstaluj to, co człowiek przejrzał.
//!
//! **Ani jednego `use tauri::`** — jak w całym tym katalogu (`docs/ARCHITECTURE.md` §3).
//!
//! Cała polityka mieszka w `skills::ingest` (adres, limity, normalizacja, skan) i w
//! `skills::place` (walidacja, plan, zapis, sidecar). Tutaj jest wyłącznie to, czego tamte dwa
//! moduły nie mają, bo mieć nie mogły: **uruchomienie pobrania**. T-19 zapisał to wprost jako
//! decyzję — „prawdziwe pobieranie w bramce" jest poza jego zakresem, bo bramka, która wymaga
//! internetu, czerwieni się od cudzych awarii; „sieć żyje w aplikacji", czyli tutaj.
//!
//! # Bajty nie przechodzą przez okno
//!
//! [`review_skill_inner`] zapisuje pobraną umiejętność jako **kopię kanoniczną** w danych
//! aplikacji (`<biblioteka>/skills/<name>/`), a oknu oddaje sam przegląd. [`install_skill_into`]
//! czyta tę kopię z powrotem po nazwie. Treść, którą wykona agent, nie ma po co przechodzić
//! przez warstwę, która ją renderuje — a katalogi vendorów są wyjściem builda, którego źródłem
//! jest ta jedna kopia (niezmiennik 4, `skills::place::remove`).
//!
//! Drugi powód jest twardszy: `Import` po stronie okna niesie nazwę, streszczenie i przejrzane
//! ciało, a `skills::Skill` niesie jeszcze **pliki dołączone** ze ścieżkami źródłowymi, bo
//! instalacja kopiuje je przez `fs::copy` (bit wykonywalności `scripts/run.sh`). Instalacja
//! złożona z tego, co przyszło z okna, gubiłaby te pliki po cichu.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::commands::Drivers;
use crate::engine::drivers::{
    AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome, Policy, RunSpec,
};
use crate::engine::supervisor::GroupProof;
use crate::library::agents::{Agent, Overrides, resolve};
use crate::skills::ingest::{
    self, FILE_CAP, FetchError, Finding, Import, Reviewed, Target, Verdict, Weight,
};
use crate::skills::{Error, Roots, Scope, Skill, SkillDoc};

/// Kopie kanoniczne wewnątrz biblioteki: `~/.loadout/skills/<name>/`
/// (`docs/ARCHITECTURE.md` §8).
const SKILLS_DIR: &str = "skills";

/// Katalog, w którym ląduje pobranie, zanim wiadomo, jak umiejętność się nazywa.
///
/// Obok `skills/`, nie w środku: katalog roboczy pomiędzy nimi wyglądałby dla każdego, kto
/// wypisuje kopie kanoniczne, jak umiejętność o nazwie `incoming`.
const INCOMING_DIR: &str = "incoming";

/// Nazwa pliku umiejętności. Ta sama po obu stronach pobrania.
const SKILL_FILE: &str = "SKILL.md";

/// Plik, w którym Loadout notuje, skąd wzięła się umiejętność o danej nazwie.
///
/// Leży OBOK kopii kanonicznych, w tym samym katalogu, co sidecar instalacji — bo to jest zapis
/// Loadouta o umiejętnościach, a nie plik którejkolwiek z nich. Powód, dla którego nie może
/// stać ani w środku katalogu umiejętności, ani w `installed.json`, stoi przy
/// [`remember_origin`].
const ORIGINS_FILE: &str = "origins.json";

/// Ile bajtów odmowy `curl`-a czytamy, zanim przestaniemy. Zdanie dla człowieka ma się zmieścić
/// w jednej linii, a `--show-error` tyle właśnie produkuje; sufit jest po to, żeby wywrócony
/// serwer nie mógł nam oddać megabajta „błędu".
const COMPLAINT_CAP: u64 = 4_096;

/// Jedno znalezisko tak, jak widzi je okno.
///
/// Lustro `Finding` z `src/state/skills.ts`. `id` nie ma odpowiednika w `ingest::Finding` i nie
/// powinno mieć: tożsamością znaleziska jest para (reguła, linia) — „jedno znalezisko na parę
/// (reguła, linia)" mówi wprost doc tamtej struktury — więc `id` jest tu **wyprowadzone**, a nie
/// wymyślone. Karta przeglądu bierze je do `acknowledge`, czyli do zdania „człowiek przeczytał
/// TO znalezisko", a nie „przeczytał tę regułę".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingWire {
    pub id: String,
    pub rule: String,
    /// `warn` albo `block`.
    pub weight: String,
    /// Numer linii liczony od 1, `null` kiedy znalezisko nie dotyczy żadnej konkretnej.
    pub line: Option<usize>,
    /// Linia zacytowana dosłownie. Człowiek ma przeczytać atak, nie jego opis.
    pub quoted: String,
    /// Tekst ZDJĘTY z ciała. `null` dla wszystkiego poza `hidden-text`.
    pub recovered: Option<String>,
}

impl From<&Finding> for FindingWire {
    fn from(finding: &Finding) -> Self {
        Self {
            // Reguła i linia, bo to jest para, która identyfikuje znalezisko. Znalezisko bez
            // linii (skaner, który nie ruszył) dostaje `-`, żeby dwa takie z różnych reguł nie
            // zlały się w jeden wiersz do przeczytania.
            id: match finding.line {
                Some(line) => format!("{}:{line}", finding.rule),
                None => format!("{}:-", finding.rule),
            },
            rule: finding.rule.clone(),
            weight: match finding.weight {
                Weight::Warn => "warn",
                Weight::Block => "block",
            }
            .to_owned(),
            line: finding.line,
            quoted: finding.quoted.clone(),
            recovered: finding.recovered.clone(),
        }
    }
}

/// Przegląd treści tak, jak widzi go okno. Lustro `Reviewed` z `src/state/skills.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewWire {
    /// Ciało dokładnie takie, jakie poszło na dysk — i dokładnie to, które przeskanowaliśmy.
    pub body: String,
    pub findings: Vec<FindingWire>,
    /// `clean`, `concerns` albo `blocked`.
    pub verdict: String,
}

/// Pobrana umiejętność, przejrzana, jeszcze przed pierwszym zapisem w katalogu vendora.
///
/// Lustro `Import` z `src/state/skills.ts`. Węższe od `ingest::Import` z rozmysłem — powód stoi
/// w nagłówku modułu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportWire {
    /// Nazwa umiejętności, ona też jest nazwą katalogu.
    pub name: String,
    /// `description` z `SKILL.md` — jedyne pole, po którym model decyduje, czy sięgnąć.
    pub summary: String,
    pub reviewed: ReviewWire,
    /// Ile dołączonych skryptów niesie umiejętność. LICZONE z tego, co przyszło.
    pub scripts: usize,
    /// Czy przyszła z sieci. Znacznik jest trwały i przeżywa instalację [T5 §5.4].
    pub from_the_internet: bool,
}

impl From<&Import> for ImportWire {
    fn from(import: &Import) -> Self {
        Self {
            name: import.skill.name.clone(),
            summary: import.skill.description.clone(),
            reviewed: ReviewWire::from(&import.reviewed),
            scripts: import.scripts,
            // OSTROŻNY DOMYŚLNY, i to jest jedyne pole tej struktury, którego nie ma w pliku.
            // Do 2026-08-19 była to prosta prawda: [`review_skill_inner`] był jedyną drogą,
            // którą cokolwiek tu wchodziło. Od chwili, w której [`author_skill_inner`] też
            // buduje `ImportWire`, ta przesłanka przestaje obowiązywać — więc ta konwersja
            // trzyma stronę bezpieczną (znacznik zastępuje podpisy, których v1 nie ma), a droga
            // formularza nadpisuje ją JAWNIE, bo tylko ona wie o pochodzeniu więcej niż plik.
            from_the_internet: true,
        }
    }
}

impl From<&Reviewed> for ReviewWire {
    fn from(reviewed: &Reviewed) -> Self {
        Self {
            body: reviewed.body.clone(),
            findings: reviewed.findings.iter().map(FindingWire::from).collect(),
            verdict: match reviewed.verdict {
                Verdict::Clean => "clean",
                Verdict::Concerns => "concerns",
                Verdict::Blocked => "blocked",
            }
            .to_owned(),
        }
    }
}

/// Trzy pytania z formularza, dokładnie te trzy [T5 §8.3]. Lustro `Authored`
/// z `src/state/skills.ts`.
///
/// DLACZEGO NIE `ImportWire` W DRUGĄ STRONĘ. Okno przysyła to, co człowiek **wpisał**, a nie
/// umiejętność: nie ma tu ani nazwy katalogu, ani przejrzanego ciała, ani werdyktu. Wszystkie
/// trzy powstają po tej stronie granicy i to jest cała treść tego typu — gdyby okno przysyłało
/// `name` gotowe do wpisania w ścieżkę, byłoby drugim miejscem, w którym liczy się slug
/// (niezmiennik 13), i pierwszym, które da się skierować gdziekolwiek.
///
/// Nazwy pól są nazwami pytań, a nie nazwami pól `SKILL.md`: człowiek odpowiada „kiedy tego
/// użyć", a `description` jest tym, w co ta odpowiedź się zamienia. Zamiana zachodzi w jednym
/// miejscu ([`author_skill_inner`]) i tylko dlatego da się o niej cokolwiek powiedzieć.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Authored {
    /// Nazwa tak, jak ją wpisał człowiek — zdaniem, ze spacjami i wersalikami. Slug liczy
    /// [`slug_of`], nigdy okno.
    pub name: String,
    /// „Kiedy tego użyć" → `description`. Jedyne pole, po którym model decyduje, czy sięgnąć.
    pub when_to_use: String,
    /// „Co zrobić" → ciało `SKILL.md`. Tekst nieufny dokładnie tak samo jak wklejony z linku:
    /// człowiek potrafi wkleić tu cudzy akapit, a od T-43 wkleja tu draft modelu.
    pub what_to_do: String,
}

/// Gdzie umiejętność ma wylądować — pytanie z okna, w słowach okna [T5 §8.3].
///
/// DWIE WARTOŚCI I ANI JEDNEJ WIĘCEJ, bo tyle znają vendorzy: „u mnie" i „w tym repo".
///
/// OSOBNY TYP OD [`Scope`], mimo że odwzorowanie jest jeden-do-jednego — i to nie jest warstwa
/// tłumaczeń na zapas. [`Scope`] jest słowem RDZENIA: `skills::place` liczy nim ścieżki i to on
/// zostaje, kiedy okna nie ma (osobny daemon). To jest słowo DRUTU, czyli pozycja wyboru, którą
/// człowiek czyta jako „This project" i „Everywhere" — a enum z drutu nigdy nie trafia na ekran
/// (niezmiennik 14), więc napis dla człowieka liczy okno i tylko okno.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Landing {
    /// Korzeń otwartego projektu — umiejętność jedzie z nim do zespołu.
    ThisProject,
    /// Katalog domowy — umiejętność widoczna w każdym projekcie.
    Everywhere,
}

/// Słowo drutu → słowo rdzenia. JEDNO miejsce, w którym te dwa enumy się spotykają.
///
/// 2026-08-19 — do tego dnia tego odwzorowania nie było wcale, bo nie było czego odwzorowywać:
/// [`install_skill_into`], [`delete_skill_from`] i [`list_skills_in`] miały wpisane
/// `Scope::Global`, więc cały zakres projektowy z T-18 był napisany, przetestowany
/// i nieosiągalny z aplikacji.
///
/// DLACZEGO `From`, A NIE `match` W KAŻDYM Z TRZECH WOŁAJĄCYCH. Bo trzy odwzorowania to trzy
/// okazje, żeby raz odwrócić kierunek — a rozjazd objawia się dopiero jako umiejętność zapisana
/// w innym korzeniu niż ten, który człowiek przeczytał na ekranie, i to w żywej konfiguracji
/// jego narzędzi agentowych. Jedna odpowiedź na pytanie „czym jest »ten projekt«" (niezmiennik 13).
impl From<Landing> for Scope {
    fn from(landing: Landing) -> Self {
        match landing {
            Landing::ThisProject => Self::Project,
            Landing::Everywhere => Self::Global,
        }
    }
}

/// Korzenie rozmieszczania: dom wyprowadzony z katalogu biblioteki, korzeń projektu z okna.
///
/// `~/.loadout` leży **w** katalogu domowym, więc jego rodzic jest tym katalogiem. To nie jest
/// oszczędność argumentu: jedyne pytanie o `HOME` w całej aplikacji stoi w `lib.rs::loadout_dir`
/// i ma tam zostać jedno (niezmiennik 13). Drugi odczyt `HOME` tutaj znaczyłby też, że każdy
/// test pisze do prawdziwych katalogów vendorów.
///
/// 2026-08-19 — KORZEŃ PROJEKTU PRZYJEŻDŻA ARGUMENTEM I NIGDY NIE JEST TU ZGADYWANY. Do tego dnia
/// stała tu funkcja `global_roots` z wpisanym `project: None` i była JEDYNYM konstruktorem
/// [`Roots`] w produkcji — czyli cały zakres projektowy z T-18 był napisany, przetestowany
/// i nieosiągalny z aplikacji. `None` znaczy „nie ma otwartego projektu"; wtedy
/// [`crate::skills::place::plan`] odmawia zakresu projektowego, zamiast pisać po katalogu
/// roboczym procesu (`destinations` oddaje bez korzenia ścieżki WZGLĘDNE).
#[must_use]
fn roots_for(library: &Path, project: Option<&Path>) -> Roots {
    Roots {
        home: library.parent().unwrap_or(library).to_path_buf(),
        project: project.map(Path::to_path_buf),
        data: library.to_path_buf(),
    }
}

/// Umiejętność, która naprawdę leży w katalogach agentów. Lustro `InstalledSkill`
/// z `src/state/skills.ts`, pole w pole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledWire {
    /// Nazwa umiejętności, ona też jest nazwą katalogu.
    pub name: String,
    /// Czy przyszła z sieci. Znacznik jest trwały i przeżywa instalację [T5 §5.4].
    pub from_the_internet: bool,
    /// Po co ta umiejętność jest — pole `description` z jej `SKILL.md`, w jednym wierszu.
    ///
    /// 2026-08-23 — DO DZIŚ KAFELEK NIE MIAŁ CO POWIEDZIEĆ. Lista niosła nazwę katalogu i nic
    /// poza tym, więc sekcja Umiejętności była siatką gołych napisów: żeby dowiedzieć się, co
    /// dana umiejętność robi, trzeba było otworzyć plik poza aplikacją. Makieta ma w kafelku
    /// zdanie opisu od początku (`docs/mockup/index.html`, panel `skills`) i nie dało się go
    /// zbudować, bo dane kończyły się na tej granicy.
    ///
    /// PUSTY NAPIS, NIE `Option`: „ta umiejętność nie mówi, po co jest" jest faktem o pliku,
    /// a nie brakiem odpowiedzi — i kafelek ma go pokazać tak samo uczciwie jak każdy inny.
    pub summary: String,
}

/// Co leży w katalogach agentów — odczytane z DYSKU, bez ani jednego bajtu pamięci procesu.
///
/// 2026-08-18 — powód, dla którego ta funkcja istnieje, jest zmierzony i brzmi tak: sekcja
/// Umiejętności trzymała listę `installed` wyłącznie w magazynie okna i dopisywała do niej
/// po udanym `install_skill`. Restart kasował listę, a pliki zostawały — czyli licznik
/// „N saved" mówił „ile dodałeś w tej sesji", udając, że mówi „ile masz". Niezmiennik 4
/// złamany wprost.
///
/// # Dlaczego katalogi vendorów, a nie kopie kanoniczne
///
/// `review_skill_inner` odkłada kopię kanoniczną w `<biblioteka>/skills/<name>/` ZANIM człowiek
/// cokolwiek zatwierdzi — przegląd, który skończył się „nie, dziękuję", leży tam tak samo jak
/// przyjęty. Lista czytana stamtąd pokazywałaby jako zainstalowane wszystko, co ktoś kiedykolwiek
/// wkleił jako link. Zainstalowana znaczy „agent ją widzi", a to jest pytanie o katalogi
/// docelowe — te same, które wylicza [`crate::skills::place::destinations`]. Ścieżki liczy więc
/// tamta funkcja, nie ta (niezmiennik 23): drugie miejsce, w którym stoi `.claude/skills`,
/// rozjechałoby się z pierwszym przy pierwszym vendorze, którego dołożymy.
///
/// # Skąd bierze się `from_the_internet`
///
/// Z plików, bo inaczej nie wolno go zapisać (niezmiennik 4) — ale od 2026-08-19 z ZAPISU,
/// a nie z wniosku. Do tego dnia odpowiadała na to samo pytanie obecność kopii kanonicznej,
/// i była to prawda przez konstrukcję: kopie kanoniczne powstawały **wyłącznie**
/// w [`review_skill_inner`], czyli na jedynej drodze, którą cokolwiek wchodziło tu z sieci.
/// [`author_skill_inner`] też odkłada kopię kanoniczną, więc ta przesłanka przestała
/// obowiązywać — a wniosek wyciągany z niej dalej mówiłby „z internetu" o tekście, który
/// człowiek wpisał w tym oknie palcami. Odpowiada więc [`remember_origin`], czyli plik
/// zapisany jawnie.
///
/// **Nieobecność zapisu jest ostrożnym „tak", nie „napisana tutaj".** Biblioteka starsza niż to
/// zadanie nie ma o swoich umiejętnościach ani jednego wiersza, a wtedy jedyna wiedza, jaka
/// została, to tamta stara przesłanka: kopia kanoniczna znaczy link. Znacznik zastępuje podpisy
/// i weryfikację pochodzenia, których v1 nie ma, więc ma świecić wszędzie tam, gdzie treść MOŻE
/// być cudza — to ta sama reguła, którą trzymają `DeepScan::Unavailable` i `Discovery::Unknown`:
/// brak dowodu nie jest dowodem braku. Domyślny odwrotny („nie wiem, czyli pewnie własna") gasi
/// go dokładnie na tych umiejętnościach, na których jest potrzebny.
///
/// Katalog vendora bez kopii kanonicznej i bez zapisu (ktoś napisał umiejętność wprost tam)
/// dostaje `false` — nic jej nigdy nie pobierało.
///
/// Katalog, którego nie ma, daje **pustą listę**. Brak umiejętności to stan, nie awaria —
/// czerwony pasek na świeżej instalacji uczy człowieka ignorować czerwone paski.
///
/// # Po co tu korzeń projektu
///
/// Bo lista odpowiada na pytanie „co widzi agent pracujący TUTAJ", a nie „co kiedykolwiek
/// zapisaliśmy" (niezmiennik 4: pliki są prawdą). Umiejętność zapisana „w tym projekcie"
/// i niewidoczna na liście jest umiejętnością, której człowiek nie ma jak zabrać — a ta sekcja
/// pisze do żywej konfiguracji jego narzędzi agentowych. `None` znaczy „nie ma otwartego
/// projektu" i wtedy widać wyłącznie korzeń globalny.
/// Zdanie „po co to jest" z `SKILL.md` tej umiejętności — albo pusty napis.
///
/// Czyta JEDNYM czytnikiem front-mattera (`place::read_doc` + `place::field`), a nie własnym
/// `split(':')`: pole `description: "a: b"` rozjeżdża każdy ręcznie napisany rozbiór, a trzecia
/// kopia tej reguły byłaby tą, która o cudzysłowach nie wie (niezmiennik 13).
///
/// Nieczytelny plik daje pusty napis, nie odmowę: lista, która pada przez jeden uszkodzony
/// `SKILL.md`, zabiera człowiekowi także te umiejętności, z którymi wszystko jest w porządku
/// (niezmiennik 5).
///
/// Białe znaki zwijane do pojedynczych spacji: `description` bywa w YAML-u złamany na dwa
/// wiersze, a kafelek ma jeden.
fn summary_of(dir: Option<&PathBuf>) -> String {
    let Some(dir) = dir else { return String::new() };
    let Ok(text) = std::fs::read_to_string(dir.join(SKILL_FILE)) else {
        return String::new();
    };
    let doc = crate::skills::place::read_doc(&text);
    crate::skills::place::field(&doc, "description")
        .map(|one| one.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

pub fn list_skills_in(library: &Path, project: Option<&Path>) -> Result<Vec<InstalledWire>, Error> {
    let roots = roots_for(library, project);
    // Zbiór, nie wektor: ta sama umiejętność stoi w OBU katalogach docelowych, bo instalacja
    // pisze w oba. Lista z powtórzeniem pokazałaby człowiekowi dwa wiersze o jednym pliku
    // i policzyłaby go dwa razy w liczniku nad sekcją.
    let mut names: BTreeSet<String> = BTreeSet::new();
    // Gdzie leży plik każdej z nich — wyłącznie po to, żeby przeczytać z niego opis.
    let mut where_found: BTreeMap<String, PathBuf> = BTreeMap::new();

    // OBA KORZENIE, KIEDY PROJEKT JEST OTWARTY, i to jest połowa tej funkcji. Lista odpowiada
    // na pytanie „co widzi agent pracujący TUTAJ", a agent zagląda w oba drzewa — więc korzeń
    // pominięty tutaj jest umiejętnością, której człowiek nie zobaczy i **nie będzie miał jak
    // zabrać**, choć leży w żywej konfiguracji jego narzędzi agentowych. Bez otwartego projektu
    // widać wyłącznie globalny: „co kiedykolwiek zapisaliśmy" jest innym pytaniem, a katalog
    // w cudzym repozytorium jest osiągalny tylko z jego wnętrza.
    //
    // Ścieżki liczy dalej WYŁĄCZNIE `place::destinations` (niezmiennik 23) — drugie miejsce,
    // w którym stoi `.claude/skills`, rozjechałoby się z pierwszym przy pierwszym vendorze,
    // którego dołożymy.
    let mut dirs = Vec::from(crate::skills::place::destinations(
        Scope::Global,
        &roots.home,
        roots.project.as_deref(),
    ));
    if roots.project.is_some() {
        dirs.extend(crate::skills::place::destinations(
            Scope::Project,
            &roots.home,
            roots.project.as_deref(),
        ));
    }

    for dir in dirs {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            // Nikt jeszcze nic nie zainstalował — zero umiejętności, nie błąd.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            // Każda inna awaria odczytu jedzie w górę. Pusta lista w odpowiedzi na „nie mam
            // prawa czytać tego katalogu" jest zdaniem „nic tam nie ma", a to nieprawda.
            Err(error) => return Err(Error::Io(error)),
        };

        for entry in entries.flatten() {
            // Katalog z `SKILL.md` w środku, a nie każdy wpis: obok katalogów umiejętności
            // leżą pliki vendorów i `.DS_Store`, a wiersz „umiejętność .DS_Store" jest
            // dokładnie tym rodzajem śmiecia, przez który człowiek przestaje czytać listę.
            if entry.path().join(SKILL_FILE).is_file() {
                let name = entry.file_name().to_string_lossy().into_owned();
                /* Pierwszy katalog wygrywa i to jest celowe: ta sama umiejętność stoi w obu
                 * drzewach vendorów, bo instalacja pisze w oba z JEDNEGO źródła — więc opis
                 * jest ten sam, a odczyt z każdego po kolei byłby N otwarciami tego samego
                 * zdania. */
                where_found
                    .entry(name.clone())
                    .or_insert_with(|| entry.path());
                names.insert(name);
            }
        }
    }

    // Zapis czytany RAZ, przed pętlą: plik jest jeden na bibliotekę, a odczyt per umiejętność
    // znaczyłby N otwarć tego samego pliku i N różnych odpowiedzi, gdyby ktoś pisał w niego
    // w trakcie.
    let origins = origins_of(library);

    Ok(names
        .into_iter()
        .map(|name| InstalledWire {
            from_the_internet: origins.get(&name).copied().unwrap_or_else(|| {
                // Bez zapisu zostaje wyłącznie przesłanka sprzed tego zadania — i wolno jej
                // ufać dokładnie w tę jedną stronę, bo do 2026-08-19 kopie kanoniczne
                // powstawały tylko na drodze linku. Powód, dla którego ostrożny kierunek jest
                // tu jedynym uczciwym, stoi w doc tej funkcji.
                library
                    .join(SKILLS_DIR)
                    .join(&name)
                    .join(SKILL_FILE)
                    .is_file()
            }),
            summary: summary_of(where_found.get(&name)),
            name,
        })
        .collect())
}

/// Zdejmuje katalog, jeżeli tam jest. Brak katalogu to **nie** jest awaria.
///
/// Osobna funkcja, bo pobranie robi to dwa razy — raz na katalogu roboczym, raz na kopii
/// kanonicznej — a droga formularza trzeci; „nie ma go, czyli już jest zdjęty" napisane trzy razy
/// to trzy okazje, żeby raz pomylić kierunek porównania i zacząć wywracać się na pierwszym
/// imporcie każdej umiejętności.
///
/// Zwraca `std::io::Error`, a nie enum jednej z dróg: obie umieją go przyjąć przez `From`, więc
/// `?` w każdej z nich zachowuje przyczynę bez przepisywania jej przez `to_string()`.
fn gone(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Bajty spod adresu, pobrane komendą z `skills::ingest`.
///
/// Komendę buduje `ingest::build_fetch_command` i to jest jedyne miejsce, w którym powstaje
/// wywołanie `curl`-a: flagi (`--proto '=https'`, `--max-redirs`, `--max-filesize`, `--max-time`),
/// `env_clear` i adres podany **stdinem, nie w argv** (niezmiennik 9) mieszkają tam. Ta funkcja
/// wyłącznie uruchamia to, co tamta zbudowała.
///
/// Limit czytamy drugi raz u siebie, na bajtach, które faktycznie przyszły: `--max-filesize`
/// w argv jest deklaracją narzędzia, nie dowodem (niezmiennik 20) — `curl` nie egzekwuje go dla
/// odpowiedzi bez `Content-Length`.
fn fetched(url: &str) -> Result<Vec<u8>, FetchError> {
    let mut child = ingest::build_fetch_command(url).spawn()?;

    let taken = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("there was nothing to read from the download"))?;
    // `read_capped` bierze uchwyt przez wartość, więc porzuca go razem z błędem — `curl`
    // dostaje wtedy zerwaną rurę i schodzi sam. Bez tego proces zostałby sierotą, a
    // `child.wait()` niżej czekałby na niego do końca świata.
    let body = ingest::read_capped(taken, FILE_CAP);

    // Stderr czytamy PRZED `wait`: `curl` z pełnym buforem błędu nie zszedłby, a my czekalibyśmy
    // na jego zejście. Zdanie i tak jest jednolinijkowe (`--silent --show-error`), ale czekanie
    // na proces, którego nikt nie czyta, jest zakleszczeniem, nie ryzykiem.
    let complaint = child.stderr.take().map_or_else(String::new, |stream| {
        let bytes = ingest::read_capped(stream, COMPLAINT_CAP).unwrap_or_default();
        String::from_utf8_lossy(&bytes).trim().to_owned()
    });

    let status = child.wait()?;
    let body = body?;

    if !status.success() {
        // Zdanie `curl`-a, kiedy jakieś powiedział. Własne „download failed" byłoby drugim
        // opisem tej samej awarii, a to pierwsze niesie kod HTTP, który mówi, co poprawić.
        return Err(FetchError::Io(std::io::Error::other(
            if complaint.is_empty() {
                format!("Loadout could not download {url}")
            } else {
                complaint
            },
        )));
    }
    Ok(body)
}

/// Adres → umiejętność przejrzana i odłożona jako kopia kanoniczna.
///
/// `library` to `~/.loadout` i przychodzi **argumentem**, nigdy z `HOME` czytanego w środku —
/// ten sam powód, co przy `RunDeps::home`.
///
/// 2026-08-16 — obsłużony jest **wyłącznie** link prosto do `SKILL.md`. `Target::Folder`
/// (podkatalog repozytorium) i `Target::Gist` wymagają wypisania plików przez API `GitHuba`,
/// czego w tym drzewie nie ma ani jednej linii, a policzenie dołączonych skryptów jest liczbą,
/// którą karta przeglądu pokazuje człowiekowi przed instalacją [T5 §8.3]. Zwrócenie tam zera
/// byłoby więc kłamstwem dokładnie w tym miejscu, w którym człowiek decyduje o
/// bezpieczeństwie — dlatego te dwa kształty odmawiają zdaniem, które mówi, co zrobić.
pub fn review_skill_inner(library: &Path, url: &str) -> Result<ImportWire, FetchError> {
    // Polityka adresu PIERWSZA i niezmieniona: https, host z listy, kształt linku. Czysta
    // funkcja z `ingest`, więc odmowa pada przed dotknięciem sieci.
    match ingest::resolve_url(url)? {
        Target::RawFile => {}
        Target::Folder { .. } | Target::Gist => {
            return Err(FetchError::Io(std::io::Error::other(
                "Loadout can read a link straight to a SKILL.md file. Open the skill's SKILL.md \
                 and paste that link.",
            )));
        }
    }

    let incoming = library.join(INCOMING_DIR);
    // Świeży katalog przy każdym pobraniu: plik z poprzedniego, nieudanego importu wjechałby do
    // tego jako plik dołączony — czyli do przeglądu, który człowiek zatwierdza.
    gone(&incoming)?;
    std::fs::create_dir_all(&incoming)?;
    std::fs::write(incoming.join(SKILL_FILE), fetched(url)?)?;

    // Normalizacja, skan i policzenie skryptów mieszkają w `from_folder`. Ta warstwa nie ogląda
    // treści ani razu.
    let import = ingest::from_folder(&incoming)?;

    // ODMOWA NAZWY PRZED PIERWSZYM `remove_dir_all`, i to jest naprawa defektu istniejącego
    // dziś na trunku, osiągalnego z okna: do 2026-08-19 stała tu jedna linia licząca ścieżkę
    // z pola `name` pobranego pliku. `SKILL.md` bez tego pola daje `Skill::default()`, czyli
    // `name: ""`, czyli ścieżkę `<biblioteka>/skills/` — a `gone()` niżej kasuje wtedy WSZYSTKIE
    // kopie kanoniczne razem z `installed.json`. `name: ../../x` wychodzi poza bibliotekę.
    // Obie drogi wejścia liczą tę samą ścieżkę i oddają ją temu samemu `gone()`, więc obie
    // pytają teraz tę samą funkcję.
    //
    // `FetchError` nie ma wariantu na „nazwa nie przechodzi walidacji" i nie dostanie go tutaj:
    // `skills/mod.rs` nie należy do tego zadania (AGENTS.md §7). Zdanie walidatora jedzie więc
    // w `Io` — jedynym wariancie niosącym cudzy tekst — i ląduje na ekranie słowo w słowo, bo
    // `ipc.rs` woła na tym `to_string()`.
    let canonical = canonical_for(library, &import.skill.name)
        .map_err(|refused| FetchError::Io(std::io::Error::other(refused.to_string())))?;
    gone(&canonical)?;
    if let Some(parent) = canonical.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // `rename`, nie kopiowanie: kopia kanoniczna ma powstać jednym ruchem, żeby nie istniał
    // moment, w którym `install_skill_into` widzi katalog w połowie zapisany.
    std::fs::rename(&incoming, &canonical)?;

    // Czytamy jeszcze raz, z miejsca docelowego: `Import` z `incoming` niesie ścieżki źródłowe
    // plików dołączonych, a te po przeniesieniu wskazują na katalog, którego już nie ma.
    Ok(ImportWire::from(&ingest::from_folder(&canonical)?))
}

/// Nazwa katalogu policzona z tego, co człowiek wpisał w pierwsze pole.
///
/// JEDNO MIEJSCE, w którym powstaje slug (niezmiennik 13). Slug jest **tym samym faktem**, co
/// nazwa katalogu na dysku: to, co formularz pokazuje człowiekowi pod polem („to wyląduje jako
/// `review-pull-requests`"), i to, co `place::plan` wpisze w ścieżkę. Drugie liczenie w oknie
/// rozjechałoby się z tym na pierwszym znaku spoza ASCII — a rozjazd objawia się dopiero jako
/// katalog o innej nazwie niż zdanie, które człowiek przeczytał przed naciśnięciem przycisku.
///
/// Ta funkcja **nie odmawia**. Odmowa jest jedna i mieszka w [`canonical_for`], razem
/// z komunikatami walidatora — slug, którego nie da się przyjąć (puste wejście, samo `Claude`),
/// ma zostać nazwany zdaniem rdzenia, a nie zdaniem wymyślonym tutaj drugi raz.
#[must_use]
pub fn slug_of(typed: &str) -> String {
    let mut slug = String::with_capacity(typed.len());

    // `to_lowercase`, nie `to_ascii_lowercase`: wersalik spoza ASCII ma zejść do małej litery,
    // ZANIM zapytamy, czy umiemy go zapisać. Inaczej „Śledzenie zmian" gubi pierwszą literę
    // i katalog nazywa się `ledzenie-zmian` — czyli nie tak, jak nazwał go człowiek.
    for character in typed.to_lowercase().chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            slug.push(character);
        } else if let Some(letters) = without_accent(character) {
            slug.push_str(letters);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            // Wszystko inne jest GRANICĄ SŁOWA, nie znakiem do zapisania: odstęp, przecinek,
            // półpauza, a także litera pisma, którego nie umiemy przepisać. Warunek pilnuje
            // obu reguł `is_slug` naraz — ani łącznika wiodącego, ani podwójnego.
            slug.push('-');
        }
    }

    // Łącznik końcowy bierze się z interpunkcji zamykającej („Ship it — fast, safely!"),
    // a `is_slug` odrzuca go tak samo jak wiodący.
    slug.trim_end_matches('-').to_owned()
}

/// Ta sama litera zapisana po ASCII — albo `None`, kiedy nie umiemy jej przepisać.
///
/// DLACZEGO TABLICA, A NIE ROZKŁAD UNICODE. Rozkład kanoniczny (NFD plus zdjęcie znaków
/// łączących) to zależność na `unicode-normalization` i odpowiedź na pytanie szersze niż to,
/// które tu stoi. Tu wystarczy: polskie dziewięć, bo w tym języku napisane jest to repo, i jeden
/// ciągły blok Latin-1 Supplement, bo kosztuje pięć zakresów.
///
/// Litera spoza tej listy jest granicą słowa, a nie znakiem wyrzuconym po cichu: nazwa złożona
/// wyłącznie z takich liter daje pusty slug i kończy się ODMOWĄ walidatora, zamiast katalogiem
/// o nazwie, w której człowiek nie rozpozna tego, co wpisał.
fn without_accent(character: char) -> Option<&'static str> {
    // Jedna gałąź na literę WYJŚCIOWĄ, nie na wejściową: polskie i te z Latin-1 stoją obok
    // siebie, bo `clippy::match_same_arms` czyta dwie gałęzie o tym samym ciele jako powtórzenie.
    // Zakresy pokrywają całe bloki Latin-1 (`à`–`å`, `è`–`ë`, `ì`–`ï`, `ò`–`ö`, `ù`–`ü`), więc
    // polskie `ó` siedzi w jednym z nich i nie ma własnej pozycji.
    Some(match character {
        'ą' | 'à'..='å' => "a",
        'ć' | 'ç' => "c",
        'ę' | 'è'..='ë' => "e",
        'ì'..='ï' => "i",
        'ł' => "l",
        'ń' | 'ñ' => "n",
        'ò'..='ö' | 'ø' => "o",
        'ś' => "s",
        'ù'..='ü' => "u",
        'ý' | 'ÿ' => "y",
        'ź' | 'ż' => "z",
        // Dwuznaki: jedna litera na wejściu, dwie na wyjściu.
        'ß' => "ss",
        'æ' => "ae",
        _ => return None,
    })
}

/// Opis-zastępnik na czas pytania o samą nazwę.
///
/// `place::validate_strict` odpowiada o CAŁYM dokumencie, a [`canonical_for`] pyta o jeden człon
/// ścieżki. Bez tego pola do odmowy o nazwie dołączałoby „Missing required field in frontmatter:
/// description" — zdanie o czymś, o co nikt tutaj nie pytał, i którego człowiek stojący nad
/// pierwszym polem formularza nie ma jak naprawić. Ten napis nie jedzie ani na ekran, ani do
/// pliku: wchodzi do walidatora i wychodzi razem z jego odpowiedzią.
const NAME_ONLY: &str = "asked about the name and nothing else";

/// `<biblioteka>/skills/<name>` — albo odmowa, **zanim** cokolwiek zostanie skasowane.
///
/// 2026-08-19 — POWSTAŁO, BO OBIE DROGI WEJŚCIA LICZĄ TĘ SAMĄ ŚCIEŻKĘ I ŻADNA JEJ NIE
/// SPRAWDZAŁA. [`review_skill_inner`] bierze `name` z front-mattera pobranego pliku, składa
/// z niego ścieżkę i robi na niej `remove_dir_all` (przez [`gone`]). `SKILL.md` bez pola
/// `name:` daje `Skill::default()`, czyli `name: ""`, czyli ścieżkę `<biblioteka>/skills/` —
/// i kasowane są **wszystkie kopie kanoniczne razem z `installed.json`**. `name: ../../x`
/// wychodzi poza bibliotekę. Dziś trafia to tylko wklejony link, więc zdarza się rzadko;
/// formularz zamienia tę nazwę w rzecz, którą człowiek wpisuje palcami.
///
/// DLACZEGO ODMOWA JEST TU, A NIE W DWÓCH MIEJSCACH: policzenie ścieżki i decyzja „czy wolno"
/// to jedno pytanie. Rozdzielone, przechodzą przez drugą drogę wejścia bez sprawdzenia —
/// i tak właśnie wygląda dzisiejszy defekt.
///
/// DLACZEGO ZDANIE JEST WALIDATORA: `place::validate_strict` odpowiada już na pytanie „czy to
/// jest nazwa umiejętności", jednym komunikatem na przyczynę i dosłownie tym, który powie
/// vendor (niezmiennik 23). Nazwa, która przechodzi `is_slug`, składa się wyłącznie z `a-z`,
/// `0-9` i łącznika — więc „jeden człon ścieżki" wychodzi z tej samej reguły, a nie z drugiej,
/// napisanej obok.
pub fn canonical_for(library: &Path, name: &str) -> Result<PathBuf, Error> {
    // Dokument, w którym jedyną rzeczą wartą zakwestionowania jest nazwa. Nazwa katalogu i pole
    // `name` to ten sam napis, więc reguła „katalog musi zgadzać się z nazwą" nie ma tu o czym
    // mówić — a to jest jedyna reguła walidatora, która pyta o dwie rzeczy naraz.
    let doc = SkillDoc {
        fields: vec![
            ("name".to_owned(), name.to_owned()),
            ("description".to_owned(), NAME_ONLY.to_owned()),
        ],
        body: String::new(),
    };
    crate::skills::place::validate_strict(name, &doc)
        .map_err(|messages| Error::Invalid { messages })?;

    Ok(library.join(SKILLS_DIR).join(name))
}

/// Zapisuje, skąd umiejętność się wzięła — poza jej katalogiem i poza sidecarem instalacji.
///
/// 2026-08-19 — DO TEGO DNIA POCHODZENIE BYŁO **WYWNIOSKOWANE**, a nie zapisane:
/// [`list_skills_in`] odpowiadał „z internetu" wtedy, gdy obok zainstalowanego katalogu
/// leżała kopia kanoniczna, bo kopie kanoniczne powstawały wyłącznie w [`review_skill_inner`].
/// Ta przesłanka przestaje być prawdziwa w chwili, w której [`author_skill_inner`] też odkłada
/// kopię kanoniczną — więc znacznik musi mieć własny zapis (niezmiennik 4: pole, którego nie da
/// się odtworzyć z plików, jest polem, którego nie wolno zapisać; tu jest odwrotnie — pole,
/// którego nie da się już wywnioskować, trzeba zapisać jawnie).
///
/// DWA MIEJSCA, W KTÓRYCH TEN ZAPIS STAĆ NIE MOŻE, i oba są zmierzone, nie teoretyczne:
///
/// - **`skills/installed.json`.** `place::write_sidecar` odtwarza cały plik z samego zbioru
///   ścieżek (`place.rs:673-689`), więc cokolwiek dopisanego obok przepada przy następnej
///   instalacji albo usunięciu — po cichu i bez śladu.
/// - **wnętrze katalogu umiejętności.** `ingest::bundled_files` zabiera **każdego** sąsiada
///   `SKILL.md`, więc znacznik położony obok niego pojechałby do katalogów vendorów jako plik
///   dołączony umiejętności — do żywej konfiguracji narzędzi człowieka.
///
/// DLACZEGO `std::io::Result`, A NIE [`Error`]: zapisać ten wiersz nie da się wyłącznie wtedy,
/// gdy nie da się napisać pliku, a wołają tę funkcję obie drogi wejścia — jedna niosąca
/// [`Error`], druga [`FetchError`]. Oba te enumy umieją przyjąć `std::io::Error` przez `From`,
/// więc `?` w każdej z nich zachowuje przyczynę. Własny wariant znaczyłby przepisanie zdania
/// przez `to_string()` w jednej z dwóch.
pub fn remember_origin(library: &Path, name: &str, from_the_internet: bool) -> std::io::Result<()> {
    let path = origins_path(library);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Cały plik czytany i pisany na nowo. Wiersz per umiejętność w pliku dopisywanym końcem
    // znaczyłby dwie odpowiedzi na jedno pytanie po drugim imporcie tej samej nazwy — a wtedy
    // liczy się ta, którą czytelnik akurat znalazł pierwsza.
    let mut record = Origins {
        from_the_internet: origins_of(library),
    };
    record
        .from_the_internet
        .insert(name.to_owned(), from_the_internet);

    let text = serde_json::to_string_pretty(&record).map_err(std::io::Error::other)?;
    std::fs::write(&path, text + "\n")?;
    Ok(())
}

/// Skąd wzięła się każda umiejętność, o której cokolwiek zapisaliśmy.
///
/// `#[serde(default)]` plus domyślne ignorowanie nieznanych pól: plik zapisany przez nowszą
/// wersję Loadouta ma się wczytać, a nie wywrócić bieg (niezmiennik 5).
#[derive(Debug, Default, Serialize, Deserialize)]
struct Origins {
    /// Nazwa umiejętności → czy jej treść przyszła z sieci. `BTreeMap`, żeby `git diff` na tym
    /// pliku pokazywał zmianę, a nie przetasowanie kluczy.
    #[serde(default)]
    from_the_internet: BTreeMap<String, bool>,
}

fn origins_path(library: &Path) -> PathBuf {
    library.join(SKILLS_DIR).join(ORIGINS_FILE)
}

/// Co plik pochodzenia mówi o umiejętnościach w tej bibliotece.
///
/// Brak pliku, plik nieczytelny i plik o nieznanym kształcie dają ten sam wynik: pustą mapę.
/// Nieobecność zapisu jest odpowiedzią, którą [`list_skills_in`] umie obsłużyć ostrożnie,
/// a błąd w tym miejscu zaczerwieniłby całą sekcję za brak pliku, którego wolno nie mieć —
/// każda biblioteka starsza niż to zadanie go nie ma.
fn origins_of(library: &Path) -> BTreeMap<String, bool> {
    std::fs::read_to_string(origins_path(library))
        .ok()
        .and_then(|text| serde_json::from_str::<Origins>(&text).ok())
        .map(|record| record.from_the_internet)
        .unwrap_or_default()
}

/// Trzy pola z formularza → ta sama droga, którą idzie wklejony link.
///
/// **Potok jest kolejnością, nie zbiorem funkcji** (nagłówek `skills::ingest`): złóż plik przez
/// `place::emit`, zapisz go, przeczytaj przez `ingest::from_folder`. Tylko wtedy skan widzi
/// **tekst pliku**, a nie strukturę, którą sobie zbudowaliśmy — a R1 (znaki niewidzialne,
/// komentarze HTML) i R5 (`allowed-tools`, `hooks` we front-matterze) czytają wyłącznie tekst.
/// Formularz budujący `Skill` wprost z trzech pól omija `ingest::review` w całości: wszystko
/// działa, znaleziska nie powstają, a plik z ukrytym akapitem instaluje się jako czysty
/// (niezmiennik 23).
///
/// ZAKRESU TU NIE MA I NIE MA GO POTRZEBOWAĆ, i to jest twierdzenie o tej funkcji, nie brak.
/// Ta droga odkłada wyłącznie **kopię kanoniczną** w bibliotece (`~/.loadout/skills/<name>/`),
/// a kopia kanoniczna jest globalna z definicji: jest źródłem, z którego katalogi vendorów są
/// wyjściem builda (niezmiennik 4). Wybór „ten projekt / wszędzie" zapada dopiero przy
/// rozmieszczaniu, jedno wywołanie później, i jedzie do [`install_skill_into`] razem z korzeniem
/// projektu — tak samo dla umiejętności wpisanej tutaj, jak dla wklejonej linkiem.
///
/// 2026-08-19 — stało tu „zakres zostaje globalny […] wybór jest osobnym zadaniem (T-44)". To
/// zdanie przestało być prawdziwe w chwili, w której wybór powstał: czytelnik brał z niego, że
/// umiejętność napisana w formularzu ląduje wyłącznie u człowieka, a ona jedzie tam, gdzie wskazał.
pub fn author_skill_inner(library: &Path, authored: Authored) -> Result<ImportWire, Error> {
    let name = slug_of(&authored.name);
    // Odmowa PIERWSZA, przed policzeniem czegokolwiek, co dotyka dysku — powód stoi
    // przy `canonical_for`.
    let canonical = canonical_for(library, &name)?;

    // Tutaj trzy odpowiedzi przestają być tekstem z okna i stają się umiejętnością. To jedyne
    // miejsce, w którym „kiedy tego użyć" zamienia się w `description`.
    let skill = Skill {
        name,
        description: authored.when_to_use,
        body: authored.what_to_do,
        ..Skill::default()
    };

    // ZŁÓŻ PLIK. `place::emit` jest tu dwiema rzeczami naraz i obie są potrzebne. Pierwsza: to
    // on nadaje plikowi front-matter, więc `hooks:` wpisane przez człowieka w polu „co zrobić"
    // przestaje być front-matterem i staje się zwykłym wierszem ciała — pole, które WYKONUJE
    // kod, nie ma jak dojechać do vendora. Druga: pola zdjęte wychodzą z niego listą, a nie po
    // cichu (`_stripped` nikt jeszcze nie czyta i to jest znalezisko zgłoszone w TASK.md, nie
    // przeoczenie tutaj).
    let (composed, _stripped) = crate::skills::place::emit(&skill);
    let text = settled(&composed);

    // ZAPISZ TO, CO PRZESKANOWANE. Kopia kanoniczna powstaje jednym plikiem, po zdjęciu tego,
    // co zdejmuje normalizacja — więc bajty na dysku i bajty pokazane człowiekowi są jednym
    // napisem, a nie dwoma, które ktoś kiedyś porówna.
    gone(&canonical)?;
    std::fs::create_dir_all(&canonical)?;
    std::fs::write(canonical.join(SKILL_FILE), &text)?;

    // PRZECZYTAJ Z DYSKU, tą samą funkcją, którą czyta pobranie. Znaleziska i werdykt, które
    // zobaczy człowiek, mówią wtedy o bajtach, które naprawdę tam leżą — a nie o napisie, który
    // mieliśmy w zmiennej (niezmiennik 20).
    let import = ingest::from_folder(&canonical).map_err(|error| Error::Invalid {
        // `Error` nie ma wariantu na „nie dało się odczytać tego, co właśnie zapisaliśmy" i nie
        // dostanie go tutaj: `skills/mod.rs` nie należy do tego zadania (AGENTS.md §7). Zdanie
        // rdzenia niesie przyczynę — najczęściej sufit rozmiaru pliku.
        messages: vec![format!(
            "Loadout saved this skill and could not read it back: {error}"
        )],
    })?;

    // Zapis pochodzenia PO zapisie treści: wiersz mówiący „napisana tutaj" o umiejętności,
    // której nie ma na dysku, byłby zapisem o czymś, co się nie stało.
    remember_origin(library, &import.skill.name, false)?;

    Ok(ImportWire {
        // JEDYNE POLE, KTÓRE TA DROGA WIE LEPIEJ od `From<&Import>`: treść wpisał człowiek
        // w tym oknie, więc znacznik nie ma się palić. Wszystkie pozostałe pochodzą z pliku.
        from_the_internet: false,
        ..ImportWire::from(&import)
    })
}

/// Tekst, na którym normalizacja rdzenia nie ma już nic do zdjęcia.
///
/// 2026-08-19 — PĘTLA, NIE JEDNO WYWOŁANIE, i to jest własność `ingest::review`, nie ostrożność
/// na zapas. Zdjęcie komentarza HTML **skleja** tekst przed nim z tekstem po nim, więc potrafi
/// odsłonić otwarcie następnego, którego w wejściu nie było: `<!<!---->--` wychodzi z pierwszego
/// przejścia jako `<!--`. Kopia kanoniczna ma trzymać dokładnie te bajty, które przeskanowaliśmy
/// i pokazaliśmy człowiekowi — plik i `ImportWire::reviewed.body` są jedną rzeczą — a to jest
/// prawdą tylko dla tekstu, na którym drugie przejście już nic nie zmienia.
///
/// Kończy się zawsze: przejście, które cokolwiek zmieniło, zwróciło tekst KRÓTSZY, bo
/// normalizacja wyłącznie zdejmuje znaki i nigdy żadnego nie dokłada.
fn settled(composed: &str) -> String {
    let mut text = ingest::review(composed).body;
    loop {
        let again = ingest::review(&text).body;
        if again == text {
            return text;
        }
        text = again;
    }
}

/// Zapisuje przejrzaną umiejętność w obu katalogach vendorów.
///
/// Bierze **nazwę**, nie treść: instalowane jest to, co leży w kopii kanonicznej, czyli
/// dokładnie te bajty, które zostały przeskanowane i pokazane człowiekowi. Umiejętność złożona
/// z tego, co odesłało okno, byłaby drugim brzmieniem tej samej treści — i to tym, które
/// przeszło przez warstwę renderującą.
///
/// Zgoda na znaleziska blokujące jest warunkiem WYWOŁANIA i mieszka w magazynie sekcji
/// (`src/state/skills.ts`, T-19). Drugie sprawdzenie tutaj byłoby drugim miejscem, w którym
/// mieszka odpowiedź na pytanie „czy człowiek to przeczytał" (niezmiennik 13).
///
/// # Zakres przyjeżdża z okna, korzeń projektu też
///
/// Wybór „ten projekt / wszędzie" jest odpowiedzią człowieka, a nie stałą tej warstwy, i jedzie
/// razem z korzeniem projektu — tym samym, którego używa bieg (`AppState::project_for`). Dwie
/// odpowiedzi na „który to projekt" rozjadą się pierwszego dnia, w którym ktoś przełączy kartę
/// (niezmiennik 13). `project: None` z zakresem projektowym jest ODMOWĄ z rdzenia, nigdy
/// katalogiem roboczym procesu.
pub fn install_skill_into(
    library: &Path,
    name: &str,
    landing: Landing,
    project: Option<&Path>,
) -> Result<Vec<PathBuf>, Error> {
    let canonical = library.join(SKILLS_DIR).join(name);
    let import = ingest::from_folder(&canonical).map_err(|error| Error::Invalid {
        // Nazwa przychodzi z okna, więc katalog może nie istnieć — a `place::Error` nie ma
        // wariantu na „nie ma czego instalować" i nie dostanie go tutaj: `skills/mod.rs` nie
        // należy do tego zadania (AGENTS.md §7).
        messages: vec![format!(
            "There is nothing to install under the name '{name}': {error}"
        )],
    })?;

    // Walidacja, plan i odmowa przed pierwszym zapisem — wszystko w `place::plan`. Zakres
    // projektowy bez korzenia odbija się TAM, zdaniem `Error::NoProjectRoot`, i to jest jedyny
    // powód, dla którego można tu podać `project` niesprawdzony pod kątem „czy jest": ani
    // `destinations`, ani `apply` nie zobaczą pustki, bo plan odmawia przed nimi.
    let roots = roots_for(library, project);
    let plan = crate::skills::place::plan(&import.skill, landing.into(), &roots)?;
    crate::skills::place::apply(&plan, &import.skill)?;
    Ok(plan.writes)
}

/// Zdejmuje umiejętność z katalogów agentów.
///
/// 2026-08-18 — POWSTAŁO, BO SEKCJA MIAŁA PRZYCISK BEZ SKUTKU. Lista z
/// [`list_skills_in`] czyta katalogi vendorów, więc wiersz na ekranie odpowiada plikom na
/// dysku — a jedyne, co człowiek mógł z nim zrobić, to zainstalować go jeszcze raz. Kontrolka,
/// której handler nie ma skutku, jest gorsza niż jej brak (niezmiennik 16).
///
/// **Ani jednej linii kasowania tutaj** (niezmiennik 23). Cała polityka mieszka
/// w [`crate::skills::place::remove`]: dwa przebiegi (najpierw decyzja o obu katalogach, potem
/// pierwsze skasowanie), sidecar jako jedyne źródło odpowiedzi „czy to nasze", i kopia
/// kanoniczna, która **zostaje**, bo katalogi vendorów są wyjściem builda (niezmiennik 4).
///
/// # Trzy odpowiedzi, trzy różne zdania
///
/// - **Zdjęte.** `Ok(())` — i nic więcej, bo okno pyta tylko o to, czy się udało.
/// - **Nie nasze.** Katalog o tej nazwie napisał ktoś inny, więc nie kasujemy **niczego**
///   (także drugiej kopii). Zdanie rdzenia jedzie na ekran słowo w słowo: własne tłumaczenie
///   byłoby drugim miejscem, w którym mieszka ten sam komunikat, a jedno z dwóch zawsze jest
///   nieaktualne.
/// - **Nic tam nie było.** Osobne zdanie, nie ciche `Ok(())`. Przycisk, który melduje sukces,
///   choć nic nie zaszło, jest tym samym defektem, który to zadanie naprawia — a jedyny stan,
///   w którym to się zdarza, to lista starsza niż dysk, i o tym właśnie ma być to zdanie.
///
/// # Ta sama nazwa w dwóch zakresach to dwie rzeczy
///
/// Dlatego zakres jedzie argumentem: zdjęcie umiejętności „z tego projektu" ma zostawić kopię
/// globalną nietkniętą, i odwrotnie. Zabranie obu naraz jest inną czynnością, o którą nikt nie
/// prosił — a kopia zabrana „przy okazji" znika z katalogów, do których zagląda Claude Code
/// tego człowieka, bez ani jednego zdania na ekranie.
pub fn delete_skill_from(
    library: &Path,
    name: &str,
    landing: Landing,
    project: Option<&Path>,
) -> Result<(), Error> {
    // Nazwa przychodzi z okna, więc jest wejściem, któremu nie ufamy (T3 §5.2). Sidecar
    // obroniłby nas i tak — `<katalog>/../..` nie stoi na liście „to napisał Loadout", więc
    // `remove` odmówiłby — ale odmowa po ludzku ma padać PRZED dotknięciem dysku, i ma mówić
    // o nazwie, a nie o cudzym katalogu.
    if Path::new(name).file_name().is_none_or(|only| only != name) {
        return Err(Error::Invalid {
            messages: vec![format!(
                "\"{name}\" is not the name of a skill Loadout installed. Pick one from the list."
            )],
        });
    }

    // Zakres jedzie do `place::remove` tą samą drogą, którą jedzie do `place::plan`: ta sama
    // nazwa w dwóch korzeniach to DWIE rzeczy, a kopia zabrana „przy okazji" znika z katalogów,
    // do których zagląda Claude Code tego człowieka, bez ani jednego zdania na ekranie.
    let roots = roots_for(library, project);
    match crate::skills::place::remove(name, landing.into(), &roots)? {
        crate::skills::place::Removed::Done { paths } if paths.is_empty() => Err(Error::Invalid {
            messages: vec![format!(
                "There is nothing installed under the name '{name}' any more, so nothing was \
                     removed. The list you are looking at is older than the folder."
            )],
        }),
        crate::skills::place::Removed::Done { .. } => Ok(()),
        // Zdanie rdzenia, nazwane katalogiem, którego to dotyczy. Kolizja nazw jest normalna:
        // `pdf` to oczywista nazwa i ktoś mógł napisać swoją ręcznie.
        crate::skills::place::Removed::Skipped { path, why } => Err(Error::Invalid {
            messages: vec![format!("{why} ({}). Nothing was removed.", path.display())],
        }),
    }
}

// ── Draft: jedna tura POZA grafem ──────────────────────────────────────────────────────────
//
// Umiejętność, której człowiek chce, nie jest biegiem: to jedna tura, jeden prompt, jedna
// odpowiedź. Droga prowadzi więc przez [`crate::engine::drivers::AgentDriver::start`], a nie
// przez planistę — złożenie jednokrokowego workflow po to, żeby zawołać planistę, **jest**
// etapem biegu zaszytym w Ruście, czyli dokładnie tym, czego zabrania niezmiennik 27
// i decyzja D7. Warstwa sterowników nie zna słowa „krok" i to jest cały powód, dla którego
// ta droga jest od tego wolna z definicji.

/// Czego chcemy od modelu, słowo w słowo — prompt jako **dane** w tej warstwie (precedens:
/// `HANDOFF_INDEX_OPENS` i `with_the_task` w `commands::run`).
///
/// KRÓTKI Z ROZMYSŁU (niezmiennik 28). Poprawność draftu nie stoi na dokładności tej instrukcji:
/// tekst i tak przechodzi przez `ingest::from_folder` tutaj i przez `place::validate_strict` po
/// drodze z T-42, a człowiek czyta trzy pola przed zapisem. Akapit dopisywany po każdym słabym
/// drafcie jest dokładnie tym, co niezmiennik 28 nazywa promptem rosnącym monotonicznie —
/// i kosztuje tokeny w każdym pytaniu, na zawsze.
///
/// Po angielsku, jak wszystko, co czyta model i człowiek (decyzja D5). Kończy się dwukropkiem
/// i nową linią, bo zaraz za nim staje zdanie człowieka.
const ASK_FOR_A_SKILL: &str = "\
Write one skill as a single SKILL.md file and say nothing else.
Open it with a front matter block between --- lines carrying two keys: name, which is lowercase \
words joined by hyphens, and description, which says WHEN somebody should reach for this skill.
After the front matter, write what to do, as Markdown, in the second person.

This is what the person asked for:
";

/// Odmowa dla drugiego pytania zadanego, kiedy pierwsze jeszcze pisze.
///
/// Zdanie, nie cisza: cisza po kontrolce wygląda dokładnie jak kontrolka zepsuta, a tutaj drugie
/// naciśnięcie kosztowałoby drugą turę u dostawcy.
const ALREADY_WRITING: &str =
    "An agent is already writing a skill. Wait for that one to finish, or stop it first.";

/// Zdanie o grupie, która nadal odpowiada na sygnał zerowy (niezmiennik 6).
///
/// Ta sama treść, którą mówi krok biegu w tym samym stanie (`commands::run`, `Ended::Stopped`):
/// dwa różne zdania o jednym fakcie to dwa różne zgłoszenia błędu od tej samej osoby.
const MAY_STILL_BE_RUNNING: &str =
    "Loadout could not make sure this agent stopped, so it may still be running.";

/// Odmowa dla biblioteki, w której nikogo jeszcze nie zapisano.
const NOBODY_SAVED: &str =
    "There is no agent saved on this machine, so there is nobody to ask. Save one first.";

/// Ile zdarzeń mieści się w kanale draftu, zanim sterownik na nim stanie.
///
/// Ta sama liczba, co `EVENT_QUEUE` w `commands::run`, i celowo nie pożyczona: tamta jest
/// prywatna, a pojemność buforu draftu nie jest tym samym faktem co pojemność buforu biegu —
/// tutaj nikt tych linii nie czyta, więc liczba decyduje wyłącznie o tym, jak często drenaż
/// budzi się przy zalewie.
const EVENT_QUEUE: usize = 256;

/// Podkatalogi katalogu roboczego draftu: gdzie pracowała tura i gdzie stanął jej tekst.
const ASKED_IN: &str = "asked";
const ANSWERED_IN: &str = "answered";

/// Czym skończyło się jedno pytanie zadane agentowi.
///
/// **Anulowanie jest wariantem wartości, nigdy błędem** (niezmiennik 7): `Err(Cancelled)`
/// zmusza każdego wołającego do rozróżniania „to się nie udało" od „to zatrzymał człowiek",
/// a rozróżnienie zgubione raz jest zgubione wszędzie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftOutcome {
    /// Model oddał umiejętność: trzy pola, gotowe do formularza z T-42. Zapisu tu nie ma
    /// i mieć nie będzie — droga zapisu jest jedna ([`author_skill_inner`]) i to ona składa
    /// plik, skanuje go i odkłada kopię kanoniczną (niezmiennik 23).
    Wrote(Authored),
    /// Człowiek zatrzymał pisanie.
    Cancelled,
}

/// Miejsce na JEDEN draft naraz i uchwyt do tego, który pisze teraz.
///
/// # Dlaczego jeden naraz, a nie „ile naraz" z suwaka
///
/// Bo tej liczby nie ma z czego wziąć. Limit równoległości jest dziś **per bieg**, nie globalny:
/// `run_workflow_inner` zakłada sobie własny `Limiter`, a `run_workflow_with_slots` — jedyna
/// funkcja przyjmująca wspólną pulę — nie ma w produkcji ani jednego wołającego. Draft
/// udający, że bierze slot ze wspólnej puli, byłby czwartym miejscem, w którym ta liczba nie
/// znaczy tego, co mówi. Granica jest więc własna i jawna: jeden, a drugie pytanie jest
/// odmową ze zdaniem.
#[derive(Debug, Default)]
pub struct Drafting {
    /// `Some` znaczy „ktoś właśnie pisze" i niesie token **tego** draftu.
    ///
    /// `std::sync::Mutex` i **nigdy trzymany przez `await`** (niezmiennik 8): każde wzięcie tego
    /// zamka mieści się w jednym wyrażeniu, które kopiuje token albo go odkłada i oddaje zamek.
    /// Zamek trzymany przez turę zawiesiłby Stop na czas pisania przez model — czyli dokładnie
    /// wtedy, kiedy Stop jest do czegokolwiek potrzebny.
    ///
    /// Token jest **własny**, a nie wzięty z `deps().control`, i to nie jest ostrożność na zapas:
    /// `AppState.live` jest PODMIENIANY przy każdym Starcie (`AppState::begin_run`), więc draft
    /// trzymający się tamtego uchwytu traci swój token w chwili, w której człowiek uruchomi bieg
    /// w innej karcie — i Stop na drafcie przestaje cokolwiek robić.
    writing: Mutex<Option<CancellationToken>>,
}

impl Drafting {
    /// Miejsce, na którym nikt nie pisze.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// „Stop" z okna: zatrzymuje draft, który pisze teraz. Bez draftu nie robi nic.
    ///
    /// Dowód zejścia grupy **nie wraca tędy** i to jest wybór: [`GroupProof`] czyta tura
    /// (`handle.cancel().await`), a niesie go odpowiedź [`draft_skill_inner`] — czyli to samo
    /// wywołanie, na które okno już czeka. Druga droga na ten sam fakt byłaby drugim miejscem,
    /// w którym mieszka odpowiedź „czy agent naprawdę zszedł" (niezmiennik 13).
    ///
    /// [`GroupProof`]: crate::engine::supervisor::GroupProof
    pub fn stop(&self) {
        // Zamek wzięty i oddany w JEDNYM wyrażeniu, przed czymkolwiek, co czeka (niezmiennik 8).
        // Zatruty zamek odplatamy zamiast panikować: `panic!` w agentowym runtime zabiera cały
        // bieg (AGENTS.md §4), a uchwyt po panice jednej tury jest dalej poprawnym uchwytem.
        let token = self
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(token) = token {
            token.cancel();
        }
    }

    /// Zajmuje jedyne miejsce na draft. `None` znaczy „ktoś już pisze".
    ///
    /// Odmowa mieszka TUTAJ, a nie w widoku, i to jest ta sama decyzja, którą ma magazyn sekcji
    /// (`src/state/skills.ts`, nagłówek): schowana kontrolka jest sugestią, bo zostaje klawiatura,
    /// skrót i wywołanie komendy wprost. Warunkiem jest więc samo WYWOŁANIE.
    fn claim(&self) -> Option<Claim<'_>> {
        // Sprawdzenie i zajęcie w JEDNYM wzięciu zamka. Dwa osobne („czy wolne", potem „zajmij")
        // zostawiają okno, w którym dwa pytania zadane w tej samej chwili widzą oba wolne
        // miejsce — a wtedy „jeden naraz" jest zdaniem, nie własnością. Zamek ginie razem z tym
        // wyrażeniem, przed pierwszym `await` (niezmiennik 8).
        let mut writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        if writing.is_some() {
            return None;
        }
        let stop = CancellationToken::new();
        *writing = Some(stop.clone());
        Some(Claim {
            drafting: self,
            stop,
        })
    }

    /// Oddaje miejsce. Wołane wyłącznie przez [`Claim`], czyli na każdej drodze wyjścia z tury.
    fn release(&self) {
        *self.writing.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

/// Zajęte miejsce na jeden draft — oddawane samo, na KAŻDEJ drodze wyjścia.
///
/// Struktura z [`Drop`], a nie para wywołań „zajmij" / „oddaj": dróg wyjścia z jednej tury jest
/// siedem (odmowa biblioteki, nieudany start, Stop, limit czasu, tekst, który nie jest
/// umiejętnością, sukces i panika w środku), a miejsce oddane w sześciu z siedmiu jest miejscem,
/// którego już nikt nigdy nie dostanie — od tej chwili KAŻDE pytanie jest odmową „ktoś już pisze",
/// aż do restartu aplikacji.
struct Claim<'a> {
    drafting: &'a Drafting,
    /// Token TEGO draftu — ten sam, który cofa [`Drafting::stop`].
    stop: CancellationToken,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.drafting.release();
    }
}

/// Jedno zdanie człowieka → trzy pola napisane przez wybranego agenta, jedną turą poza grafem.
///
/// `library` to `~/.loadout` i przychodzi **argumentem**, nigdy z `HOME` czytanego w środku —
/// ten sam powód, co przy [`author_skill_inner`]: katalog domowy odczytany tutaj znaczyłby, że
/// każdy test pyta prawdziwą bibliotekę.
///
/// `agent` jest **identyfikatorem** zapisanego agenta, nie nazwą pliku i nie nazwą vendora:
/// model, prompt systemowy i dial bezpieczeństwa biorą się z jego definicji przez
/// `library::agents::resolve`, a nie z niczego wpisanego tutaj.
pub async fn draft_skill_inner(
    library: &Path,
    drivers: &Drivers,
    drafting: &Drafting,
    want: &str,
    agent: &str,
) -> Result<DraftOutcome, Error> {
    // JEDEN NARAZ, i odmowa PRZED czymkolwiek, co dotyka dysku albo sterownika: drugie pytanie
    // ma zostawić pierwsze nietknięte, a jedynym sposobem, żeby to była prawda, jest nie zaczynać.
    // Odmowa, która ubija to, co już pisze, jest gorsza od kolejki — człowiek traci odpowiedź,
    // na którą czekał, i nigdy się nie dowiaduje dlaczego.
    let Some(claim) = drafting.claim() else {
        return Err(refusal(ALREADY_WRITING.to_owned()));
    };

    // Kto ma to napisać. Model, prompt systemowy, dial bezpieczeństwa i limit czasu biorą się
    // z JEGO zapisanej definicji, złożonej tym samym `resolve`, którym składa je krok biegu.
    let saved = the_agent_saved_as(library, agent)?;
    let effective = resolve(&saved, &Overrides::default())
        .map_err(|error| refusal(error.to_string()))?
        .agent;

    // Ten sam identyfikator nosi tura i jej katalog roboczy: gdyby katalog kiedyś przeżył
    // awarię aplikacji, widać z jego nazwy, czyj był.
    let run = Uuid::now_v7();
    let scratch = Scratch::new(run)?;
    let spec = RunSpec {
        run_id: run,
        cwd: scratch.asked(),
        // Instrukcja i zdanie człowieka jadą jako DANE, wyłącznie stdinem (niezmiennik 9): ta
        // warstwa nie skleja komendy i nie zna ani jednej flagi vendora.
        prompt: format!("{ASK_FOR_A_SKILL}{want}"),
        model: some_text(&effective.model),
        // Prompt systemowy agenta, nie zdanie człowieka. Zdanie człowieka w tym polu byłoby
        // niezmiennikiem 9 złamanym po cichu, bo stąd wchodzi do argv, a argv widzi `ps`.
        system_append: some_text(&effective.instructions),
        // DIAL WOLNO TYLKO OBNIŻYĆ (D6: „przelotka nie omija diala bezpieczeństwa"). Odpowiedź
        // wraca strumieniem, więc do pisania po dysku nie ma powodu — a dial skopiowany
        // z definicji wygląda poprawnie do chwili, w której ktoś prosi o umiejętność swojego
        // najmocniejszego agenta.
        policy: Policy::ReadOnly,
        /* Sieci NIE MA i to jest decyzja: ta rozmowa prosi o umiejętność, a odpowiedź wraca
         * strumieniem. Agent, który przy okazji poszedłby do internetu, robiłby coś, o co nikt
         * w tym oknie nie prosił. */
        reaches_the_web: false,
        tools: None,
        // Nic poza katalogiem roboczym: umiejętność pisze się z jednego zdania, więc nie ma tu
        // czego czytać. Odnośnik do pliku, którego agentowi nie wolno otworzyć, jest odnośnikiem
        // bez handlera (niezmiennik 16).
        extra_dirs: Vec::new(),
        resume: None,
    };

    // Odbiór staje PRZED startem sterownika: vendor ma prawo powiedzieć pierwsze zdarzenia
    // jeszcze w `start`, a kanał bez odbiorcy zatrzymałby go na pierwszym pełnym buforze.
    let (events, inbox) = mpsc::channel::<DecodedEvent>(EVENT_QUEUE);
    let drain = tokio::spawn(off_the_wire(inbox));

    let drafted = match (drivers)(effective.runs_with).start(spec, events).await {
        // Vendor bez adaptera odmawia dokładnie tutaj i jego zdanie jest CAŁĄ odpowiedzią: panika
        // zabrałaby okno, a cisza zostawiłaby człowieka przy kontrolce, która nic nie robi.
        Err(error) => Err(refusal(error.to_string())),
        Ok(mut handle) => {
            let limit = give_up_after(effective.give_up_after_minutes);
            let ended = one_turn(&mut *handle, &claim.stop, limit).await;
            what_came_of_it(&mut *handle, ended, limit, &scratch).await
        }
    };

    // Uchwyt zszedł razem z gałęzią wyżej, więc kanał jest zamknięty i drenaż kończy się sam.
    // Czekamy na niego, zamiast go porzucić: zadanie przeżywające to wywołanie trzymałoby
    // odbiornik otwarty i przy następnym drafcie nie dałoby się powiedzieć, czyje linie są czyje.
    let _ = drain.await;
    drafted
}

/// Jedna tura draftu: skończyła się sama, zatrzymał ją człowiek, albo przekroczyła swój limit.
enum Ended {
    /// Tura wróciła sama — z wynikiem albo z błędem sterownika.
    Turn(anyhow::Result<TurnOutcome>),
    /// Człowiek nacisnął Stop.
    Stopped,
    /// Draft przekroczył limit czasu swojego agenta.
    Overdue,
}

/// Czeka na koniec tury, na Stop albo na limit czasu — i **nie zdejmuje zadania Rusta**.
///
/// `tokio::time::timeout(limit, handle.wait())` robi z zewnątrz to samo, jest krótsze o trzy znaki
/// i jest błędem, przed którym stoi niezmiennik 10: anuluje ZADANIE RUSTA, a proces vendora
/// zostaje żywy i pali limit u dostawcy do końca świata. Dlatego limit czasu jest tutaj zwykłą
/// gałęzią wyboru, a zejście po grupie robi dopiero [`what_came_of_it`], przez sterownik.
async fn one_turn(
    handle: &mut dyn AgentHandle,
    stop: &CancellationToken,
    limit: Duration,
) -> Ended {
    let waiting = handle.wait();
    tokio::pin!(waiting);
    let overdue = tokio::time::sleep(limit);
    tokio::pin!(overdue);
    tokio::select! {
        // `biased`, bo tura, która właśnie się skończyła, ma pierwszeństwo przed Stopem wpadającym
        // w tej samej chwili: ubijanie czegoś, co już zeszło, zamieniałoby gotowy draft
        // w anulowany zależnie od tego, który poll wypadł pierwszy. Z tego samego powodu limit
        // czasu stoi PO Stopie — człowiek, który nacisnął Stop w ostatniej sekundzie, ma
        // przeczytać „zatrzymane", a nie „przekroczony limit".
        biased;
        done = &mut waiting => Ended::Turn(done),
        () = stop.cancelled() => Ended::Stopped,
        () = &mut overdue => Ended::Overdue,
    }
}

/// Co z tury wynikło dla człowieka: trzy pola, anulowanie jako wartość, albo jedno zdanie.
async fn what_came_of_it(
    handle: &mut dyn AgentHandle,
    ended: Ended,
    limit: Duration,
    scratch: &Scratch,
) -> Result<DraftOutcome, Error> {
    match ended {
        // PRZEKROCZONY LIMIT IDZIE TĄ SAMĄ DROGĄ, CO STOP: przez sterownik, po dowód. Powód
        // nazywa LIMIT CZASU i liczbę, którą trzeba zmienić — inaczej człowiek szuka wady
        // w agencie, którego nikt nie zepsuł.
        Ended::Overdue => {
            let minutes = limit.as_secs() / 60;
            Err(refusal(match handle.cancel().await {
                GroupProof::Alive { .. } => format!(
                    "This draft ran longer than its {minutes} minute limit, and Loadout could \
                     not make sure the agent stopped, so it may still be running."
                ),
                GroupProof::Dead { .. } => format!(
                    "This draft ran longer than its {minutes} minute limit, so Loadout stopped \
                     it. Give that agent more minutes, or ask for something smaller."
                ),
            }))
        }
        // ANULOWANIE IDZIE PRZEZ STEROWNIK, nie przez zdjęcie zadania Rusta (niezmienniki 6 i 10).
        Ended::Stopped => match handle.cancel().await {
            // Dowód zejścia grupy jest, więc nie ma o czym mówić: anulowanie jest WARTOŚCIĄ,
            // nigdy błędem (niezmiennik 7).
            GroupProof::Dead { .. } => Ok(DraftOutcome::Cancelled),
            // Dopóki dowodu nie ma, traktujemy grupę jak żywą (niezmiennik 6). Cisza jest tu
            // najdroższa z możliwych: osierocony agent pisze dalej, a płaci za to człowiek.
            GroupProof::Alive { .. } => Err(refusal(MAY_STILL_BE_RUNNING.to_owned())),
        },
        Ended::Turn(Err(error)) => Err(refusal(error.to_string())),
        Ended::Turn(Ok(turn)) => {
            // Normalne zakończenie idzie przez `close`: `claude` z otwartym stdinem czeka
            // w nieskończoność, więc tura bez tego zostawia żywy proces [T1 §2, §4.6].
            let code = handle.close().await.ok().flatten();
            // Sukces to zero **i** `is_error == false` (niezmiennik 19). Agent, który wypisał
            // „nie dam rady" i wyszedł czysto, nie napisał umiejętności.
            if !turn.ok || !matches!(code, None | Some(0)) {
                return Err(refusal(nothing_came_back(&turn.reason)));
            }
            three_fields(scratch, &turn.text).map(DraftOutcome::Wrote)
        }
    }
}

/// Tekst modelu → trzy pola formularza, przeczytane rdzeniem, który czyta wklejony link.
///
/// **Tym samym rdzeniem, a nie własnym parserem front-mattera**, i to nie jest oszczędność kodu:
/// `ingest::from_folder` czyta TEKST pliku, więc R1 (znaki niewidzialne, komentarze HTML) i R5
/// (`hooks:`, `allowed-tools:` we front-matterze) mają na czym pracować. Odczyt, który tam nie
/// wchodzi, nie produkuje ani jednego znaleziska — a umiejętność z ukrytą instrukcją wygląda
/// wtedy na czystą aż do dysku.
fn three_fields(scratch: &Scratch, text: &str) -> Result<Authored, Error> {
    let skill = ingest::from_folder(&scratch.answer(text)?)
        .map_err(|error| {
            refusal(format!(
                "Loadout could not read what the agent wrote: {error}"
            ))
        })?
        .skill;

    // Odmowa nazywa, CZEGO NIE MA, po jednej rzeczy na przyczynę: jedno „to nie jest
    // umiejętność" na trzy różne braki nie mówi człowiekowi, co poprawić.
    let mut missing = Vec::new();
    if skill.name.trim().is_empty() {
        missing.push("no name in it");
    }
    if skill.description.trim().is_empty() {
        missing.push("nothing that says when to use it");
    }
    if skill.body.trim().is_empty() {
        missing.push("nothing in it to do");
    }
    if !missing.is_empty() {
        return Err(refusal(format!(
            "The agent came back with something that is not a skill: {}. Ask again, or write \
             the three answers yourself.",
            missing.join(", ")
        )));
    }

    // Pola przechodzą tak, jak je przeczytał rdzeń — bez jednego znaku poprawki tutaj. Drugie
    // przycinanie w tym miejscu byłoby drugim brzmieniem tej samej treści, a człowiek zobaczy
    // dokładnie to, co za chwilę pójdzie na dysk drogą z T-42.
    Ok(Authored {
        name: skill.name,
        when_to_use: skill.description,
        what_to_do: skill.body,
    })
}

/// Zapisany agent o tym identyfikatorze.
///
/// Identyfikatorem, nie nazwą pliku: nazwa pliku powstaje ze zmiennej nazwy agenta, a `id`
/// przeżywa zmianę nazwy (T4 §5.1). Lista przychodzi z `commands::agents::list_agents_inner`,
/// czyli tą samą drogą, którą sekcja Agenci wypisuje ją na ekran — drugi spacer po katalogu
/// byłby drugą odpowiedzią na pytanie „kogo mam zapisanych" (niezmiennik 13).
pub fn the_agent_saved_as(library: &Path, id: &str) -> Result<Agent, Error> {
    let saved =
        super::agents::list_agents_inner(library).map_err(|error| refusal(error.to_string()))?;
    // Biblioteka bez ani jednego agenta i biblioteka bez TEGO agenta to dwie różne rzeczy do
    // zrobienia: pierwszą naprawia zapisanie kogokolwiek, drugą wybranie kogoś innego.
    if saved.is_empty() {
        return Err(refusal(NOBODY_SAVED.to_owned()));
    }
    saved
        .into_iter()
        .find(|one| one.id.to_string() == id)
        .ok_or_else(|| {
            refusal(format!(
                "No agent saved here has the id {id}. Pick one from the list, or save one first."
            ))
        })
}

/// Zdejmuje z drutu wszystko, co powie sterownik, i porzuca to.
///
/// Draft nie pokazuje ani jednej z tych linii — widok strumienia ma jednego właściciela (sekcja
/// Praca, niezmiennik 13) — ale MUSI je odebrać. `mpsc::Sender` staje na pełnym buforze, a tura,
/// która stoi na `send`, nie kończy się nigdy: dla bramki to jest „nic się nie uruchomiło"
/// (rc 124), nie czerwień. Zmierzone na agencie robiącym `find /usr/share`: 121 000 linii
/// na sekundę.
async fn off_the_wire(mut inbox: mpsc::Receiver<DecodedEvent>) {
    while inbox.recv().await.is_some() {}
}

/// Zdanie o turze, która skończyła się bez umiejętności.
fn nothing_came_back(reason: &FinishReason) -> String {
    match reason {
        FinishReason::Failed(said) => format!("The agent could not write this skill: {said}"),
        FinishReason::LimitReached => {
            "The agent stopped before it finished, because it ran into a limit of its own.\
             Try again, or ask for something smaller."
                .to_owned()
        }
        FinishReason::Cancelled | FinishReason::Completed => {
            "The agent stopped before it wrote anything. Ask again.".to_owned()
        }
    }
}

/// Limit czasu draftu — minuty z definicji agenta, tak samo jak przy kroku biegu.
///
/// Zero znaczy „poddaj się natychmiast", więc traktujemy je jak brak zdania i zostawiamy jedną
/// minutę: limit ubijający każdą turę w chwili startu jest gorszy niż brak limitu.
fn give_up_after(minutes: u32) -> Duration {
    Duration::from_secs(u64::from(minutes.max(1)) * 60)
}

/// Napis albo nic. Puste pole w definicji agenta znaczy „nie mam zdania", a nie „ustaw pustkę".
///
/// Bliźniak tej funkcji stoi w `commands::run` (`some_text`) i jest tam prywatny. Jedno wyrażenie
/// przepisane tutaj jest tańsze niż otwarcie tamtego pliku: `commands/run.rs` nie należy do tego
/// zadania (AGENTS.md §7), a różnica między tymi dwiema odpowiedziami byłaby widoczna od razu —
/// pusty napis podstawiony pod flagę modelu to vendor, który odmawia startu.
fn some_text(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.to_owned())
}

/// Zdanie dla człowieka jako odmowa tej warstwy.
///
/// `Error::Invalid` niesie listę komunikatów i renderuje je jedną linią — tego samego wariantu
/// używa [`author_skill_inner`], kiedy odmawia zdaniem, którego `skills::Error` nie ma w typie.
/// Nowego wariantu nie dokładamy: `skills/mod.rs` nie należy do tego zadania (AGENTS.md §7).
fn refusal(said: String) -> Error {
    Error::Invalid {
        messages: vec![said],
    }
}

/// Katalog roboczy jednej tury draftu — POZA biblioteką i sprzątany na każdej drodze wyjścia.
///
/// # Dlaczego nie w bibliotece
///
/// Bo katalog, który zostałby tam po awarii aplikacji, jest umiejętnością, której nikt nie
/// przejrzał, leżącą dokładnie tam, gdzie ta sekcja trzyma przejrzane. Draft nie zapisuje niczego
/// trwałego i nie ma prawa zapisać: trzy pola wracają do formularza z T-42 i dopiero tamten zapis
/// składa plik, skanuje go i odkłada kopię kanoniczną (niezmiennik 23).
///
/// # Dlaczego mimo to plik
///
/// Bo rdzeń czyta KATALOG (`ingest::from_folder`), i to jest niezmiennik 20 w tym jednym miejscu:
/// skan, który nie widział bajtów pliku, nie widział ataku. Niezmiennik 9 dotyczy zdania człowieka
/// i sekretów — te jadą stdinem i nie zatrzymują się nigdzie; tutaj ląduje ODPOWIEDŹ modelu, ta
/// sama, która za sekundę stanie w oknie, i ginie razem z tym wywołaniem.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(run: Uuid) -> std::io::Result<Self> {
        let root = std::env::temp_dir().join(format!("loadout-draft-{run}"));
        std::fs::create_dir_all(root.join(ASKED_IN))?;
        Ok(Self { root })
    }

    /// Katalog roboczy tury: pusty i tylko do czytania.
    fn asked(&self) -> PathBuf {
        self.root.join(ASKED_IN)
    }

    /// Tekst modelu na dysku i adres katalogu, w którym leży.
    ///
    /// OSOBNY katalog, nie ten, w którym pracowała tura: `ingest::from_folder` liczy też pliki
    /// dołączone, więc cokolwiek powstałoby obok, weszłoby do umiejętności jako jej plik.
    fn answer(&self, text: &str) -> std::io::Result<PathBuf> {
        let dir = self.root.join(ANSWERED_IN);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(SKILL_FILE), text)?;
        Ok(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Bez `?` i bez `expect`: to jest ostatnia droga wyjścia z tury, a katalog, którego nie
        // udało się usunąć z katalogu tymczasowego, nie jest niczym, o czym warto przewrócić
        // odpowiedź dla człowieka. Biblioteki to nie dotyczy w ogóle — nic tu w niej nie leży.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
