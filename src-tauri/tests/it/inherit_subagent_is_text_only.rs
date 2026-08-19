//! AC-3 dla T-57: podagent gospodarza wchodzi do biegu jako **tekst**, a jego front-matter nie
//! wchodzi nigdzie — ani do promptu, ani do argv, ani do pliku ustawień biegu.
//!
//! T-54 dowiodło, że `scan::agent_body` odcina front-matter (`inherit_agents_are_text.rs`).
//! Tamto kryterium sądzi funkcję nad napisem. To sądzi **bieg**: co z cudzego pliku naprawdę
//! przekracza granicę i którymi drogami.
//!
//! **Słabą wersją jest `assert!(!prompt.contains("mcpServers"))`** i mówi to wprost sam
//! kontrakt. Przechodzi dla implementacji, która zjada nazwę pola i przepuszcza jego **wcięte
//! dzieci** — a to one uruchamiają proces: `command: npx`, `args: ["-y", "@playwright/mcp@…"]`.
//! Rozróżnia to asercja o KOMENDZIE, nie o nazwie pola.
//!
//! DLACZEGO akurat `mcpServers` jest tu najgroźniejsze: uruchamia proces **poza grupą procesów
//! Loadouta**, a niezmiennik 6 wymaga dowodu śmierci grupy, której nie założyliśmy. Krok się
//! kończy, `kill(-pgid, 0)` oddaje `ESRCH`, dowód jest prawdziwy — i nie dotyczy tamtego
//! procesu. Zmierzone 2026-08-19: hak gospodarza zostawił **14 sierot z `ppid=1`**, które
//! przeżyły wyjście `claude`, a eksperymenty łącznie 30. Osierocony proces pali limit
//! u dostawcy tak długo, jak długo nikt nie patrzy; to jest błąd finansowy, nie higieniczny.
//! Pozostałe cztery pola są z tej samej rodziny: `tools` i `permissionMode` przepisują politykę
//! biegu z miejsca, którego nasze UI nie pokazuje, `memory` wskazuje cudzy katalog pamięci,
//! a `model` po cichu zmienia rachunek.
//!
//! CZEGO NIE DA SIĘ ZAPYTAĆ O ARGV, i mówię to wprost, bo kryterium niespełnialne jest gorsze
//! niż jego brak: nasze własne flagi brzmią `--tools` i `--model`, więc zakaz **napisu**
//! „tools" w argv nie przechodzi dla żadnej poprawnej implementacji. W argv pytamy więc o
//! WARTOŚCI z front-mattera (każda niesie własny znacznik) i o dwie nazwy pól, które w naszym
//! argv nie występują nigdy: `mcpServers` i `permissionMode` (nasza flaga to
//! `--permission-mode`). O nazwy wszystkich pięciu pól pytamy tam, gdzie pytanie ma sens —
//! w prompcie.
//!
//! JEDEN `#[test]`: zaślepka, która nie dokleja nic, przechodzi wszystkie asercje negatywne —
//! rozbite na osobne zestawy dałyby w warstwie `before` obraz „w połowie zielony". Przypadek
//! pozytywny stoi więc pierwszy, a punkt (d) pilnuje, żeby fikstura naprawdę niosła to, czego
//! reszta zabrania.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::engine::drivers::claude::{ClaudeDriver, RunSettings};
use loadout_lib::engine::drivers::{Policy, RunSpec};
use loadout_lib::inherit::scan;
use loadout_lib::inherit::wire::{self, Chosen};
use uuid::Uuid;

/// Znacznik ciała — jedyna część tego pliku, która ma przekroczyć granicę.
const BODY_MARKER: &str = "BODY-ONLY-2e93";
/// Znacznik zadania kroku: prompt kroku ma przeżyć doklejenie.
const STEP_MARKER: &str = "STEP-PROMPT-9a02";

/// Rola gospodarza, po nazwie pliku bez rozszerzenia.
const ROLE: &str = "release-engineer";

/// Odwzorowanie `.claude/agents/*.md` gospodarza: pięć pól, które przenoszą **maszynerię**,
/// w tym `mcpServers` z zagnieżdżonym `command` i `args` [zmierzone 2026-08-19, trzy pliki na
/// trzynaście]. Wartości niosą znaczniki, bo o nie — nie o nazwy pól — pytamy w argv.
const AGENT_MD: &str = "\
---
name: release-engineer
description: Cuts the release notes.
tools: Read, Write, Bash(LOADOUT-T57-TOOL-c41)
model: LOADOUT-T57-MODEL-c41
permissionMode: bypassPermissions
memory: ../../.claude/notes/LOADOUT-T57-MEMORY-c41
mcpServers:
  playwright:
    command: npx
    args: [\"-y\", \"@playwright/mcp@0.0.75\"]
---

# Release engineer

BODY-ONLY-2e93 — cut the notes from the merged pull requests, newest first.

Say which pull request you could not classify, instead of guessing.
";

/// Pięć nazw pól, wypisanych literalnie — nie wziętych ze stałej implementacji. Kryterium
/// sprawdzające implementację jej własną tablicą przechodzi po każdej zmianie tej tablicy,
/// łącznie z literówką.
const MACHINERY_FIELDS: [&str; 5] = ["tools", "model", "permissionMode", "memory", "mcpServers"];

/// To, czego w argv i w plikach biegu nie ma prawa być: znaczniki wartości plus dwie nazwy pól,
/// które w naszym argv nie występują nigdy.
const MACHINERY_VALUES: [&str; 7] = [
    "LOADOUT-T57-TOOL-c41",
    "LOADOUT-T57-MODEL-c41",
    "LOADOUT-T57-MEMORY-c41",
    "mcpServers",
    "permissionMode",
    "command: npx",
    "@playwright/mcp@0.0.75",
];

/// Reguła odmowy przepisana z gospodarza (T-53). Stoi tu po to, żeby plik ustawień biegu
/// naprawdę powstał i żeby miało co przemiatać punkt (b).
const DENY_RULE: &str = "Read(secrets/**)";

/// Repozytorium gospodarza z podagentem i katalog biegu pod nim.
struct World {
    /// Trzyma katalog tymczasowy przy życiu na czas testu; kasuje go `Drop`.
    _tmp: tempfile::TempDir,
    project: PathBuf,
    run: PathBuf,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("host-repo");
    let agents = project.join(".claude").join("agents");
    fs::create_dir_all(&agents).unwrap();
    fs::write(agents.join(format!("{ROLE}.md")), AGENT_MD).unwrap();

    // Katalog biegu istnieje, zanim cokolwiek się zacznie — tak samo jak w biegu, gdzie zakłada
    // go `lay_out_the_run_dir`, zanim ruszy pierwszy proces.
    let run = project
        .join(".loadout")
        .join("runs")
        .join("20260819T101500__r7");
    fs::create_dir_all(&run).unwrap();

    World {
        _tmp: tmp,
        project,
        run,
    }
}

/// Krok, do którego podagent ma się dopisać.
fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: format!("{STEP_MARKER}: write the release notes for 2.1.233."),
        model: None,
        system_append: Some("Answer in English.".to_owned()),
        // `ReadOnly` świadomie: `bypassPermissions` z fikstury jest zarazem legalną wartością
        // naszej własnej flagi `--permission-mode`, więc przemiatanie argv po tym napisie
        // sądziłoby politykę biegu zamiast dziedziczenia.
        policy: Policy::ReadOnly,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Całe argv jako jeden napis — treść przemycona jako wartość dowolnego argumentu jest tak samo
/// widoczna w `ps` jak ta stojąca za flagą, którą akurat podejrzewamy.
fn argv_text(command: &tokio::process::Command) -> String {
    command
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Wszystko, co bieg zapisał: ścieżka i bajty każdego pliku pod katalogiem biegu.
fn files_under(root: &Path) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    walk(root, &mut found);
    found.sort_by(|left, right| left.0.cmp(&right.0));
    found
}

fn walk(dir: &Path, found: &mut Vec<(PathBuf, String)>) {
    let Ok(listing) = fs::read_dir(dir) else {
        return;
    };
    for entry in listing {
        let entry = entry.unwrap();
        let path = entry.path();
        if fs::symlink_metadata(&path).unwrap().is_dir() {
            walk(&path, found);
        } else {
            found.push((
                path.clone(),
                String::from_utf8_lossy(&fs::read(&path).unwrap()).into_owned(),
            ));
        }
    }
}

#[test]
fn the_subagents_body_crosses_the_boundary_and_its_front_matter_never_does()
-> Result<(), Box<dyn Error>> {
    let world = world();

    // (d) KONTROLA PRZECIW PUSTEMU CZYTANIU. Bez tych dwóch pętli cały ten test mierzyłby plik
    // bez front-mattera i przechodził na niczym — każda asercja negatywna niżej jest wtedy
    // prawdziwa z powodu, który nie ma nic wspólnego z implementacją.
    //
    // Front-matter to wszystko między pierwszym a drugim `---`; ciało tego pliku żadnej takiej
    // kreski nie ma, więc podział jest jednoznaczny.
    let front_matter = AGENT_MD
        .split("---")
        .nth(1)
        .ok_or("the fixture has no front matter at all")?;
    for field in MACHINERY_FIELDS {
        assert!(
            front_matter.contains(field),
            "the fixture's front matter no longer carries `{field}`, so this criterion would \
             pass by measuring a file that has nothing to strip"
        );
    }
    for value in MACHINERY_VALUES {
        assert!(
            front_matter.contains(value),
            "the fixture's front matter no longer carries {value:?}, so sweeping for it proves \
             nothing"
        );
    }

    let chosen = Chosen {
        subagent: Some(ROLE.to_owned()),
        ..Chosen::default()
    };
    let inherited = wire::from_the_host(&world.project, &world.run, &chosen)?;

    let before = spec(&world.project);
    let after = inherited.applied_to(before.clone());

    // (a) CIAŁO, i to dokładnie to ciało, które oddaje `scan::agent_body` — czyli wszystko za
    // drugim `---`. Pytamy wiersz po wierszu, bo pytanie o jeden znacznik przechodzi dla
    // implementacji, która przywiozła pierwsze zdanie i zgubiła resztę.
    let body = scan::agent_body(AGENT_MD);
    assert!(
        body.contains(BODY_MARKER)
            && body.lines().filter(|line| !line.trim().is_empty()).count() >= 3,
        "the fixture's body is not what this test assumes ({body:?}), so the assertions below \
         would prove nothing"
    );
    for line in body.lines().filter(|line| !line.trim().is_empty()) {
        assert!(
            after.prompt.contains(line.trim()),
            "the line {:?} of the host's subagent never reached the prompt. The body is the \
             whole point: it is the part a person wrote for an agent, and it is the only part we \
             are allowed to carry. The prompt was {:?}",
            line.trim(),
            after.prompt
        );
    }
    assert!(
        after.prompt.contains(STEP_MARKER),
        "the step's own prompt is gone from {:?}: the inherited text replaced it instead of \
         joining it",
        after.prompt
    );

    // (b) NAZWY PÓL — w prompcie. Osobna asercja na pole, bo komunikat porażki ma powiedzieć,
    // KTÓRE przeszło; jedna wspólna mówi tylko, że coś jest nie tak.
    for field in MACHINERY_FIELDS {
        assert!(
            !after.prompt.contains(field),
            "`{field}` is in the text we would send to the model. Front matter is the boundary \
             of machinery, and we cut the block rather than filtering fields: a blacklist is \
             incomplete by definition and breaks quietly at the vendor's next release. The \
             prompt was {:?}",
            after.prompt
        );
    }

    // …i WARTOŚCI — w argv oraz w każdym pliku, który ten bieg zostawił po sobie, łącznie
    // z plikiem ustawień biegu. Wartość jest tu ostrzejszym pytaniem niż nazwa: implementacja,
    // która zjada wiersz `mcpServers:` i zostawia jego wcięte dzieci, przechodzi każdy test
    // pytający o nazwę pola — a to dzieci uruchamiają proces.
    let settings = RunSettings::write(&world.run, &[DENY_RULE.to_owned()])?;
    let command = ClaudeDriver::new()
        .with_inherited(inherited.flags().to_vec())
        .with_settings(settings)
        .command(&after);
    let argv = argv_text(&command);
    let written = files_under(&world.run);
    assert!(
        !written.is_empty(),
        "the run directory holds no file at all, so sweeping it proves nothing"
    );

    for value in MACHINERY_VALUES {
        assert!(
            !argv.contains(value),
            "{value:?} stands in the command line: {argv:?}. Front matter is machinery, and \
             machinery does not cross this boundary in any direction -- not as text, not as an \
             argument, not as a settings key"
        );
        for (path, text) in &written {
            assert!(
                !text.contains(value),
                "{value:?} was written into {path:?}. The run's own files are ours; the host's \
                 front matter has no business in any of them"
            );
        }
    }

    // (c) KOMENDA, nie nazwa pola. `mcpServers` uruchamia proces poza grupą procesów Loadouta,
    // więc nie wchodzi ani do dowodu śmierci grupy (niezmiennik 6), ani do żadnego licznika
    // kosztu: zmierzone 2026-08-19, hak gospodarza zostawił 14 sierot z `ppid=1`, które
    // przeżyły wyjście `claude`.
    for command_line in ["npx", "@playwright/mcp@0.0.75"] {
        assert!(
            !after.prompt.contains(command_line),
            "{command_line:?} reached the prompt. Asking only about the field name passes for an \
             implementation that eats `mcpServers:` and keeps its indented children -- and the \
             children are what starts a process outside our process group. The prompt was {:?}",
            after.prompt
        );
    }

    Ok(())
}
