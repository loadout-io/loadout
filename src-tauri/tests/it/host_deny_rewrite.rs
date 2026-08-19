//! AC-4 dla T-53: z repo gospodarza bierzemy **tekst** `deny`, a cztery pozostałe pola
//! odrzucamy.
//!
//! Fikstura jest ulepiona z prawdziwego kształtu `.claude/settings.json` tego repo i niesie
//! **wszystkie pięć pól naraz**, każde z własnym znacznikiem. Test sam ją zapisuje, bo
//! kryterium mierzy zachowanie na cudzym pliku, a nie na naszym (niezmiennik 20).
//!
//! # Słaba wersja tego kryterium przechodzi dla implementacji, która przywraca haki
//!
//! `assert_eq!(rules, vec!["Read(HOST-DENY-MARKER/**)"])` samo w sobie dowodzi wyłącznie, że
//! `deny` zostało **wzięte**. Przechodzi je implementacja, która obok tego przenosi `env`,
//! `sandbox` i `hooks` **drugą drogą** — kopiując cały obiekt `permissions` albo cały plik
//! i dokładając `deny` na wierzch. To jest ta implementacja, która przywraca haki gospodarza
//! i `autoAllowBashIfSandboxed`, a jej test świeci na zielono.
//!
//! Rozróżniają to wyłącznie asercje **negatywne postawione na dokumencie, który naprawdę idzie
//! na dysk**. `assert!(result.env.is_none())` na jakiejś strukturze pośredniej nie liczy się:
//! przechodzi trywialnie dla struktury, która pola `env` w ogóle nie ma, podczas gdy droga
//! zapisu kopiuje surowy plik obok niej.

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::engine::drivers::claude::RunSettings;
use loadout_lib::engine::drivers::host::deny_rules;

/// Jedyna rzecz, która ma prawo przejść przez tę granicę.
const HOST_DENY_RULE: &str = "Read(HOST-DENY-MARKER/**)";

/// Sam znacznik reguły `deny` — po nim poznajemy go w surowym tekście naszego dokumentu.
const DENY_MARKER: &str = "HOST-DENY-MARKER";

/// Cudza lista auto-zatwierdzania. To nie jest nasza polityka: nasza mieszka w jednej tabeli
/// w adapterze (niezmiennik 23).
const ALLOW_MARKER: &str = "HOST-ALLOW-MARKER";

/// Blok `env` gospodarza **nadpisuje** środowisko podane przez Loadouta, czyli przewraca
/// `env_clear()` z niezmiennika 9 od zewnątrz.
const ENV_MARKER: &str = "HOST_ENV_MARKER";

/// `autoAllowBashIfSandboxed: true` przepuszcza **dowolną** komendę mimo naszej białej listy
/// narzędzi — pole, które nas ROZSZERZA.
const SANDBOX_MARKER: &str = "autoAllowBashIfSandboxed";

/// Hak gospodarza startuje proces w **swojej** grupie, jego dziecko dostaje `ppid=1`
/// i przeżywa wyjście `claude` [zmierzone 2026-08-19: 30 sierot].
const HOOK_MARKER: &str = "HOST-HOOK-MARKER";

/// Cudze ustawienia projektowe ze wszystkimi pięcioma polami naraz.
const HOST_SETTINGS: &str = r#"{
  "env": { "HOST_ENV_MARKER": "1" },
  "permissions": {
    "defaultMode": "acceptEdits",
    "allow": ["Bash(HOST-ALLOW-MARKER:*)"],
    "deny": ["Read(HOST-DENY-MARKER/**)"]
  },
  "sandbox": { "autoAllowBashIfSandboxed": true },
  "hooks": {
    "PreToolUse": [
      { "hooks": [{ "type": "command", "command": "HOST-HOOK-MARKER" }] }
    ]
  }
}"#;

/// Ten sam dokument z jednym przecinkiem za dużo. Repo gospodarza, którego nie kontrolujemy,
/// nie ma prawa zatrzymać naszego biegu.
const BROKEN_SETTINGS: &str = r#"{
  "permissions": { "deny": ["Read(HOST-DENY-MARKER/**)",] }
}"#;

/// Zakłada `<projekt>/.claude/settings.json` o podanej treści.
fn host_project(project: &Path, settings: &str) -> Result<(), Box<dyn Error>> {
    let dir = project.join(".claude");
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("settings.json"), settings)?;
    Ok(())
}

#[test]
fn only_the_deny_text_crosses_over_and_the_other_four_fields_are_dropped()
-> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    host_project(project.path(), HOST_SETTINGS)?;

    let rules = deny_rules(project.path());
    assert_eq!(
        rules,
        vec![HOST_DENY_RULE.to_owned()],
        "the whole vector, not contains: a rewrite that also carries the host's allow list, or \
         drops the rule it was supposed to keep, is invisible to a membership test. It came out \
         as {rules:?}"
    );

    // Druga połowa: dokument, który NAPRAWDĘ idzie na dysk. Struktura pośrednia nie liczy się -
    // asercja o polu, którego typ w ogóle nie ma, przechodzi trywialnie, podczas gdy droga
    // zapisu kopiuje surowy plik obok niej.
    let run = tempfile::tempdir()?;
    let settings = RunSettings::write(run.path(), &rules)?;
    let raw = fs::read_to_string(settings.path())?;

    // Ta jedna asercja stoi w drugą stronę i bez niej cztery nieobecności niżej spełnia
    // funkcja, która zwraca pustkę.
    assert!(
        raw.contains(DENY_MARKER),
        "the host's own deny rule never reached our settings file: {raw:?}. Rewriting is how \
         that rule comes back to us at all - the host's file is cut off whole by \
         --setting-sources with a zero-length argument, and that is the only lever that puts \
         out its hooks"
    );

    assert!(
        !raw.contains(ALLOW_MARKER),
        "the host's allow list reached our settings file: {raw:?}. Somebody else's \
         auto-approval list is not our policy; ours lives in one table in the adapter"
    );
    assert!(
        !raw.contains(ENV_MARKER),
        "the host's env block reached our settings file: {raw:?}. That block OVERRIDES the \
         environment Loadout passed, which undoes env_clear() from the outside - there is no \
         fixing it on our side, only not loading it"
    );
    assert!(
        !raw.contains(SANDBOX_MARKER),
        "the host's sandbox block reached our settings file: {raw:?}. \
         autoAllowBashIfSandboxed lets ANY command through despite our tool whitelist - it is a \
         field that widens us, which is why 'let us read his settings, he knows what he forbids \
         at home' is handing over the wheel rather than being careful"
    );
    assert!(
        !raw.contains(HOOK_MARKER),
        "the host's hook reached our settings file: {raw:?}. Its PreToolUse hook starts a \
         process in ITS OWN process group, that child gets ppid=1 and outlives the exit of \
         claude: the death proof from invariant 6 stays true and stops meaning anything. \
         Measured 2026-08-19: 14 orphans from one run, 30 across the experiments, each one \
         burning the rate limit in the background"
    );

    Ok(())
}

#[test]
fn a_project_that_never_met_claude_still_starts() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;

    // Bez `.claude/settings.json` w ogóle. Nie ma tu `Result` do rozpakowania i to jest część
    // kryterium: pusta lista jest ODPOWIEDZIĄ, nie awarią.
    let rules = deny_rules(project.path());
    assert!(
        rules.is_empty(),
        "a project with no .claude/settings.json produced {rules:?}. It has to produce nothing \
         and start anyway: a repo that has never seen Claude is a normal place to run a step"
    );

    Ok(())
}

#[test]
fn one_broken_comma_in_someone_elses_file_does_not_stop_our_run() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    host_project(project.path(), BROKEN_SETTINGS)?;

    let rules = deny_rules(project.path());
    assert!(
        rules.is_empty(),
        "an unparseable host settings file produced {rules:?}. It has to produce nothing and \
         let the run start: we do not control that repo, and one trailing comma in it must not \
         cost a step. Scraping the rule out of broken text is the other failure - what came \
         through would be whatever a regex happened to match"
    );

    Ok(())
}
