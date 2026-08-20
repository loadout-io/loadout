//! AC-4 dla T-57: dziedziczy się to, co człowiek wybrał — i **domyślnie nic**.
//!
//! Repozytorium gospodarza to cudzy tekst, którego nikt nie audytował. Bieg, który wciąga go
//! „bo był", każe człowiekowi płacić za kontekst, o który nie prosił, i oddaje odpowiedzi
//! oparte na regułach, których ten człowiek nigdy nie zobaczył. Dlatego pusty wybór jest
//! stanem domyślnym, a nie trybem awaryjnym.
//!
//! **Słabą wersją tego kryterium jest test na samym punkcie (a)** i mówi to wprost sam
//! kontrakt. Przechodzi dla implementacji, która przy niepustej liście dziedziczy WSZYSTKO —
//! czyli zamienia wybór człowieka w przełącznik „host: tak/nie". Rozróżnia to punkt (b):
//! fikstura ma trzy umiejętności, wybrane są dwie, a trzecia ma nie przekroczyć granicy ani
//! nazwą, ani treścią.
//!
//! **Druga słaba wersja jest po stronie odmowy.** Ciche pominięcie nazwy, której u gospodarza
//! nie ma, daje bieg, w którym człowiek zaznaczył pięć pozycji, agent dostał trzy i nikt się
//! o tym nie dowiedział — bo „agent nie zna tej umiejętności" jest z zewnątrz nieodróżnialne od
//! „model nie uznał, że warto jej użyć". Punkt (c) wymaga więc odmowy, która **wymienia
//! nazwę**: odmowa bez nazwy zamienia jedno odznaczenie w przeszukiwanie listy.
//!
//! Odmowa pada, ZANIM cokolwiek zostanie zapisane, i to jest osobna asercja. Bieg, który
//! przepisał połowę wyboru i dopiero potem odmówił, zostawia katalog pluginu w kształcie,
//! którego nikt nie zamawiał — a katalog, który powstał, prędzej czy później zostanie komuś
//! podany.
//!
//! JEDEN `#[test]`: zaślepka, która nie dziedziczy niczego i nigdy nie odmawia, przechodzi
//! punkt (a) — rozbite na osobne zestawy dałyby w warstwie `before` obraz „w połowie zielony".

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::engine::drivers::{Policy, RunSpec};
use loadout_lib::inherit::scan;
use loadout_lib::inherit::wire::{self, Chosen};
use uuid::Uuid;

/// Trzy umiejętności, z czego wybrane będą dwie. Trzecia niesie własny znacznik, żeby jej
/// nieobecność dało się sprawdzić także po TREŚCI, nie tylko po nazwie katalogu.
const ALPHA_MD: &str = "---\nname: alpha\ndescription: Reads a log and says what broke.\n---\n\nStart from the first stack trace.\n";
const BETA_MD: &str = "---\nname: beta\ndescription: Turns a failing gate into one sentence.\n---\n\nQuote the first failing assertion.\n";
const GAMMA_MD: &str = "---\nname: gamma\ndescription: The one nobody picked.\n---\n\nGAMMA-ONLY-5b73 — this file stays in the host repository.\n";
const GAMMA_MARKER: &str = "GAMMA-ONLY-5b73";

/// Nazwa, której u gospodarza nie ma. Wybrana obok jednej, która jest — odmowa ma paść mimo
/// tego, że połowa wyboru jest w porządku.
const MISSING: &str = "delta";

/// Pełne `.claude/` gospodarza: umiejętności, learnings i podagent. Punkt (a) mierzy właśnie
/// takie repozytorium — „domyślnie nic" ma być prawdą tam, gdzie jest co brać.
const LEARNINGS_MD: &str = "# Learnings — backend-dev\n\n## Recurring patterns (BINDING)\n\n- LEARNINGS-ONLY-4d19: never hold a std mutex across an await.\n\n## Run journal\n\n- nothing yet.\n";
const SUBAGENT_MD: &str = "---\nname: release-engineer\nmodel: opus\n---\n\nSUBAGENT-ONLY-7c31 — cut the notes from the merged pull requests.\n";

/// Znacznik zadania kroku: prompt bez doklejki ma być tym samym promptem, co do bajtu.
const STEP_MARKER: &str = "STEP-PROMPT-4e60";

/// Repozytorium gospodarza i trzy katalogi biegu — po jednym na przypadek, żeby żaden nie
/// oglądał śladów po poprzednim.
struct World {
    /// Trzyma katalog tymczasowy przy życiu na czas testu; kasuje go `Drop`.
    _tmp: tempfile::TempDir,
    project: PathBuf,
    /// Bieg bez jawnego wyboru.
    nothing_chosen: PathBuf,
    /// Bieg z dwiema wybranymi umiejętnościami.
    two_chosen: PathBuf,
    /// Bieg z nazwą, której u gospodarza nie ma.
    unknown_name: PathBuf,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("host-repo");
    let claude = project.join(".claude");

    let skills = claude.join("skills");
    for (name, text) in [("alpha", ALPHA_MD), ("beta", BETA_MD), ("gamma", GAMMA_MD)] {
        fs::create_dir_all(skills.join(name)).unwrap();
        fs::write(skills.join(name).join("SKILL.md"), text).unwrap();
    }

    fs::create_dir_all(claude.join("learnings")).unwrap();
    fs::write(
        claude.join("learnings").join("backend-dev.md"),
        LEARNINGS_MD,
    )
    .unwrap();
    fs::create_dir_all(claude.join("agents")).unwrap();
    fs::write(
        claude.join("agents").join("release-engineer.md"),
        SUBAGENT_MD,
    )
    .unwrap();

    let runs = project.join(".loadout").join("runs");
    World {
        nothing_chosen: runs.join("20260819T101500__r7"),
        two_chosen: runs.join("20260819T101501__r8"),
        unknown_name: runs.join("20260819T101502__r9"),
        _tmp: tmp,
        project,
    }
}

fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: format!("{STEP_MARKER}: rename the widget."),
        model: None,
        system_append: Some("Answer in English.".to_owned()),
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Wszystko, co leży pod `root`, ścieżkami względnymi. Pusty zbiór także wtedy, gdy `root` nie
/// istnieje — i to jest odpowiedź, na której stoi punkt (a).
fn files_under(root: &Path) -> BTreeSet<PathBuf> {
    let mut found = BTreeSet::new();
    walk(root, Path::new(""), &mut found);
    found
}

fn walk(dir: &Path, prefix: &Path, found: &mut BTreeSet<PathBuf>) {
    let Ok(listing) = fs::read_dir(dir) else {
        return;
    };
    for entry in listing {
        let entry = entry.unwrap();
        let relative = prefix.join(entry.file_name());
        if fs::symlink_metadata(entry.path()).unwrap().is_dir() {
            walk(&entry.path(), &relative, found);
        } else {
            found.insert(relative);
        }
    }
}

/// Czy którykolwiek plik pod `root` niesie ten napis.
fn anything_says(root: &Path, needle: &str) -> bool {
    files_under(root)
        .iter()
        .filter_map(|relative| fs::read(root.join(relative)).ok())
        .any(|bytes| String::from_utf8_lossy(&bytes).contains(needle))
}

#[test]
fn nothing_is_inherited_by_default_and_a_choice_is_honoured_item_by_item()
-> Result<(), Box<dyn Error>> {
    let world = world();

    // (d) KONTROLA. Fikstura ma naprawdę mieć z czego wybierać: przy jednej umiejętności
    // „wybrane dwie z trzech" nie odróżnia wyboru od przełącznika, a punkt (a) przechodziłby
    // dla repozytorium, w którym i tak nie ma czego dziedziczyć.
    let found = scan::skills(&world.project)?;
    assert!(
        found.len() >= 3,
        "the fixture offers {} skill(s); with fewer than three, honouring a choice of two is \
         indistinguishable from inheriting everything",
        found.len()
    );

    // ── (a) Bez jawnego wyboru: ani flagi, ani jednego bajtu w prompcie ───────────────────
    let before = spec(&world.project);
    let by_default =
        wire::from_the_host(&world.project, &world.nothing_chosen, &Chosen::default())?;

    assert!(
        by_default.flags().is_empty(),
        "this run was given no explicit choice and still carries {:?}. A full .claude/ in the \
         folder somebody opened is not consent: it is somebody else's repository, and the \
         person running Loadout never saw what is in it",
        by_default.flags()
    );

    let untouched = by_default.applied_to(before.clone());
    assert_eq!(
        untouched.prompt, before.prompt,
        "nothing was chosen, and the step's prompt changed anyway"
    );
    assert_eq!(
        untouched.system_append, before.system_append,
        "nothing was chosen, and the step's system prompt changed anyway"
    );
    for marker in ["LEARNINGS-ONLY-4d19", "SUBAGENT-ONLY-7c31", GAMMA_MARKER] {
        assert!(
            !untouched.prompt.contains(marker),
            "{marker:?} reached the prompt of a run that inherits nothing. The host's .claude is \
             full, and that is exactly the case this point measures"
        );
    }

    let left_behind = files_under(&world.nothing_chosen);
    assert!(
        left_behind.is_empty(),
        "nothing was chosen, and this run's directory holds {left_behind:?}. A plugin directory \
         that exists will be handed to somebody sooner or later"
    );

    // ── (b) Wybór respektowany CO DO SZTUKI ───────────────────────────────────────────────
    let chosen = Chosen {
        skills: vec!["alpha".to_owned(), "beta".to_owned()],
        ..Chosen::default()
    };
    assert_eq!(
        chosen.skills.len(),
        2,
        "this point only means something when the choice is a strict subset of the fixture"
    );
    let inherited = wire::from_the_host(&world.project, &world.two_chosen, &chosen)?;

    let flags = inherited.flags();
    let plugin = PathBuf::from(
        flags
            .get(1)
            .ok_or("two chosen skills produced no [--plugin-dir, <dir>] fragment")?,
    );

    // RÓWNOŚĆ zbioru, nie zawieranie: „czy alpha jest" nigdy nie pyta „czy tylko alpha i beta".
    // Implementacja, która wsypała do katalogu wszystko, co znalazła, przechodzi każde pytanie
    // o obecność.
    let carried: BTreeSet<PathBuf> = files_under(&plugin.join("skills"));
    let expected: BTreeSet<PathBuf> = [
        PathBuf::from("alpha").join("SKILL.md"),
        PathBuf::from("beta").join("SKILL.md"),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        carried, expected,
        "the plugin directory carries {carried:?}. The person picked two of three, and a choice \
         that is honoured only as \"the host: yes or no\" is not a choice"
    );

    // …i trzecia umiejętność nie przeszła też TREŚCIĄ, pod żadną inną nazwą.
    assert!(
        !anything_says(&world.two_chosen, GAMMA_MARKER),
        "the text of the skill nobody picked is somewhere under {:?}",
        world.two_chosen
    );

    // ── (c) Nazwa spoza tego, co skan znalazł: odmowa, która ją wymienia ──────────────────
    let with_a_ghost = Chosen {
        skills: vec!["alpha".to_owned(), MISSING.to_owned()],
        ..Chosen::default()
    };
    let refusal = wire::from_the_host(&world.project, &world.unknown_name, &with_a_ghost)
        .err()
        .ok_or(
            "a name the host does not have was accepted. Leaving it out quietly gives a run \
             where the person picked two skills, the agent got one, and nothing anywhere says so",
        )?;
    let said = refusal.to_string();
    assert!(
        said.contains(MISSING),
        "the refusal does not name the item: {said:?}. A refusal that does not say which entry \
         it is about turns one unticked box into a search through the whole list"
    );

    // Odmowa PRZED zapisem. Bieg, który przepisał połowę wyboru i dopiero potem odmówił,
    // zostawia katalog pluginu w kształcie, którego nikt nie zamawiał.
    let half_written = files_under(&world.unknown_name);
    assert!(
        half_written.is_empty(),
        "the run refused and still left {half_written:?} behind"
    );

    Ok(())
}
