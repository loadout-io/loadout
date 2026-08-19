//! AC-3 dla T-53: plik ustawień biegu piszemy **my** i jest w nim **jeden** klucz.
//!
//! Test spina obie połówki jednym ruchem: każe sterownikowi zapisać plik, buduje komendę tego
//! samego sterownika i czyta plik **ze ścieżki wziętej z argv** — nie ze ścieżki zwróconej
//! przez zapis. To jest o jedną asercję mniej i o jedno spięcie więcej: dopiero czytanie spod
//! ścieżki z argv wiąże „co obiecaliśmy procesowi" z „co naprawdę leży na dysku"
//! (niezmiennik 21 — plik ustawień biegu ma dokładnie jednego czytelnika, i jest nim proces,
//! który startujemy).
//!
//! # Dwie słabe wersje tego kryterium
//!
//! **Pierwsza jest po stronie dokumentu.**
//! `assert!(doc.get("permissions").and_then(|p| p.get("deny")).is_some())` przechodzi dla pliku,
//! który **oprócz** `deny` niesie `env` i `hooks` przepisane hurtem z gospodarza — czyli dla
//! dokładnie tego dokumentu, który przywraca maszynerię, po której pozbycie się to zadanie
//! istnieje. Rozróżniają to dwie asercje o **całych zbiorach kluczy**, na obu poziomach, plus
//! przemiatanie surowego tekstu: zagnieżdżony przemyt przechodzi każde sprawdzenie kluczy
//! najwyższego poziomu.
//!
//! **Druga jest po stronie argv.** `has_flag(&args, "--settings")` przechodzi dla sterownika,
//! który flagę stawia, a pliku nie pisze — a wtedy CLI umiera na brakującym pliku dopiero
//! w produkcji, przy starcie prawdziwego biegu.

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::engine::drivers::claude::{ClaudeDriver, RunSettings};
use loadout_lib::engine::drivers::{Policy, RunSpec};
use serde_json::Value;
use uuid::Uuid;

/// Dwie reguły ze znacznikiem osobliwym na tyle, żeby ich obecność w pliku nie dała się
/// wytłumaczyć zbiegiem okoliczności. Dwie, a nie jedna, bo kolejność listy odmów jest
/// asercją: przetasowana po drodze jest listą, której człowiek nie zweryfikuje spojrzeniem.
const DENY_FIRST: &str = "Read(LOADOUT-T53-DENY-MARKER-A/**)";
const DENY_SECOND: &str = "Read(LOADOUT-T53-DENY-MARKER-B/**)";

/// Cztery napisy, których w tym pliku nie ma prawa być **na żadnym poziomie zagnieżdżenia**.
///
/// Każdy z nich jest polem gospodarza, które nas **rozszerza**, a nie ogranicza: `allow` to
/// cudza polityka, `env` nadpisuje środowisko podane przez Loadouta i przewraca `env_clear()`
/// z niezmiennika 9 od zewnątrz, `sandbox` przepuszcza dowolną komendę mimo białej listy,
/// a `hooks` startuje procesy poza naszą grupą.
const NEVER_IN_THE_FILE: [&str; 4] = ["allow", "env", "sandbox", "hooks"];

/// `RunSpec` do zbudowania komendy. Polityka jest tu bez znaczenia — mierzą ją AC-1 i AC-2.
fn spec() -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: "rename the widget".to_owned(),
        model: None,
        system_append: None,
        policy: Policy::ReadOnly,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Wartość stojąca **zaraz za** flagą.
fn value_after<'a>(args: &[&'a OsStr], flag: &str) -> Option<&'a OsStr> {
    let at = args.iter().position(|arg| *arg == OsStr::new(flag))?;
    args.get(at + 1).copied()
}

/// Posortowane klucze obiektu JSON. `None`, kiedy to w ogóle nie jest obiekt.
fn keys(value: &Value) -> Option<Vec<&str>> {
    let mut names: Vec<&str> = value.as_object()?.keys().map(String::as_str).collect();
    names.sort_unstable();
    Some(names)
}

#[test]
fn the_run_settings_file_is_ours_and_carries_exactly_one_key() -> Result<(), Box<dyn Error>> {
    let run = tempfile::tempdir()?;
    let deny = vec![DENY_FIRST.to_owned(), DENY_SECOND.to_owned()];

    let settings = RunSettings::write(run.path(), &deny)?;
    let written = settings.path().to_path_buf();

    let command = ClaudeDriver::new().with_settings(settings).command(&spec());
    let args: Vec<&OsStr> = command.as_std().get_args().collect();

    // ── 1. Flaga stoi raz, niesie ŚCIEŻKĘ, i to ścieżkę tego pliku ────────────────────────
    let count = args
        .iter()
        .filter(|arg| **arg == OsStr::new("--settings"))
        .count();
    assert_eq!(
        count, 1,
        "--settings appears {count} time(s). It is the carrier of our rewritten deny list and \
         nothing else; twice means two documents and the CLI picking one. argv was {args:?}"
    );

    let value =
        value_after(&args, "--settings").ok_or("--settings was passed with nothing after it")?;
    assert!(
        !value.to_string_lossy().starts_with('{'),
        "--settings carries JSON inline instead of a path: {value:?}. The flag accepts both, and \
         the content in argv is readable by every user on this machine via ps - the same leak \
         invariant 9 names for the prompt, only harder to spot because nobody expects settings \
         to be secret"
    );

    let from_argv = Path::new(value);
    assert_eq!(
        from_argv,
        written.as_path(),
        "argv points the process at a different file than the one we wrote. Whatever is at \
         {written:?} then has no reader at all, and the isolation we promised is a file nobody \
         loads (invariant 21)"
    );

    // ── 2. Plik leży pod PODANYM katalogiem i naprawdę istnieje ───────────────────────────
    assert!(
        from_argv.starts_with(run.path()),
        "the settings file landed at {from_argv:?}, outside the directory it was handed \
         ({:?}). The driver does not pick its own place and does not write to $TMPDIR: run \
         artefacts live in the run directory (docs/ARCHITECTURE.md section 8)",
        run.path()
    );
    assert!(
        from_argv.is_file(),
        "argv names {from_argv:?} and there is no file there. A driver that raises the flag \
         without writing the file passes every check that only asks whether the flag is \
         present, and the CLI dies on the missing file in production, at the start of a real run"
    );

    // ── 3-6. Dokument, który naprawdę leży na dysku ───────────────────────────────────────
    let raw = fs::read_to_string(from_argv)?;
    let doc: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("the settings file is not valid JSON ({error}): {raw:?}"))?;

    let top = keys(&doc).ok_or("the settings file is not a JSON object at the top level")?;
    assert_eq!(
        top,
        vec!["permissions"],
        "the top level of our settings file carries {top:?}. Exactly one key, and the second one \
         is a new criterion rather than a patch: allow, env, sandbox and hooks do not enter from \
         our side either. The file was {raw:?}"
    );

    let permissions = doc
        .get("permissions")
        .ok_or("the settings file has no permissions object")?;
    let inner = keys(permissions).ok_or("permissions is not a JSON object")?;
    assert_eq!(
        inner,
        vec!["deny"],
        "permissions carries {inner:?}. Comparing whole key sets is the point: asking only \
         whether deny is there passes for a document that copied the host's permissions object \
         wholesale and put deny on top. The file was {raw:?}"
    );

    let rules: Vec<&str> = permissions
        .get("deny")
        .and_then(Value::as_array)
        .ok_or("permissions.deny is not an array")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        rules,
        vec![DENY_FIRST, DENY_SECOND],
        "the deny list came out as {rules:?}. It has to carry what it was handed, in the order \
         it was handed: a list of refusals reshuffled on the way is one no human can verify at \
         a glance"
    );

    for word in NEVER_IN_THE_FILE {
        assert!(
            !raw.contains(word),
            "the raw text of our settings file contains {word:?}. Sweeping the text, not the \
             top-level keys, is what catches nested smuggling - and every one of those four \
             fields WIDENS us: allow is somebody else's policy, env overrides the environment \
             Loadout passed and undoes env_clear() from the outside, sandbox lets any command \
             through despite the tool whitelist, hooks start processes outside our group. The \
             file was {raw:?}"
        );
    }

    Ok(())
}
