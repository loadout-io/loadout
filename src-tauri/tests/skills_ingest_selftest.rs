//! AC-6 dla T-19: samotest czyta z dysku, a nie ze swojego własnego planu.
//!
//! **Słabą wersją tego kryterium jest samotest zbudowany z `InstallPlan`, który przed chwilą
//! wykonaliśmy.** Przechodzi zawsze — i jest dokładnie tym zielonym ptaszkiem postawionym
//! dlatego, że `fs::write` zwróciło `Ok`, o którym mówi akapit otwierający T-18. Plik może
//! być pusty, obcięty, zapisany do połowy albo nadpisany przez czyjeś narzędzie sekundę
//! później; plan o żadnej z tych rzeczy nie wie i nigdy się nie dowie.
//!
//! Rozróżnia przypadek z obciętym plikiem: Tier 2 musi być **ponownym odczytem i ponownym
//! sparsowaniem z dysku**, więc te same dwie ścieżki dają dwa różne wyniki zależnie od tego,
//! co w nich leży. Samotest liczony z planu zwraca w obu przebiegach to samo, co do znaku —
//! i dlatego porównujemy też oba podsumowania ze sobą.
//!
//! Tier 3 nie jest tu implementowany drugi raz: werdykt bierze `discovery_from_init` z T-18.
//! Sprawdzamy dwie rzeczy o tej delegacji — że brak CLI daje `Unknown` (pokazywane jako
//! `not installed`, nigdy jako porażka [T5 §6.3]) i że zdarzenie wymieniające umiejętność
//! daje `Seen`. Bez tej drugiej „Unknown" mogłoby być wpisane na sztywno.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use loadout_lib::skills::ingest::{self, Installed, SelfTest, SpecCheck};
use loadout_lib::skills::place;
use loadout_lib::skills::place::Discovery;
use loadout_lib::skills::{Roots, Scope, Skill};

const NAME: &str = "pdf";
const DESCRIPTION: &str = "Extracts text and tables from PDF files.";
const BODY: &str = "Read the file first, then answer from what it says.\n";

/// Zdanie, którym karta mówi „wszystko na miejscu" [T5 §8.3, „Installed for 6 tools."].
/// Liczba jest liczbą MIEJSC, tak samo jak `of` — dwa katalogi, które pokrywa jedna instalacja.
const ALL_GOOD: &str = "Installed for 2 tools.";

/// Zdarzenie `init` od vendora, który umiejętność wymienia. Kształt jest ten sam, co w T-18.
const INIT_WITH_SKILL: &str = r#"{"type":"system","subtype":"init","skills":["pdf"]}"#;

/// Dom, dane aplikacji i umiejętność już zainstalowana w obu katalogach docelowych.
struct World {
    /// Trzyma katalog tymczasowy przy życiu na czas testu; kasuje go `Drop`.
    _tmp: tempfile::TempDir,
    skill: Skill,
    /// Katalogi, w które instalacja naprawdę pisała — to samo, co dostaje `self_test`.
    writes: Vec<PathBuf>,
}

fn installed_world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let roots = Roots {
        home,
        project: None,
        data: tmp.path().join("data"),
    };
    let skill = Skill {
        name: NAME.to_owned(),
        description: DESCRIPTION.to_owned(),
        body: BODY.to_owned(),
        ..Skill::default()
    };

    let plan = place::plan(&skill, Scope::Global, &roots)
        .expect("a skill with a valid name and a description should be plannable");
    place::apply(&plan, &skill).expect("apply could not carry out its own plan");

    World {
        _tmp: tmp,
        skill,
        writes: plan.writes,
    }
}

fn run(world: &World, init_line: &str) -> SelfTest {
    ingest::self_test(&world.skill, &world.writes, init_line)
}

#[test]
fn a_file_emptied_after_the_install_is_seen_as_broken() {
    let world = installed_world();
    let intact = run(&world, "");

    assert_eq!(
        intact.installed,
        Installed {
            ok: 2,
            of: 2,
            broken: Vec::new(),
        },
        "both copies were written a moment ago and both parse, so this is the complete case"
    );

    // Jedna z dwóch kopii znika co do treści, a ścieżka zostaje. To jest kształt awarii, który
    // Tier 2 ma łapać: `fs::write` zwróciło Ok, plik jest, i nie ma w nim umiejętności.
    let doomed = world.writes[0].clone();
    fs::write(doomed.join("SKILL.md"), "").unwrap();
    let after = run(&world, "");

    assert_eq!(
        after.installed,
        Installed {
            ok: 1,
            of: 2,
            broken: vec![doomed.clone()],
        },
        "one of the two copies is zero bytes long. A self-test built from the plan we just \
         carried out cannot tell this apart from a healthy install, because the plan says what \
         we meant to write, not what is on disk"
    );
    assert_eq!(
        after.installed.ok + after.installed.broken.len(),
        after.installed.of,
        "every destination is either readable or named as broken; a copy that is neither is a \
         copy nobody will ever look at"
    );

    assert!(
        !after.summary.iter().any(|line| line.contains(ALL_GOOD)),
        "the card still says `{ALL_GOOD}` next to an install that is half broken. A complete \
         sentence over an incomplete install is worse than no sentence: it stops the person \
         from checking the one thing that just did not work.\n{:?}",
        after.summary
    );
    assert_ne!(
        after.summary, intact.summary,
        "the summary is the same before and after one of the two files was emptied, so it is \
         being computed from the plan and not from what is on disk"
    );
}

#[test]
fn an_untouched_install_says_so_in_both_fields_and_in_the_sentence() {
    let world = installed_world();
    let result = run(&world, "");

    assert_eq!(
        result.valid,
        SpecCheck::Valid,
        "Tier 1 is spec validity and this skill has a name, a description and nothing else"
    );
    assert_eq!(
        result.installed,
        Installed {
            ok: 2,
            of: 2,
            broken: Vec::new(),
        }
    );
    assert!(
        result.summary.iter().any(|line| line.contains(ALL_GOOD)),
        "with both copies readable the card has to say so, in words. Without this direction the \
         honest implementation would be one that never claims anything, and a self-test that \
         never says `it worked` is a self-test nobody reads.\n{:?}",
        result.summary
    );
}

#[test]
fn tier_three_takes_its_answer_from_the_vendor_and_never_calls_a_missing_cli_a_failure() {
    let world = installed_world();

    let offline = run(&world, "");
    assert_eq!(
        offline.discovered,
        Discovery::Unknown("not installed"),
        "an empty init line means the CLI never started. That is not the same as `the vendor \
         does not see the skill`, and showing it as a failure teaches people to ignore the one \
         tier that would catch a vendor moving its skills directory [T5 §6.3]"
    );
    for line in &offline.summary {
        assert!(
            !line.to_lowercase().contains("fail"),
            "a vendor that is not installed is reported as `not installed`, never as something \
             that failed: {line}"
        );
    }
    assert!(
        offline
            .summary
            .iter()
            .any(|line| line.contains("not installed")),
        "and it is reported, not left out — a tier that says nothing looks like a tier that \
         passed.\n{:?}",
        offline.summary
    );

    let seen = run(&world, INIT_WITH_SKILL);
    assert_eq!(
        seen.discovered,
        Discovery::Seen,
        "when the vendor lists the skill, Tier 3 says so. This is the direction that proves the \
         answer comes from `discovery_from_init` and is not the word `Unknown` written out by \
         hand"
    );
}
