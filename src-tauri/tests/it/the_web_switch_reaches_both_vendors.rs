//! „Może sięgnąć do internetu" jest JEDNYM wyborem agenta i działa u OBU vendorów.
//!
//! # Co to mierzy
//!
//! 2026-08-23, pytanie właściciela: „czemu dostępu do neta nie mają?". Zmierzone w jego
//! bibliotece: 18 agentów, ani jeden z siecią, i dwie różne przyczyny.
//!
//! * **Claude.** Sieć jest w `tools_for` dopiero przy `Unrestricted`, a `everything` znaczy „to,
//!   co daje dial". Furtka istniała — agent mógł WYPISAĆ `WebFetch`/`WebSearch` na swojej liście
//!   narzędzi — i nikt przez nią nie przeszedł, bo o niej nie wiedział. Kontrolka, do której nikt
//!   nie trafia, jest kontrolką, której nie ma.
//! * **Codex.** Furtki nie było WCALE. Sieć wisi u niego przy `workspace-write` jako
//!   `network_access`, a ta skrzynia nie wysyłała tego klucza ani razu — więc agent do researchu
//!   nie miał jak jej dostać inaczej niż przez `danger-full-access`, czyli zdejmując całą
//!   piaskownicę. Wybór między „widzi świat i może zepsuć wszystko" a „nie zepsuje niczego i nie
//!   widzi nic" jest dokładnie tym, co T-63 usunęło po stronie Claude'a.
//!
//! # SŁABĄ WERSJĄ jest sprawdzenie jednego vendora
//!
//! Przechodzi ją pole czytane wyłącznie przez adapter claude'owy — a wtedy przełącznik w
//! formularzu działa dla połowy agentów i milczy dla drugiej, co jest gorsze niż jego brak.
//! Dlatego sądzone są obie komendy, w jednym pliku.
//!
//! # I DRUGA STRONA, bez której to kryterium zazieleniłoby wpuszczenie sieci na stałe
//!
//! Agent BEZ tego wyboru dostaje dokładnie to, co dostawał przedtem: żadnego `WebFetch`, żadnego
//! `network_access`. Sieć włączona domyślnie jest zmianą uprawnień, o którą nikt nie prosił.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;

use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::build_exec_argv;
use loadout_lib::engine::drivers::{Policy, RunSpec};
use uuid::Uuid;

fn spec(policy: Policy, web: bool) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: "look it up".to_owned(),
        model: None,
        system_append: None,
        policy,
        reaches_the_web: web,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Argumenty komendy Claude'a. Wyrocznią jest ZBUDOWANA KOMENDA, nigdy plik adaptera
/// czytany z dysku (niezmiennik 20).
fn claude_argv(spec: &RunSpec) -> Vec<String> {
    ClaudeDriver::new()
        .command(spec)
        .as_std()
        .get_args()
        .map(OsStr::to_string_lossy)
        .map(std::borrow::Cow::into_owned)
        .collect()
}

/// Wartość podana po tej fladze — albo `None`, kiedy flagi nie ma.
fn after<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    let at = argv.iter().position(|one| one == flag)?;
    argv.get(at + 1).map(String::as_str)
}

#[test]
fn claude_gets_the_two_web_verbs_at_a_dial_that_touches_no_files() -> Result<(), Box<dyn Error>> {
    // `ReadOnly` z rozmysłem: to jest ten agent, dla którego cała ta furtka istnieje — research,
    // który nie ma prawa ruszyć ani jednego pliku.
    let argv = claude_argv(&spec(Policy::ReadOnly, true));
    let available = after(&argv, "--tools").ok_or("--tools is missing")?;
    let approved = after(&argv, "--allowedTools").ok_or("--allowedTools is missing")?;

    for name in ["WebFetch", "WebSearch"] {
        assert!(
            available.split(',').any(|one| one == name),
            "'{name}' is not in the tool set, so the agent cannot reach the internet at all. \
             Got: {available}"
        );
        assert!(
            approved.split(',').any(|one| one == name),
            "'{name}' is available and NOT approved. Under `--permission-mode dontAsk` nobody \
             answers the question, so from the outside this looks like a tool that always \
             refuses. Got: {approved}"
        );
    }
    Ok(())
}

#[test]
fn claude_still_touches_no_files_when_it_reaches_the_web() -> Result<(), Box<dyn Error>> {
    let argv = claude_argv(&spec(Policy::ReadOnly, true));
    let available = after(&argv, "--tools").ok_or("--tools is missing")?;

    for forbidden in ["Bash", "Write", "Edit"] {
        assert!(
            !available.split(',').any(|one| one.starts_with(forbidden)),
            "'look only' promises the person that this agent changes no files, and the web says \
             nothing about files. '{forbidden}' arriving through this door would make the switch \
             a second road to permissions beside the dial. Got: {available}"
        );
    }
    Ok(())
}

#[test]
fn codex_gets_network_access_in_its_sandbox() {
    let argv = build_exec_argv(&spec(Policy::EditInFolder, true));

    assert!(
        argv.iter()
            .any(|one| one == "sandbox_workspace_write.network_access=true"),
        "Codex has no tool list — the internet is a setting of its sandbox, off by default. \
         Without this key the research agent has no way to reach the web short of \
         `danger-full-access`, which also hands it the whole disk. Got: {argv:?}"
    );
    assert_eq!(
        after(&argv, "-s"),
        Some("workspace-write"),
        "and the dial itself must not move. Opening the web by raising the sandbox is exactly \
         the trade this switch exists to remove"
    );
}

#[test]
fn an_agent_without_the_switch_gets_what_it_always_got() -> Result<(), Box<dyn Error>> {
    let claude = claude_argv(&spec(Policy::ReadOnly, false));
    let available = after(&claude, "--tools").ok_or("--tools is missing")?;
    assert!(
        !available.contains("Web"),
        "the web arrived without anybody asking for it. Switched on by default it is a change of \
         permissions nobody chose. Got: {available}"
    );

    let codex = build_exec_argv(&spec(Policy::EditInFolder, false));
    assert!(
        !codex.iter().any(|one| one.contains("network_access")),
        "same on the other side: the sandbox stays exactly as it was. Got: {codex:?}"
    );
    Ok(())
}
