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
//! aplikacji (`<biblioteka>/skills/<name>/`), a oknu oddaje sam przegląd. [`install_skill_inner`]
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

use serde::{Deserialize, Serialize};

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

/// Korzenie instalacji dla zakresu globalnego, wyprowadzone z katalogu biblioteki.
///
/// `~/.loadout` leży **w** katalogu domowym, więc jego rodzic jest tym katalogiem. To nie jest
/// oszczędność argumentu: jedyne pytanie o `HOME` w całej aplikacji stoi w `lib.rs::loadout_dir`
/// i ma tam zostać jedno (niezmiennik 13). Drugi odczyt `HOME` tutaj znaczyłby też, że każdy
/// test pisze do prawdziwych katalogów vendorów.
///
/// `project: None`, bo okno nie przysyła zakresu — a `plan` odmawia zakresu projektowego bez
/// korzenia, zamiast zgadywać katalog roboczy.
#[must_use]
fn global_roots(library: &Path) -> Roots {
    Roots {
        home: library.parent().unwrap_or(library).to_path_buf(),
        project: None,
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
pub fn list_skills_inner(library: &Path) -> Result<Vec<InstalledWire>, Error> {
    let roots = global_roots(library);
    // Zbiór, nie wektor: ta sama umiejętność stoi w OBU katalogach docelowych, bo instalacja
    // pisze w oba. Lista z powtórzeniem pokazałaby człowiekowi dwa wiersze o jednym pliku
    // i policzyłaby go dwa razy w liczniku nad sekcją.
    let mut names: BTreeSet<String> = BTreeSet::new();

    for dir in
        crate::skills::place::destinations(Scope::Global, &roots.home, roots.project.as_deref())
    {
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
                names.insert(entry.file_name().to_string_lossy().into_owned());
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
    // moment, w którym `install_skill_inner` widzi katalog w połowie zapisany.
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
/// [`list_skills_inner`] odpowiadał „z internetu" wtedy, gdy obok zainstalowanego katalogu
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
/// Nieobecność zapisu jest odpowiedzią, którą [`list_skills_inner`] umie obsłużyć ostrożnie,
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
/// Zakres zostaje globalny, dokładnie jak na drodze linku — wybór „ten projekt / wszędzie"
/// jest osobnym zadaniem (T-44).
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
pub fn install_skill_inner(library: &Path, name: &str) -> Result<Vec<PathBuf>, Error> {
    let canonical = library.join(SKILLS_DIR).join(name);
    let import = ingest::from_folder(&canonical).map_err(|error| Error::Invalid {
        // Nazwa przychodzi z okna, więc katalog może nie istnieć — a `place::Error` nie ma
        // wariantu na „nie ma czego instalować" i nie dostanie go tutaj: `skills/mod.rs` nie
        // należy do tego zadania (AGENTS.md §7).
        messages: vec![format!(
            "There is nothing to install under the name '{name}': {error}"
        )],
    })?;

    // Walidacja, plan i odmowa przed pierwszym zapisem — wszystko w `place::plan`. Zakres jest
    // globalny, bo okno nie przysyła innego; korzenie wyprowadza `global_roots`.
    let roots = global_roots(library);
    let plan = crate::skills::place::plan(&import.skill, Scope::Global, &roots)?;
    crate::skills::place::apply(&plan, &import.skill)?;
    Ok(plan.writes)
}

/// Zdejmuje umiejętność z katalogów agentów.
///
/// 2026-08-18 — POWSTAŁO, BO SEKCJA MIAŁA PRZYCISK BEZ SKUTKU. Lista z
/// [`list_skills_inner`] czyta katalogi vendorów, więc wiersz na ekranie odpowiada plikom na
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
pub fn delete_skill_inner(library: &Path, name: &str) -> Result<(), Error> {
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

    let roots = global_roots(library);
    match crate::skills::place::remove(name, Scope::Global, &roots)? {
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
