//! Rozmieszczanie: walidacja, emiter `SpecStrict`, plan, kopiowanie, usuwanie i werdykt
//! „czy vendor to widzi".
//!
//! Cicha porażka, przed którą stoi cały ten plik, to zielony ptaszek „Installed for 6 tools"
//! postawiony dlatego, że `fs::write` zwróciło `Ok`. Plik leży, ścieżka jest o poziom obok tej,
//! w którą vendor zagląda, a użytkownik dowiaduje się o tym nigdy — bo „agent nie wie
//! o umiejętności" nie da się odróżnić od „model nie uznał, że warto jej użyć". Dlatego
//! [`discovery_from_init`] czyta zdarzenie od vendora zamiast wnioskować z kodu powrotu.
//!
//! **Kopiujemy, nie symlinkujemy** [T5 §4.5]. Dowiązanie działa u Claude Code i jest tam nawet
//! udokumentowane, ale: u pozostałych pięciu vendorów jest niezweryfikowane, rozpada się
//! u każdego kolegi z zespołu, który sklonuje repo z umiejętnością w zakresie projektu, i na
//! Windowsie wymaga trybu dewelopera albo uprawnień administratora — czyli jest największym
//! zagrożeniem dla przenośności w całym tym projekcie. `fs::copy` zachowuje przy tym
//! uprawnienia, więc `scripts/run.sh` zostaje wykonywalny.
//!
//! Kodu platformowego tu nie ma (niezmiennik 3): dowiązanie wykrywamy `fs::symlink_metadata`,
//! nie `#[cfg(unix)]`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    DESTINATION_DIRS, Error, NON_SPEC_FIELDS, RESERVED_DIR_NAME, Result, Roots, SPEC_FIELDS, Scope,
    Skill, SkillDoc,
};

/// Nazwa pliku umiejętności. Jedna u wszystkich sześciu vendorów [T5 §2.2] — zmienna, żeby
/// „ten sam plik pod dwiema ścieżkami" znaczyło dosłownie to samo w zapisie i w odczycie
/// pierwszego wiersza cudzego katalogu.
const SKILL_FILE: &str = "SKILL.md";

/// Zdanie, którym `context: fork` wraca do ciała [T5 §4.2].
///
/// `context` jest polem Claude Code i żaden z pozostałych pięciu vendorów go nie zna, więc
/// jedyne, co z niego zostaje przenośne, to instrukcja napisana wprost do modelu.
const FORK_SENTENCE: &str = "Run this as an isolated task.";

/// Tablica pól ma sześć pozycji i [`spec_line`] ma sześć gałęzi. Siódme pole dopisane do
/// [`SPEC_FIELDS`] bez gałęzi tutaj nie jest błędem kompilacji — po cichu **nie jedzie do
/// pliku**, a `SKILL.md` bez pola wygląda jak `SKILL.md`, w którym autor go nie podał.
/// Ta linia zamienia tę ciszę w błąd kompilacji.
const _: () = assert!(SPEC_FIELDS.len() == 6);

/// Co instalacja zapisze — pokazywane człowiekowi, zanim cokolwiek się wydarzy.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    /// Dwa katalogi umiejętności: `<korzeń>/.claude/skills/<name>` i `…/.agents/skills/<name>`.
    pub writes: Vec<PathBuf>,
    /// Katalogi o tej nazwie, które już tam są — nasze do nadpisania albo cudze.
    pub conflicts: Vec<Conflict>,
    /// Sidecar, w którym [`apply`] zapisze, że te katalogi napisał Loadout.
    ///
    /// Plan niesie tę ścieżkę, żeby [`apply`] nie potrzebowało [`Roots`]: plan jest pełnym
    /// opisem tego, co się stanie, a zapis „to jest nasze" jest częścią tego, co się stanie.
    /// Sidecar leży poza oboma drzewami docelowymi — jest zapisem Loadouta o wyjściu builda,
    /// nie kolejnym plikiem obok `SKILL.md` (niezmiennik 21: nikt nie czyta `.loadout-marker`,
    /// a sidecar czytają [`plan`] i [`remove`]).
    pub sidecar: PathBuf,
}

/// Katalog o tej samej nazwie już istnieje w miejscu docelowym.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// Loadout pisał ten katalog wcześniej — jest w sidecarze. Instalacja go nadpisze,
    /// i tak ma być: katalogi vendorów są wyjściem builda (niezmiennik 4).
    Update { path: PathBuf },
    /// Katalogu nie ma w sidecarze, więc nie jest nasz. Nie nadpisujemy go bez pytania.
    Foreign {
        path: PathBuf,
        /// Pierwszy wiersz cudzego `SKILL.md`, zacytowany dosłownie — żeby człowiek
        /// zobaczył, czyj to plik, zanim zdecyduje.
        first_line: String,
    },
}

/// Wynik usunięcia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removed {
    /// Obie kopie zdjęte. Kanoniczna umiejętność w danych aplikacji zostaje — usuwamy
    /// wyjście builda, nie źródło (niezmiennik 4).
    Done { paths: Vec<PathBuf> },
    /// Co najmniej jeden katalog o tej nazwie nie jest nasz. Nie kasujemy **niczego**:
    /// pół usunięcia zostawia stan, którego nikt nie umie opisać, a cudza umiejętność
    /// skasowana „przy okazji" jest nie do odzyskania.
    Skipped {
        path: PathBuf,
        /// Zdanie dla człowieka: dlaczego to zostało.
        why: String,
    },
}

/// Czy vendor naprawdę widzi umiejętność [T5 §6.3, poziom 3].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    /// Vendor wymienił ją na swojej liście.
    Seen,
    /// Vendor wymienił swoje umiejętności i tej wśród nich nie ma.
    NotSeen {
        /// Ścieżki, w które pisaliśmy. To jest cała treść zgłoszenia dla człowieka:
        /// „napisaliśmy tu i tu, a vendor tego nie widzi".
        looked_in: Vec<PathBuf>,
    },
    /// Nie wiadomo — i to nie jest błąd (niezmiennik 5). Brak CLI, zdarzenie o nieznanym
    /// kształcie, nowa wersja vendora: żadne z tego nie może zaświecić się na czerwono.
    Unknown(&'static str),
}

/// Dwa katalogi docelowe dla danego zakresu — same korzenie, bez `<name>`.
///
/// `project = None` przy [`Scope::Project`] daje ścieżki względne, czyli „tutaj": tak samo
/// rozwiązuje je Codex (`$CWD/.agents/skills`). Do dysku to nigdy nie dochodzi — [`plan`]
/// odmawia zakresu projektowego bez korzenia zwrotem [`super::Error::NoProjectRoot`].
#[must_use]
pub fn destinations(scope: Scope, home: &Path, project: Option<&Path>) -> [PathBuf; 2] {
    let root = match scope {
        Scope::Global => home,
        Scope::Project => project.unwrap_or(Path::new("")),
    };
    DESTINATION_DIRS.map(|dir| root.join(dir))
}

/// Reguły walidatora referencyjnego, przepisane w Ruście [T5 §6.2].
///
/// Przepisane, nie wywołane: `agentskills` jest w Pythonie, a `uv` jako zależność środowiska
/// uruchomieniowego aplikacji desktopowej w Ruście to zły interes za ~40 linii reguł. CLI
/// zostaje wyrocznią różnicową w naszym własnym CI, nie w produkcie.
///
/// Komunikaty są **dosłownie** te z wyroczni, bo użytkownik zobaczy je też wtedy, gdy vendor
/// odmówi po swojemu — a dwa różne zdania o tej samej przyczynie to dwa różne zgłoszenia.
/// Jedna przyczyna, jeden komunikat: wspólne „invalid skill" na osiem przyczyn nie mówi
/// nikomu, co poprawić.
///
/// Nazwa katalogu jest osobnym argumentem, bo dwie reguły dotyczą **jej**, nie pliku:
/// zgodność z `name` i zarezerwowane [`super::RESERVED_DIR_NAME`].
pub fn validate_strict(dir_name: &str, doc: &SkillDoc) -> Result<(), Vec<String>> {
    /// Limity ze specyfikacji [T5 §2.3]. Stoją tutaj, przy jedynej funkcji, która ich używa;
    /// druga kopia „dla podglądu w UI" jest tym, jak jeden z nich zostaje na starej wartości.
    const NAME_MAX: usize = 64;
    const DESCRIPTION_MAX: usize = 1024;
    const COMPATIBILITY_MAX: usize = 500;

    /// Słowa zastrzeżone w `name`. Nie ma ich w specyfikacji agentskills.io, są w dokumentacji
    /// platformy Anthropic — i kosztują jedno porównanie, więc egzekwujemy [T5 §2.3].
    const RESERVED_WORDS: [&str; 2] = ["anthropic", "claude"];

    let mut said = Vec::new();
    let field = |key: &str| {
        doc.fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
            .filter(|value| !value.trim().is_empty())
    };

    match field("name") {
        // Nazwy nie ma wcale — reszta reguł o nazwie nie ma o czym mówić, a jedna przyczyna
        // to jedno zdanie (nie osiem zdań o jednym brakującym polu).
        None => said.push("Missing required field in frontmatter: name".to_owned()),
        Some(name) => {
            if name != name.to_lowercase() {
                said.push(format!("Skill name '{name}' must be lowercase"));
            }
            if !is_slug(name) {
                said.push(format!(
                    "Skill name '{name}' must be lowercase letters, digits and single hyphens"
                ));
            }
            if name.chars().count() > NAME_MAX {
                said.push(format!(
                    "Skill name '{name}' must be {NAME_MAX} characters or less"
                ));
            }
            if let Some(word) = RESERVED_WORDS
                .iter()
                .find(|word| name.to_lowercase().contains(*word))
            {
                said.push(format!(
                    "Skill name '{name}' must not contain the reserved word '{word}'"
                ));
            }
            // Nazwa katalogu jest tym, co użytkownik wpisuje jako `/name`. Kiedy nie zgadza
            // się z polem, Claude Code bierze katalog, a lista bierze pole — i umiejętność
            // ma dwie nazwy zależnie od tego, kto pyta.
            if dir_name != name {
                said.push(format!(
                    "Directory name '{dir_name}' must match skill name '{name}'"
                ));
            }
        }
    }

    if field("description").is_none() {
        said.push("Missing required field in frontmatter: description".to_owned());
    }
    if field("description").is_some_and(|value| value.chars().count() > DESCRIPTION_MAX) {
        said.push(format!(
            "Skill description must be {DESCRIPTION_MAX} characters or less"
        ));
    }
    if field("compatibility").is_some_and(|value| value.chars().count() > COMPATIBILITY_MAX) {
        said.push(format!(
            "Skill compatibility must be {COMPATIBILITY_MAX} characters or less"
        ));
    }

    // Umiejętność napisana w folderze o tej nazwie jest poprawna, zainstalowana i niewidoczna
    // — Claude Code taki folder pomija, w dowolnej wielkości liter [T5 fact-check]. To ta sama
    // klasa cichej porażki co zła ścieżka, więc blokujemy, zamiast ostrzegać.
    if dir_name.eq_ignore_ascii_case(RESERVED_DIR_NAME) {
        said.push(format!(
            "Directory name '{dir_name}' is reserved: Claude Code skips a skill written in a \
             folder called '{RESERVED_DIR_NAME}'"
        ));
    }

    // Zbiór, nie lista: to samo pole wpisane dwa razy jest jedną przyczyną. Posortowany, bo
    // komunikat ma być ten sam niezależnie od kolejności pól w pliku, który go wywołał.
    let unexpected: BTreeSet<&str> = doc
        .fields
        .iter()
        .map(|(key, _)| key.as_str())
        .filter(|key| !SPEC_FIELDS.contains(key))
        .collect();
    if !unexpected.is_empty() {
        let named: Vec<&str> = unexpected.into_iter().collect();
        said.push(format!(
            "Unexpected fields in frontmatter: {}. Only [{}] are allowed.",
            named.join(", "),
            allowed_list()
        ));
    }

    if said.is_empty() { Ok(()) } else { Err(said) }
}

/// `^[a-z0-9]+(-[a-z0-9]+)*$`, przepisane bez zależności na wyrażenia regularne.
///
/// Trzy warunki na łączniku to całe wyrażenie: bez wiodącego, bez końcowego, bez podwójnego.
fn is_slug(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Sześć dozwolonych pól tak, jak wylicza je walidator referencyjny: alfabetycznie,
/// w apostrofach, po przecinku.
///
/// Liczone z [`SPEC_FIELDS`], nie wpisane drugi raz: literał obok tablicy jest tym, jak
/// komunikat zaczyna wymieniać pole, którego emiter już nie zna.
fn allowed_list() -> String {
    let mut names = SPEC_FIELDS;
    names.sort_unstable();
    names
        .iter()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Jeden `SKILL.md` w trybie `SpecStrict` i lista pól, które zostały zdjęte.
///
/// Jeden plik do obu katalogów, nie dwa warianty: jeden plik to jeden diff i jedna rzecz do
/// zdebugowania. `ClaudeExtended` jest świadomie poza zakresem [T5 §11].
///
/// Zdjęte pola **wracają**, nie giną: `argument-hint` jako wiersz `Arguments: …`, a
/// `context: fork` jako `Run this as an isolated task.` przed pierwszym akapitem [T5 §4.2].
/// Reszta z czternastki nie ma przenośnego odpowiednika i zostaje tylko na zwróconej liście —
/// po to, żeby UI mogło powiedzieć, co dokładnie zniknęło.
#[must_use]
pub fn emit(skill: &Skill) -> (String, Vec<&'static str>) {
    // Kolejność bierze się z SPEC_FIELDS, a nie z kolejności gałęzi w `spec_line`. Dzięki
    // temu przestawienie pól w tablicy przestawia plik, zamiast rozjechać się z nim.
    let front: String = SPEC_FIELDS
        .into_iter()
        .filter_map(|field| spec_line(skill, field))
        .collect();

    // Zdjęte nie znaczy skasowane. Te dwa pola mają przenośny odpowiednik w prozie i wracają
    // PRZED pierwszym akapitem: agent, który przeczyta instrukcję wcześniej niż to, jak go
    // wywołano, wykona ją z niepełną wiedzą [T5 §4.2].
    let mut preamble = String::new();
    if let Some(hint) = skill
        .extras
        .get("argument-hint")
        .filter(|hint| !hint.trim().is_empty())
    {
        preamble.push_str(&format!("Arguments: {hint}\n"));
    }
    // Tylko `fork`. Inna wartość `context` znaczy coś, czego nie umiemy przetłumaczyć, a
    // zdanie postawione „na wszelki wypadek" kłamie o tym, jak umiejętność pobiegnie.
    if skill
        .extras
        .get("context")
        .is_some_and(|context| context.trim() == "fork")
    {
        preamble.push_str(FORK_SENTENCE);
        preamble.push('\n');
    }
    if !preamble.is_empty() && !skill.body.is_empty() {
        preamble.push('\n');
    }

    // Zwracamy `&'static str` z tablicy, nie klucze z mapy: lista zdjętych pól ma nazywać
    // czternaście pól, o których wiemy, dlaczego spadły. Pole spoza tej czternastki (import
    // z jutrzejszej wersji vendora) też nie trafia do front-mattera — ale nie umiemy o nim
    // powiedzieć nic ponad „nie ma go w specyfikacji", więc go nie nazywamy.
    let stripped: Vec<&'static str> = NON_SPEC_FIELDS
        .into_iter()
        .filter(|field| skill.extras.contains_key(*field))
        .collect();

    (
        format!("---\n{front}---\n{preamble}{}", skill.body),
        stripped,
    )
}

/// Jeden wiersz (albo blok) front-mattera dla nazwanego pola specyfikacji — albo `None`,
/// kiedy pola nie ma.
///
/// `None` nie jest tym samym, co pusta wartość: `license:` bez niczego za dwukropkiem jest
/// wartością, o którą następny czytelnik musi zapytać, a `metadata:` bez par jest mapą pustą,
/// nie mapą nieobecną.
fn spec_line(skill: &Skill, field: &str) -> Option<String> {
    match field {
        "name" => (!skill.name.is_empty()).then(|| format!("name: {}\n", scalar(&skill.name))),
        "description" => (!skill.description.is_empty())
            .then(|| format!("description: {}\n", scalar(&skill.description))),
        "license" => skill
            .license
            .as_ref()
            .map(|value| format!("license: {}\n", scalar(value))),
        "compatibility" => skill
            .compatibility
            .as_ref()
            .map(|value| format!("compatibility: {}\n", scalar(value))),
        "metadata" => (!skill.metadata.is_empty()).then(|| {
            let pairs: String = skill
                .metadata
                .iter()
                .map(|(key, value)| format!("  {key}: {}\n", scalar(value)))
                .collect();
            format!("metadata:\n{pairs}")
        }),
        "allowed-tools" => skill
            .allowed_tools
            .as_ref()
            .map(|value| format!("allowed-tools: {}\n", scalar(value))),
        // Nieosiągalne, dopóki stoi `const _: () = assert!(SPEC_FIELDS.len() == 6)` u góry
        // pliku: siódma nazwa w tablicy nie skompiluje się, zamiast po cichu tu wpaść.
        _ => None,
    }
}

/// Wartość YAML-a: gołym tekstem, kiedy to bezpieczne, w cudzysłowie, kiedy nie.
///
/// DLACZEGO nie zawsze w cudzysłowie: `SKILL.md` w zakresie projektu ląduje w repo zespołu
/// i człowiek go czyta. DLACZEGO nie zawsze gołym: `description` przychodzi z importu
/// z sieci, a wartość zaczynająca się od `[`, zawierająca `: ` albo wyglądająca jak `true`
/// zmienia typ pola — i pięciu vendorów odmawia wtedy czegoś, czego autor nigdy nie napisał.
fn scalar(value: &str) -> String {
    const INDICATORS: [char; 15] = [
        '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '%',
    ];

    let plain = !value.is_empty()
        && value.trim() == value
        && !value.starts_with(INDICATORS)
        && !value.starts_with(['\'', '"', '@', '`'])
        && !value.contains(": ")
        && !value.contains(" #")
        && !value.ends_with(':')
        && !value.chars().any(char::is_control)
        // Gołe `true`, `null` i `42` wczytują się jako wartość innego typu niż tekst.
        && value.parse::<f64>().is_err()
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "~"
        );

    if plain {
        value.to_owned()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// Co się wydarzy, jeszcze zanim cokolwiek się wydarzy.
///
/// Waliduje **przed** pierwszym zapisem i nie tworzy ani jednego katalogu — żadnego
/// „utwórzmy, żeby sprawdzić uprawnienia". Dwa powody: użytkownik ma zobaczyć listę zmian,
/// zanim je zatwierdzi, a odmowa w połowie zostawia katalog, którego nikt nie posprząta.
pub fn plan(skill: &Skill, scope: Scope, roots: &Roots) -> Result<InstallPlan> {
    // Odmowa jest PIERWSZĄ rzeczą, jaka się tu dzieje. Nie „utwórzmy katalog, żeby sprawdzić
    // uprawnienia": katalog utworzony przed odmową zostaje na dysku i nikt go nie posprząta.
    let mut messages = validate_strict(&skill.name, &as_doc(skill))
        .err()
        .unwrap_or_default();
    // Ścieżka dołączonego pliku jest ścieżką WZGLĘDNĄ wewnątrz katalogu umiejętności.
    // `dir.join("/etc/x")` zwraca `/etc/x`, a `dir.join("../../x")` wychodzi poza oba drzewa
    // docelowe — czyli plan przestaje opisywać to, co się wydarzy. Źródło waliduje T-19,
    // ale zapis, który wychodzi poza katalog, jest sprawą tego pliku.
    messages.extend(
        skill
            .files
            .iter()
            .filter(|file| !stays_inside(&file.relative))
            .map(|file| {
                format!(
                    "Bundled file '{}' must stay inside the skill folder",
                    file.relative.display()
                )
            }),
    );
    if !messages.is_empty() {
        return Err(Error::Invalid { messages });
    }

    // Zakres projektu bez znanego korzenia to odmowa, nie katalog roboczy: zgadnięty korzeń
    // zapisuje umiejętność w losowym miejscu i nikt się o tym nie dowie.
    if scope == Scope::Project && roots.project.is_none() {
        return Err(Error::NoProjectRoot);
    }

    let sidecar = sidecar_path(&roots.data);
    let ours = recorded(&sidecar);
    let writes: Vec<PathBuf> = destinations(scope, &roots.home, roots.project.as_deref())
        .into_iter()
        .map(|dir| dir.join(&skill.name))
        .collect();

    // `symlink_metadata`, nie `exists()`: katalog będący dowiązaniem ma zostać zauważony jako
    // kolizja, a nie po cichu przejrzany na wylot do tego, na co wskazuje.
    let conflicts = writes
        .iter()
        .filter(|dir| std::fs::symlink_metadata(dir).is_ok())
        .map(|dir| {
            if ours.contains(dir) {
                Conflict::Update { path: dir.clone() }
            } else {
                Conflict::Foreign {
                    path: dir.clone(),
                    first_line: first_line(dir),
                }
            }
        })
        .collect();

    Ok(InstallPlan {
        writes,
        conflicts,
        sidecar,
    })
}

/// [`Skill`] widziany jako dokument, który za chwilę powstanie.
///
/// Walidujemy to, co **wyjdzie na dysk**, a nie to, co przyszło z importu: pola spoza szóstki
/// zdejmuje [`emit`], więc gdyby trafiły tutaj, każda umiejętność z `argument-hint` byłaby
/// odrzucona za pole, którego w zapisanym pliku i tak nie będzie.
///
/// Pole puste = pole nieobecne. `description: ""` jest dla modelu dokładnie tym samym, co brak
/// `description`: nie ma po czym zdecydować, czy w ogóle sięgnąć po umiejętność.
fn as_doc(skill: &Skill) -> SkillDoc {
    let mut fields = Vec::new();
    let mut put = |key: &str, value: &str| {
        if !value.is_empty() {
            fields.push((key.to_owned(), value.to_owned()));
        }
    };
    put("name", &skill.name);
    put("description", &skill.description);
    put("license", skill.license.as_deref().unwrap_or_default());
    put(
        "compatibility",
        skill.compatibility.as_deref().unwrap_or_default(),
    );
    if !skill.metadata.is_empty() {
        put("metadata", "present");
    }
    put(
        "allowed-tools",
        skill.allowed_tools.as_deref().unwrap_or_default(),
    );

    SkillDoc {
        fields,
        body: skill.body.clone(),
    }
}

/// Czy ścieżka dołączonego pliku zostaje wewnątrz katalogu umiejętności.
///
/// Jedno miejsce dla tej reguły, bo pyta o nią i [`plan`] (żeby odmówić), i [`apply`] (żeby
/// nie zapisać, gdyby ktoś podał mu plan i umiejętność z dwóch różnych wywołań).
fn stays_inside(relative: &Path) -> bool {
    !relative.as_os_str().is_empty()
        && relative.is_relative()
        && !relative
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
}

/// Wykonuje plan: `SKILL.md` z [`emit`] do obu katalogów, pliki dołączone przez `fs::copy`,
/// wpis do sidecara.
///
/// Tworzy dokładnie te ścieżki, które plan wymienił, i ani jednej więcej.
pub fn apply(plan: &InstallPlan, skill: &Skill) -> Result<()> {
    todo!("T-18 AC-4: zapis dokładnie tego, co w {plan:?}, dla {skill:?}")
}

/// Zdejmuje obie kopie umiejętności — i nic poza nimi.
///
/// Kanoniczna kopia w danych aplikacji zostaje: katalogi vendorów są wyjściem builda,
/// a źródło jest jedno (niezmiennik 4). Katalog, którego nie ma w sidecarze, jest cudzy
/// i nie jest kasowany.
pub fn remove(name: &str, scope: Scope, roots: &Roots) -> Result<Removed> {
    todo!("T-18 AC-6: zdjęcie obu kopii {name} dla {scope:?} {roots:?}")
}

// ── Sidecar ────────────────────────────────────────────────────────────────────────────────
//
// Jedyny zapis o tym, KTÓRY katalog vendora napisał Loadout. Bez niego „usuń umiejętność"
// nie umie odróżnić naszego `pdf/` od cudzego `pdf/`, a kolizja nazw jest normalna, nie
// wyjątkowa.
//
// DLACZEGO nie plik-znacznik obok `SKILL.md` (niezmiennik 21): znacznik leżałby w katalogu,
// który jest wyjściem builda i który wolno w całości odtworzyć — więc pierwsze `git clean`
// albo pierwsza ręczna kopia katalogu robi z cudzej umiejętności naszą. Sidecar leży
// w danych aplikacji, poza oboma drzewami docelowymi, i czytają go [`plan`] i [`remove`].

/// Katalog umiejętności w danych aplikacji: kanoniczne kopie i ten plik obok nich.
const SKILLS_DIR: &str = "skills";

/// Nazwa sidecara. Jedna, w jednym miejscu — [`plan`] wkłada ścieżkę do [`InstallPlan`],
/// a [`apply`] bierze ją stamtąd, zamiast liczyć drugi raz.
const SIDECAR_FILE: &str = "installed.json";

/// Zawartość sidecara. `#[serde(default)]` plus domyślne ignorowanie nieznanych pól:
/// plik zapisany przez nowszą wersję Loadouta ma się wczytać, a nie wywrócić bieg
/// (niezmiennik 5).
#[derive(Debug, Default, Serialize, Deserialize)]
struct Sidecar {
    /// Katalogi vendorów, które napisaliśmy. Posortowane, żeby `git diff` na tym pliku
    /// pokazywał zmianę, a nie przetasowanie.
    #[serde(default)]
    installed: Vec<String>,
}

fn sidecar_path(data: &Path) -> PathBuf {
    data.join(SKILLS_DIR).join(SIDECAR_FILE)
}

/// Katalogi, o których sidecar mówi „to nasze".
///
/// Brak pliku, plik nieczytelny i plik o nieznanym kształcie dają ten sam wynik: pusty zbiór.
/// To jest wybór w bezpieczną stronę — nie wiedząc, czy katalog jest nasz, traktujemy go jak
/// cudzy, więc najgorsze, co się stanie, to odmowa usunięcia. Odwrotny domyślny („skoro nie
/// wiem, to pewnie moje") kasuje cudzą pracę.
fn recorded(sidecar: &Path) -> BTreeSet<PathBuf> {
    fs::read_to_string(sidecar)
        .ok()
        .and_then(|text| serde_json::from_str::<Sidecar>(&text).ok())
        .map(|record| record.installed.into_iter().map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn write_sidecar(sidecar: &Path, paths: &BTreeSet<PathBuf>) -> Result<()> {
    if let Some(parent) = sidecar.parent() {
        fs::create_dir_all(parent)?;
    }
    let record = Sidecar {
        // `to_string_lossy`, nie `to_str().ok_or(..)`: ścieżka spoza UTF-8 ma nie wywrócić
        // instalacji. Zapis jest wtedy stratny i przy usuwaniu nie dopasuje się do katalogu,
        // czyli znowu wypadamy na „nie wiem, więc nie kasuję".
        installed: paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
    };
    let text = serde_json::to_string_pretty(&record).map_err(std::io::Error::other)?;
    fs::write(sidecar, text + "\n")?;
    Ok(())
}

/// Pierwszy wiersz `SKILL.md` w cudzym katalogu — dosłownie, żeby człowiek zobaczył, czyj to
/// plik, zanim zdecyduje.
///
/// Katalog bez czytelnego `SKILL.md` daje pusty łańcuch. Zdanie wymyślone w zastępstwie
/// byłoby zacytowane użytkownikowi tak, jakby stało w pliku.
fn first_line(dir: &Path) -> String {
    fs::read_to_string(dir.join(SKILL_FILE))
        .ok()
        .and_then(|text| text.lines().next().map(ToOwned::to_owned))
        .unwrap_or_default()
}

/// Werdykt „czy Claude to widzi", odczytany ze zdarzenia `system`/`init`.
///
/// Reguła ma **kolejność** i to jest jej cała treść: jeśli zdarzenie niesie tablicę `skills`,
/// liczy się wyłącznie ona; jeśli nie — liczy się `slash_commands`, bo tak umiejętność
/// z `~/.claude/skills` objawia się w CLI v2.1.233; jeśli nie ma żadnej z nich, odpowiedź
/// brzmi [`Discovery::Unknown`], nigdy „nie widzi".
///
/// DLACZEGO nie `init_line.contains(name)`: nazwa umiejętności potrafi wystąpić w `cwd`
/// (`/home/u/review-pull-requests/x`) i w `mcp_servers[].name`, nie występując w żadnej
/// z dwóch tablic. Wyszukiwanie po całej linii mówi wtedy „widzi" i to jest dokładnie ten
/// fałszywy zielony ptaszek, o który chodzi w tym zadaniu.
///
/// Pusty `init_line` znaczy „CLI nigdy nie wystartowało", czyli `Unknown("not installed")` —
/// brak vendora nie może być czerwony [T5 §6.3].
#[must_use]
pub fn discovery_from_init(name: &str, init_line: &str, wrote: &[PathBuf]) -> Discovery {
    todo!("T-18 AC-5: skills, potem slash_commands, dla {name} w {init_line} ({wrote:?})")
}
