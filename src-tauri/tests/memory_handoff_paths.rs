//! AC-6 dla T-16: nazwa pliku jest funkcją Loadouta i nie da się nią wyjść z katalogu.
//!
//! Wzór to `handoffs/<NN>__<from>__<kind>.md` [ARCHITECTURE §8]. `from` bywa czymkolwiek —
//! nazwą agenta z pliku workflow, tekstem wklejonym przez człowieka, wartością z gałęzi, której
//! nikt nie przewidział. Nazwa pliku składana z takiego wejścia jest zapisem na ścieżce, którą
//! podał ktoś inny.
//!
//! **Słabą wersją tego kryterium jest `from.replace("../", "")`.** Przechodzi na
//! `../../etc/passwd` i **pada** na `....//x`, bo po skasowaniu obu `../` z `....//` zostaje
//! `../`. Równie słabe jest `path.starts_with(handoffs)`: porównuje tekst, więc przechodzi na
//! ścieżce z `..` w środku i na dowiązaniu.
//!
//! Rozróżnia `fs::canonicalize` **obu stron** dla wszystkich pięciu wejść — pytanie brzmi
//! „gdzie ten plik naprawdę leży", a na nie odpowiada tylko system plików. Drugim rozróżnieniem
//! jest kolizja: dwa razy ta sama trójka (krok, slug, kind) nie ma prawa nadpisać pierwszego
//! pliku, bo to jest cichy sposób na skasowanie przekazania, które ktoś już przeczytał.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use loadout_lib::memory::handoff::{self, Kind, MetaDraft};

/// Wejścia wrogie w polu `from`, każde z własnym numerem kroku — dzięki temu żadne dwa nie
/// kolidują nazwą i każde odpowiada wyłącznie za swoje pytanie.
const HOSTILE: [(&str, u32); 5] = [
    // Klasyka: wyjście dwa katalogi w górę.
    ("../../etc/passwd", 1),
    // To samo, napisane tak, żeby przeżyć naiwne kasowanie `../`.
    ("....//x", 2),
    // Ścieżka bezwzględna. `PathBuf::join` z argumentem bezwzględnym **zastępuje** całą
    // dotychczasową ścieżkę — tu przewraca się implementacja, która nigdy nie widziała `..`.
    ("/absolute/x", 3),
    // Nazwa spoza ASCII. Nie jest atakiem, jest codziennością, i też musi dać slug.
    ("Ünïcode Agent", 4),
    // Same białe znaki: z tego nie zostaje ani jeden dozwolony znak.
    ("   ", 5),
];

/// Wejście, po którym slug musi zdegradować się do stałej. `01____brief.md` z pustym członem
/// nie daje się odczytać z powrotem na trzy pola, więc pusty slug jest niedopuszczalny.
const EMPTY_INPUT: &str = "   ";
const EMPTY_SLUG: &str = "agent";

/// Rodzaj używany w całym pliku — jego nazwa jest trzecim członem nazwy pliku.
const KIND_NAME: &str = "brief";

const BODY: &str = "\
## Answer
Two researchers, then a planner.

## Evidence
- tasks/T-16.md

## Open
- none
";

fn draft(from: &str, step: u32) -> MetaDraft {
    MetaDraft {
        run: "run_7f3a".to_owned(),
        step,
        from: from.to_owned(),
        to: vec!["planner".to_owned()],
        kind: Kind::Brief,
        title: "What we are building".to_owned(),
        reads: vec![],
    }
}

/// `^[a-z0-9]+(-[a-z0-9]+)*$`, napisane ręcznie, bo `regex` nie jest zależnością tego repo,
/// a `src-tauri/Cargo.toml` nie należy do T-16 (AGENTS.md §7).
fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// Rozbiera nazwę pliku na `(NN, slug, kind)`.
fn parts(path: &Path) -> (String, String, String) {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let stem = name.strip_suffix(".md").unwrap_or_default();
    assert!(
        !stem.is_empty(),
        "a handoff is a markdown file; this one is named {name:?}"
    );

    let fields: Vec<&str> = stem.split("__").collect();
    assert_eq!(
        fields.len(),
        3,
        "the name is `<NN>__<from>__<kind>.md` [ARCHITECTURE §8] — three fields, so the run \
         directory reads back into steps and authors without a database. This one is {name:?}"
    );
    (
        fields[0].to_owned(),
        fields[1].to_owned(),
        fields[2].to_owned(),
    )
}

#[test]
fn every_hostile_name_lands_inside_the_handoffs_directory() {
    let run_dir = tempfile::tempdir().unwrap();

    for (from, step) in HOSTILE {
        let written = handoff::write_handoff(run_dir.path(), draft(from, step), BODY)
            .unwrap_or_else(|error| {
                // Zapis, który się nie udał, jest tu tak samo interesujący jak zapis w złym
                // miejscu: `handoffs.join("/absolute/x")` daje `/absolute/x`, a tam nie ma
                // katalogu, więc `write_handoff` zwraca błąd zamiast zapisać cokolwiek.
                unreachable!("write_handoff failed for from = {from:?}: {error}")
            });

        assert!(
            written.path.is_file(),
            "from = {from:?} reported {} and there is no file there",
            written.path.display()
        );

        let landed = std::fs::canonicalize(written.path.parent().unwrap_or(run_dir.path()))
            .expect("the directory the handoff was written into does not resolve");
        let expected = std::fs::canonicalize(run_dir.path().join("handoffs"))
            .expect("the run directory has no `handoffs/`");
        assert_eq!(
            landed,
            expected,
            "from = {from:?} put the file in {}, and the run's handoffs live in {}. Comparing \
             the resolved paths is the whole assertion: a text comparison passes on a path with \
             `..` in the middle and on a symlink",
            landed.display(),
            expected.display()
        );

        let (number, slug, kind) = parts(&written.path);
        assert_eq!(
            number,
            format!("{step:02}"),
            "the first field is the step number, two digits with a leading zero \
             [ARCHITECTURE §8], and from = {from:?} at step {step} produced {number:?}"
        );
        assert_eq!(
            kind, KIND_NAME,
            "the third field is the kind, and from = {from:?} produced {kind:?}"
        );
        assert!(
            is_slug(&slug),
            "from = {from:?} produced the slug {slug:?}, which is not \
             `^[a-z0-9]+(-[a-z0-9]+)*$`. Everything outside that alphabet is either a path \
             separator, a shell character or something a person cannot type"
        );

        if from == EMPTY_INPUT {
            assert_eq!(
                slug, EMPTY_SLUG,
                "nothing survives slugifying {from:?}, and an empty middle field gives \
                 `{step:02}____{KIND_NAME}.md` — a name that no longer reads back into three \
                 fields. It degrades to `{EMPTY_SLUG}` instead"
            );
        }
    }
}

#[test]
fn the_same_step_twice_never_overwrites_the_first_file() {
    let run_dir = tempfile::tempdir().unwrap();

    let first = handoff::write_handoff(run_dir.path(), draft("orchestrator", 7), BODY)
        .expect("write_handoff refused an ordinary name");
    let before = std::fs::read(&first.path).unwrap();
    let first_stem = first
        .path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_owned();

    let second = handoff::write_handoff(run_dir.path(), draft("orchestrator", 7), BODY)
        .expect("write_handoff refused the second write instead of giving it its own name");

    assert_ne!(
        second.path, first.path,
        "the same step, agent and kind twice is a retry, not a correction, and the second write \
         landed on the first one's path. A handoff somebody has already read cannot be replaced \
         by a later write of the same name"
    );
    assert_eq!(
        second
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default(),
        format!("{first_stem}-2.md"),
        "the collision is resolved with a `-2` suffix on the name, so the run directory still \
         sorts by step and still says who wrote what"
    );

    let after = std::fs::read(&first.path).unwrap();
    assert!(
        after == before,
        "the first file changed while the second was being written. It held {} bytes and now \
         holds {}",
        before.len(),
        after.len()
    );

    let landed = std::fs::canonicalize(second.path.parent().unwrap_or(run_dir.path())).unwrap();
    let expected = std::fs::canonicalize(run_dir.path().join("handoffs")).unwrap();
    assert_eq!(
        landed, expected,
        "the renamed file stays in `handoffs/` like every other one"
    );
}
