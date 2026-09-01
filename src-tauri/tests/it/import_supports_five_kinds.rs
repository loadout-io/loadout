//! Importer wnosi PIĘĆ rodzajów rzeczy, a plik, który nie jest żadną z nich, nie staje się
//! pozycją do rozstrzygnięcia.
//!
//! ZGŁOSZENIE WŁAŚCICIELA, 2026-08-29. Skan `~/Projects/meetnotes` postawił na ekranie
//! 86 wierszy, z czego 37 „do rozstrzygnięcia". Dziewiętnaście z tych wierszy to były pliki,
//! dla których Loadout nie ma ani sekcji, ani wykonawcy: `.claude/settings.json`
//! i `.codex/config.toml` (ustawienia cudzej aplikacji), `.claude/hooks/autoformat.sh`
//! (hak, którego nic tu nie uruchamia), `.claude/rules/*`, `AGENTS.md`, `CLAUDE.md`
//! oraz `.claude/lib/trace-span.sh` i `.agents/h/checks.json` wzięte za „workflow".
//! To ostatnie widać było wprost na ekranie: „The trace-span workflow leaves `jq -s .`
//! unresolved" — `jq -s .` jest linią w skrypcie, nie krokiem ceremonii, więc pytanie
//! nie miało odpowiedzi. Ten sam plik dostawał zresztą raz rodzaj Hook (`settings.json`),
//! a raz Rule (`settings.local.json`), co samo w sobie mówi, że rozpoznanie zgadywało
//! po nazwie pliku.
//!
//! WYBÓR WŁAŚCICIELA: wspieramy agenta, skilla, połączenie, notatkę i workflow, który
//! naprawdę jest workflowem. Reszta nie jest pozycją — nie dlatego, że jest nieważna,
//! tylko dlatego, że po stronie Loadouta nie ma jej gdzie postawić ani czym uruchomić,
//! a decyzja bez skutku jest gorsza niż jej brak.

use std::path::Path;

use loadout_lib::import::{ItemKind, translate};

/// Pliki, które mają się stać pozycjami, po jednym na każdy wspierany rodzaj.
const BROUGHT: [(&str, &str, ItemKind); 5] = [
    (
        ".claude/agents/reviewer.md",
        "Review the changes.",
        ItemKind::Agent,
    ),
    (
        ".claude/skills/ship/SKILL.md",
        "---\nname: ship\ndescription: Ship it.\n---\nShip it.",
        ItemKind::Skill,
    ),
    (
        ".mcp.json",
        "{\"mcpServers\":{\"context7\":{\"command\":\"npx\",\"args\":[\"context7\"]}}}",
        ItemKind::Connection,
    ),
    (
        ".claude/learnings/main-loop.md",
        "The main loop keeps one writer.",
        ItemKind::Memory,
    ),
    /* Prawdziwa ceremonia, a nie plik, który tylko leży w `commands/`: deklaruje pytanie,
     * na które Loadout ma kogo zapytać, więc daje się postawić jako workflow. */
    (
        ".claude/commands/learn.md",
        "Read the run, then write one note.\n\nquestion: `Which note should this run leave behind?`\n",
        ItemKind::Workflow,
    ),
];

/// Pliki, które przed tą zmianą stały na ekranie jako wiersze z decyzją, a nie mają
/// po stronie Loadouta ani sekcji, ani wykonawcy.
const LEFT_WHERE_THEY_ARE: [(&str, &str); 10] = [
    (".claude/rules/rust-tauri.md", "Never unwrap in production."),
    (".claude/settings.json", "{\"hooks\":{\"Stop\":[]}}"),
    (".claude/settings.local.json", "{\"permissions\":{}}"),
    (".claude/hooks/autoformat.sh", "#!/usr/bin/env bash\nfmt\n"),
    (
        ".claude/lib/trace-span.sh",
        "#!/usr/bin/env bash\njq -s .\n",
    ),
    (".agents/h/checks.json", "{\"checks\": {\"quick\": \"ci\"}}"),
    ("AGENTS.md", "This is the working charter."),
    ("CLAUDE.md", "Read AGENTS.md first."),
    (".codex/config.toml", "model = \"gpt-5\"\napproval = \"on\""),
    (".claude/future.json", "{\"new\":true}"),
];

fn project() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let repo = tempfile::tempdir()?;
    for (path, content, _) in BROUGHT {
        let file = repo.path().join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, content)?;
    }
    for (path, content) in LEFT_WHERE_THEY_ARE {
        let file = repo.path().join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, content)?;
    }
    Ok(repo)
}

#[test]
fn only_the_five_supported_kinds_become_items() -> Result<(), Box<dyn std::error::Error>> {
    let repo = project()?;
    let preview = translate::preview(repo.path())?;

    // Kontrola przeciw pustej asercji: skan MUSI coś znaleźć, inaczej „nie ma tam reguł"
    // byłoby zdaniem o pustym ekranie.
    assert_eq!(
        preview.snapshot.items.len(),
        BROUGHT.len(),
        "the scan should have found exactly the five supported files, and found {:?}",
        preview
            .snapshot
            .items
            .iter()
            .map(|item| item.path.display().to_string())
            .collect::<Vec<_>>()
    );

    for (path, _, kind) in BROUGHT {
        let Some(found) = preview
            .snapshot
            .items
            .iter()
            .find(|item| item.path == Path::new(path))
        else {
            return Err(format!("{path} stopped being brought over at all").into());
        };
        assert_eq!(
            found.kind, kind,
            "{path} was read as the wrong kind of thing"
        );
    }
    Ok(())
}

#[test]
fn a_file_loadout_cannot_place_is_not_a_decision() -> Result<(), Box<dyn std::error::Error>> {
    let repo = project()?;
    let preview = translate::preview(repo.path())?;

    for (path, _) in LEFT_WHERE_THEY_ARE {
        assert!(
            !preview
                .snapshot
                .items
                .iter()
                .any(|item| item.path == Path::new(path)),
            "{path} is still on the screen as something to decide about"
        );
        assert!(
            !preview
                .draft
                .items
                .iter()
                .flat_map(|item| &item.sources)
                .any(|source| source.path == Path::new(path)),
            "{path} is still part of the plan through another item"
        );
    }
    Ok(())
}

/// Projekt złożony wyłącznie ze wspieranych rzeczy importuje się bez ani jednego pytania.
///
/// To jest miara zgłoszenia z 2026-08-29 wprost: właściciel zobaczył 37 pozycji „do
/// rozstrzygnięcia", z których większość dotyczyła plików bez miejsca po tej stronie.
#[test]
fn a_project_made_only_of_supported_things_needs_no_decision()
-> Result<(), Box<dyn std::error::Error>> {
    let repo = project()?;
    let preview = translate::preview(repo.path())?;

    assert!(
        !preview.draft.items.is_empty(),
        "the plan is empty, so the sentence below would be about nothing"
    );
    for item in &preview.draft.items {
        assert_eq!(
            item.status,
            loadout_lib::import::ImportStatus::Ready,
            "{} still asks for a decision: {}",
            item.id,
            item.status_message
        );
    }
    Ok(())
}
