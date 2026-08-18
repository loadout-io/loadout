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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::skills::ingest::{
    self, FILE_CAP, FetchError, Finding, Import, Reviewed, Target, Verdict, Weight,
};
use crate::skills::{Error, Roots, Scope};

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
            // Wszystko, co przechodzi przez [`review_skill_inner`], przyszło z sieci — to jest
            // jedyna droga, którą coś tu wchodzi. Znacznik jest trwały, więc jego wartość nie
            // ma prawa zależeć od tego, czy okno ją odesłało.
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
/// Z plików, bo inaczej nie wolno go zapisać (niezmiennik 4). Kopia kanoniczna powstaje
/// **wyłącznie** w [`review_skill_inner`], czyli na jedynej drodze, którą coś wchodzi tu z sieci
/// — jej obecność obok zainstalowanego katalogu jest więc trwałym śladem pochodzenia, którego
/// nie trzeba nigdzie zapisywać osobno. Umiejętność napisana ręcznie prosto w katalogu vendora
/// nie ma kopii kanonicznej i dostaje `false`, i tak ma być: znacznik zastępuje podpisy, których
/// v1 nie ma, więc ma świecić tam, gdzie treść przyszła od obcego.
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

    Ok(names
        .into_iter()
        .map(|name| InstalledWire {
            from_the_internet: library
                .join(SKILLS_DIR)
                .join(&name)
                .join(SKILL_FILE)
                .is_file(),
            name,
        })
        .collect())
}

/// Zdejmuje katalog, jeżeli tam jest. Brak katalogu to **nie** jest awaria.
///
/// Osobna funkcja, bo pobranie robi to dwa razy — raz na katalogu roboczym, raz na kopii
/// kanonicznej — a „nie ma go, czyli już jest zdjęty" napisane dwa razy to dwie okazje, żeby raz
/// pomylić kierunek porównania i zacząć wywracać się na pierwszym imporcie każdej umiejętności.
fn gone(dir: &Path) -> Result<(), FetchError> {
    match std::fs::remove_dir_all(dir) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(FetchError::Io(error)),
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

    let canonical = library.join(SKILLS_DIR).join(&import.skill.name);
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
