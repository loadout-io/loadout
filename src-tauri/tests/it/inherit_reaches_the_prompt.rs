//! AC-2 dla T-57: `## Recurring patterns` gospodarza dojeżdża do promptu kroku — i nic poza tą
//! sekcją, i **wyłącznie** promptem.
//!
//! T-54 dowiodło, że `scan::recurring_patterns` umie wyciąć sekcję (`inherit_recurring_patterns.rs`).
//! Ta funkcja nie miała ani jednego wołającego poza `tests/`. Tutaj pytanie brzmi inaczej: czy
//! wycięty tekst **wchodzi do kroku**, i czy wchodzi tą drogą, którą wolno.
//!
//! **Słabą wersją jest „prompt jest dłuższy niż bez dziedziczenia"** i mówi to wprost sam
//! kontrakt. Przechodzi dla implementacji, która dokleja CAŁY plik — czyli dla tej, której ten
//! mechanizm ma zapobiec: zmierzone u gospodarza 1701 z 32922 bajtów i 2016 z 73258, więc
//! „dłuższy" jest prawdą także wtedy, gdy do każdej tury każdego biegu jedzie 73 KB dziennika.
//! Rozróżnia to punkt (b) razem z (c).
//!
//! **Druga słaba wersja jest po stronie drogi.** `assert!(spec.prompt.contains(MARKER))`
//! przechodzi dla implementacji, która ten sam tekst wstawiła DODATKOWO do `system_append` —
//! a to pole staje się `--append-system-prompt`, czyli argumentem, który widzi `ps` każdego
//! użytkownika maszyny (niezmiennik 9). Dlatego (d) pyta o dwie rzeczy naraz: że `system_append`
//! wrócił nietknięty i że w CAŁYM argv nie ma ani jednego zdania z sekcji.
//!
//! **Trzecia słaba wersja jest najcichsza:** implementacja, która prompt kroku **zastępuje**
//! odziedziczonym tekstem, przechodzi (a), (b) i (c) naraz. Krok nie dostaje wtedy swojego
//! zadania i nikt tego nie zobaczy poza wynikiem, który „jakoś nie o to". Rozróżnia to asercja
//! o znaczniku promptu kroku.
//!
//! Fikstura niesie cytat blokowy z trzeciej linii prawdziwych plików ról — dosłownie
//! `` `## Recurring patterns` `` **przed** nagłówkiem — bo bez niego implementacja, która
//! przepisuje cięcie zamiast wołać `scan::recurring_patterns` (niezmiennik 23), wypada tak samo
//! jak poprawna. Na `backend-dev.md` gospodarza naiwne szukanie daje 131 bajtów zdania o tym, że
//! reguły są wiążące, zamiast 1701 bajtów reguł.
//!
//! JEDEN `#[test]`: zaślepka, która nie dokleja nic, przechodzi punkty (b), (c), (d) i (e) —
//! rozbite na osobne zestawy dałyby w warstwie `before` obraz „w połowie zielony". Przypadek
//! pozytywny stoi więc pierwszy.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{Policy, RunSpec};
use loadout_lib::inherit::wire::{self, Chosen};
use uuid::Uuid;

/// Znacznik z sekcji, która ma dojechać.
const PATTERNS_MARKER: &str = "PATTERNS-ONLY-3f81";
/// Znacznik z sekcji, która dojechać nie ma prawa.
const JOURNAL_MARKER: &str = "JOURNAL-ONLY-6c02";
/// Znacznik promptu kroku: bez niego „prompt zawiera reguły" jest prawdą także wtedy, gdy
/// zadanie kroku zostało tymi regułami **zastąpione**.
const STEP_MARKER: &str = "STEP-PROMPT-1d47";
/// Znacznik promptu systemowego agenta. Ma wrócić z `applied_to` nietknięty, co do bajtu.
const SYSTEM_MARKER: &str = "SYSTEM-APPEND-8b55";

/// Trzecia linia każdego z dziewięciu plików ról u gospodarza: cytat blokowy, w którym stoi
/// **dosłownie** `` `## Recurring patterns` `` — przed prawdziwym nagłówkiem [zmierzone 2026-08-19].
const QUOTE_ABOUT_THE_SECTION: &str = "> Auto-loaded by the orchestrator. `## Recurring patterns` is BINDING and the rest of this file is not.\n";

/// Prawdziwy nagłówek niesie przyrostek. Nagłówka równego dosłownie `## Recurring patterns` nie
/// ma w żadnym z dziesięciu plików gospodarza.
const REAL_HEADING: &str = "## Recurring patterns (BINDING — do NOT repeat)\n";

/// Jedno zdanie reguły — to ono ma dojechać do agenta.
const PATTERNS_RULE: &str = "A migration that drops a column is never additive.";

/// Jeden wiersz dziennika. Powtórzony, bo liczy się jego DŁUGOŚĆ: to on odpowiada za stosunek
/// 1701 do 32922 bajtów, po którym poznaje się wstrzykiwacz od wklejenia całego pliku.
const JOURNAL_LINE: &str = "2026-08-02 — one more entry that nobody reads twice, and that is exactly why the journal is the part which must never reach the prompt.\n";

/// Nazwa pliku roli u gospodarza, bez rozszerzenia.
const ROLE: &str = "backend-dev";

/// Umiejętność dla świata (e): bieg, który dziedziczy coś, ale nie ma skąd wziąć learnings.
const ALPHA_MD: &str = "---\nname: alpha\ndescription: Reads a log and says what broke.\n---\n\nStart from the first stack trace.\n";

/// Plik roli rozbity na części, żeby „dziennik jest wielokrotnie dłuższy" dało się sprawdzić,
/// a nie tylko zadeklarować.
struct Learnings {
    whole: String,
    patterns: String,
    journal: String,
}

fn learnings() -> Learnings {
    let patterns = format!(
        "\n- {PATTERNS_MARKER}: {PATTERNS_RULE}\n- A std mutex is never held across an await.\n\n"
    );
    let journal = format!(
        "\n{JOURNAL_MARKER} — 2026-08-01, task backend-11, three rounds.\n{}",
        JOURNAL_LINE.repeat(38)
    );
    let whole = format!(
        "# Learnings — {ROLE}\n\n{QUOTE_ABOUT_THE_SECTION}\n{REAL_HEADING}{patterns}## Run journal\n{journal}"
    );
    Learnings {
        whole,
        patterns,
        journal,
    }
}

/// Repozytorium gospodarza z plikiem learnings i katalog biegu pod nim; obok drugie
/// repozytorium, które learnings **nie ma wcale**.
struct World {
    /// Trzyma katalog tymczasowy przy życiu na czas testu; kasuje go `Drop`.
    _tmp: tempfile::TempDir,
    /// Repozytorium z `.claude/learnings/<rola>.md`.
    project: PathBuf,
    /// Katalog biegu w tym repozytorium.
    run: PathBuf,
    /// Repozytorium z umiejętnością, ale **bez** katalogu learnings.
    without_learnings: PathBuf,
    /// Katalog biegu w tamtym repozytorium.
    without_learnings_run: PathBuf,
}

fn world(file: &Learnings) -> World {
    let tmp = tempfile::tempdir().unwrap();

    let project = tmp.path().join("host-repo");
    let learnings = project.join(".claude").join("learnings");
    fs::create_dir_all(&learnings).unwrap();
    fs::write(learnings.join(format!("{ROLE}.md")), &file.whole).unwrap();

    let without_learnings = tmp.path().join("host-repo-without-learnings");
    let skills = without_learnings
        .join(".claude")
        .join("skills")
        .join("alpha");
    fs::create_dir_all(&skills).unwrap();
    fs::write(skills.join("SKILL.md"), ALPHA_MD).unwrap();

    World {
        run: runs_dir(&project).join("20260819T101500__r7"),
        without_learnings_run: runs_dir(&without_learnings).join("20260819T101501__r8"),
        _tmp: tmp,
        project,
        without_learnings,
    }
}

fn runs_dir(project: &Path) -> PathBuf {
    project.join(".loadout").join("runs")
}

/// Krok, do którego dziedziczenie ma się dopisać: własne zadanie i własny prompt systemowy.
fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: format!("{STEP_MARKER}: rename the widget and say what you changed."),
        model: None,
        // Prompt systemowy agenta — czyli KONFIGURACJA, nie treść. Stoi tu niepusty po to, żeby
        // punkt (d) miał co porównać: pole, które po dziedziczeniu ma być co do bajtu tym samym.
        system_append: Some(format!("{SYSTEM_MARKER}: answer in English.")),
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Całe argv jako jeden napis — do przemiatania. Pytamy o CAŁOŚĆ, nie o wybrane flagi: treść
/// przemycona jako wartość dowolnego argumentu jest tak samo widoczna w `ps`.
fn argv_text(command: &tokio::process::Command) -> String {
    command
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn only_the_patterns_section_reaches_the_prompt_and_it_reaches_nothing_else()
-> Result<(), Box<dyn Error>> {
    let file = learnings();

    // Straż nad samą fiksturą: bez wielokrotnie dłuższego dziennika próg 20% bajtów nie
    // rozróżnia niczego, bo „cały plik" mieści się wtedy w limicie i (c) przechodzi na pusto.
    assert!(
        file.journal.len() >= 10 * file.patterns.len(),
        "this fixture no longer models a real role file: the journal is {} bytes and the \
         patterns section {} — so the 20% threshold below would pass for pasting the whole file",
        file.journal.len(),
        file.patterns.len()
    );

    let world = world(&file);
    let chosen = Chosen {
        learnings: Some(ROLE.to_owned()),
        ..Chosen::default()
    };
    let inherited = wire::from_the_host(&world.project, &world.run, &chosen)?;

    let before = spec(&world.project);
    let after = inherited.applied_to(before.clone());

    // (a) Reguła gospodarza naprawdę dojechała.
    assert!(
        after.prompt.contains(PATTERNS_MARKER) && after.prompt.contains(PATTERNS_RULE),
        "the run's prompt carries no rule from the host's patterns section. The whole point of \
         this task is that the agent reads them; a mechanism nobody calls is the rot it was \
         written against. The prompt was {:?}",
        after.prompt
    );

    // …a zadanie kroku PRZEŻYŁO. Implementacja, która prompt zastępuje, przechodzi każdą
    // asercję niżej i oddaje agentowi cudze reguły zamiast jego roboty.
    assert!(
        after.prompt.contains(STEP_MARKER),
        "the step's own prompt is gone from {:?}: the inherited text replaced it instead of \
         joining it",
        after.prompt
    );

    // (b) Ani jednego zdania z dziennika. To jest ta połowa, która kosztuje: u gospodarza ta
    // sekcja dochodzi do 73 KB na jeden plik roli, w każdej turze, w każdym biegu.
    for forbidden in [JOURNAL_MARKER, JOURNAL_LINE.trim(), "## Run journal"] {
        assert!(
            !after.prompt.contains(forbidden),
            "{forbidden:?} reached the prompt. Everything past the patterns heading is the \
             journal, and pasting it is exactly the defect this mechanism exists to prevent: \
             measured on the host, 1701 of 32922 bytes are rules and the rest is diary"
        );
    }

    // (c) Budżet, liczony na bajtach, a nie na wrażeniu.
    let added = after.prompt.len() - before.prompt.len();
    let ceiling = file.whole.len() / 5;
    assert!(
        added < ceiling,
        "the inherited fragment is {added} bytes out of a {} byte file — over the 20% ceiling \
         ({ceiling}). An injector that carries a fifth of the file is a paste with extra steps",
        file.whole.len()
    );

    // (d) JEDNA DROGA. `system_append` wraca nietknięty…
    assert_eq!(
        after.system_append, before.system_append,
        "the inherited text moved into system_append. That field becomes \
         --append-system-prompt, so it is an argument, and arguments are readable by every user \
         on this machine through ps -- invariant 9 is about the content, not about the word \
         \"prompt\""
    );

    // …a w całym argv nie ma po niej ani śladu. Pytamy o argv zbudowane z TEGO kroku, bo tylko
    // ono odpowiada na pytanie, co naprawdę zobaczyłby `ps`.
    let command = ClaudeDriver::new()
        .with_inherited(inherited.flags().to_vec())
        .command(&after);
    let argv = argv_text(&command);
    for forbidden in [PATTERNS_MARKER, PATTERNS_RULE] {
        assert!(
            !argv.contains(forbidden),
            "{forbidden:?} stands in the command line: {argv:?}. The path of a plugin directory \
             may travel in argv; the text of somebody's rules may not"
        );
    }

    // (e) Gospodarz **bez** pliku learnings to prompt bez doklejki — nie błąd biegu. Repozytorium
    // bez tego pliku jest większością repozytoriów (niezmiennik 5), a bieg, który się na tym
    // przewraca, jest gorszy niż bieg bez dziedziczenia. Bierzemy tu skądinąd niepusty wybór,
    // żeby odpowiedź „prompt bez zmian" nie brała się po prostu z tego, że nic nie wybrano.
    let carries_a_skill = Chosen {
        skills: vec!["alpha".to_owned()],
        ..Chosen::default()
    };
    let without = wire::from_the_host(
        &world.without_learnings,
        &world.without_learnings_run,
        &carries_a_skill,
    )?;
    let untouched = spec(&world.without_learnings);
    let same = without.applied_to(untouched.clone());
    assert_eq!(
        same.prompt, untouched.prompt,
        "this project has no learnings file, and the prompt changed anyway. A heading over an \
         empty section teaches the model that the section is sometimes empty, and costs length \
         for nothing"
    );

    Ok(())
}
