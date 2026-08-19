//! AC-1 dla T-57: katalog pluginu powstaje **w katalogu biegu**, a jego ścieżka wchodzi do argv
//! sterownika — dokładnie raz i tylko wtedy, gdy jest co odziedziczyć.
//!
//! To kryterium sądzi SZEW, a nie kompozytor. T-54 dowiodło, że `rewrite::plugin_dir` umie
//! zapisać katalog i że `rewrite::plugin_argv` umie zbudować dwuelementowy fragment
//! (`inherit_plugin_dir.rs`, `inherit_argv_plugin.rs`). Obie te funkcje były wołane **wyłącznie
//! z `tests/`** — czyli ze skrzyń, w których `dead_code` milczy, bo testy integracyjne są
//! osobnymi skrzyniami. Tutaj pytanie brzmi inaczej: czy to, co one produkują, **dojeżdża do
//! komendy, którą naprawdę uruchamiamy**.
//!
//! **Słabą wersją tego kryterium jest `assert!(argv.contains(&"--plugin-dir".to_owned()))`**
//! i mówi to wprost sam kontrakt. Przechodzi dla sterownika, który dokłada flagę ZAWSZE — a
//! wtedy bieg bez ani jednej odziedziczonej umiejętności podaje vendorowi ścieżkę do katalogu,
//! którego nie ma, i to jest awaria startu procesu, nie brak funkcji. Rozróżnia to punkt (d),
//! i dlatego stoi w tym samym teście, a nie „gdzieś obok".
//!
//! **Drugą słabą wersją jest porównanie ścieżki z argv z tą, którą test sam podał.** Przechodzi
//! dla implementacji, która katalog pluginu zakłada w `$TMPDIR` albo w `.claude/` gospodarza,
//! a w argv wpisuje coś innego. Rozróżnia to czytanie `SKILL.md` **spod ścieżki wziętej z
//! argv**: dopiero to wiąże „co obiecaliśmy procesowi" z „co naprawdę leży na dysku".
//!
//! O PUNKCIE (a) I `$TMPDIR`, żeby nie było nieporozumienia: cała fikstura żyje w katalogu
//! tymczasowym, więc pytanie „czy ta ścieżka jest pod `$TMPDIR`" nie rozróżnia tu niczego.
//! Operatywne pytanie brzmi „czy jest pod katalogiem TEGO biegu", i ono wyklucza obie wady
//! naraz: katalog wybrany przez sterownika samodzielnie (`$TMPDIR`) i katalog dopisany do
//! cudzego `.claude/`, do którego nie wolno nam napisać ani bajtu.
//!
//! JEDEN `#[test]`: zaślepka, która nigdy nie wypisuje flagi, przechodzi punkt (d) — rozbity na
//! osobne zestawy dałby w warstwie `before` obraz „w połowie zielony". Przypadek pozytywny stoi
//! więc pierwszy.

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

/// Umiejętności gospodarza. Treść ma znaczenie tylko dla porównania bajt w bajt — i dlatego
/// każda niesie coś, czego nasz emiter nie napisałby sam (`argument-hint`, kolejność pól).
const ALPHA_MD: &str = "---\nname: alpha\ndescription: Reads a log and says what broke.\nargument-hint: <path>\n---\n\nStart from the first stack trace.\n";
const BETA_MD: &str = "---\nname: beta\ndescription: Turns a failing gate into one sentence.\n---\n\nQuote the first failing assertion, not the summary line.\n";
const GAMMA_MD: &str =
    "---\nname: gamma\ndescription: The one nobody picked.\n---\n\nThis file stays at home.\n";

/// Podagent gospodarza w repozytorium **bez** umiejętności. Stoi tu po to, żeby punkt (d) mierzył
/// bieg, który NAPRAWDĘ coś dziedziczy — inaczej „nie ma flagi" jest prawdą o pustym biegu, a nie
/// o biegu bez umiejętności.
const RELEASE_ENGINEER_MD: &str =
    "---\nname: release-engineer\n---\n\nCut the release notes from the merged pull requests.\n";

/// Katalog biegu w kształcie, który buduje `commands::run` (`docs/ARCHITECTURE.md` §8).
const RUN_WITH_SKILLS: &str = "20260819T101500__r7";
const RUN_WITHOUT_SKILLS: &str = "20260819T101501__r8";

/// Repozytorium gospodarza **jest** folderem projektu biegu: człowiek otwiera cudze repo i to
/// w nim pracują agenci. Katalog biegu leży pod nim, w `.loadout/runs/`.
struct World {
    /// Trzyma katalog tymczasowy przy życiu na czas testu; kasuje go `Drop`.
    _tmp: tempfile::TempDir,
    /// Repozytorium z trzema umiejętnościami.
    project: PathBuf,
    /// Katalog biegu w tym repozytorium. **Nie istnieje** przed wywołaniem.
    run: PathBuf,
    /// Repozytorium z `.claude/agents`, ale **bez** `.claude/skills`.
    bare: PathBuf,
    /// Katalog biegu w tamtym repozytorium.
    bare_run: PathBuf,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();

    let project = tmp.path().join("host-repo");
    let skills = project.join(".claude").join("skills");
    for (name, text) in [("alpha", ALPHA_MD), ("beta", BETA_MD), ("gamma", GAMMA_MD)] {
        fs::create_dir_all(skills.join(name)).unwrap();
        fs::write(skills.join(name).join("SKILL.md"), text).unwrap();
    }

    let bare = tmp.path().join("host-repo-without-skills");
    let agents = bare.join(".claude").join("agents");
    fs::create_dir_all(&agents).unwrap();
    fs::write(agents.join("release-engineer.md"), RELEASE_ENGINEER_MD).unwrap();

    World {
        run: runs_dir(&project).join(RUN_WITH_SKILLS),
        bare_run: runs_dir(&bare).join(RUN_WITHOUT_SKILLS),
        _tmp: tmp,
        project,
        bare,
    }
}

fn runs_dir(project: &Path) -> PathBuf {
    project.join(".loadout").join("runs")
}

/// `RunSpec` do zbudowania komendy. Polityka i model są tu bez znaczenia — mierzy je T-53.
fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "rename the widget".to_owned(),
        model: None,
        system_append: None,
        policy: Policy::ReadOnly,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Argumenty komendy, tak jak zobaczy je proces.
fn argv_of(command: &tokio::process::Command) -> Vec<PathBuf> {
    command
        .as_std()
        .get_args()
        .map(PathBuf::from)
        .collect::<Vec<_>>()
}

/// Ile razy ta flaga stoi w argv.
fn times(argv: &[PathBuf], flag: &str) -> usize {
    argv.iter().filter(|arg| *arg == Path::new(flag)).count()
}

/// Wartość stojąca **zaraz za** flagą.
fn value_after<'a>(argv: &'a [PathBuf], flag: &str) -> Option<&'a Path> {
    let at = argv.iter().position(|arg| *arg == PathBuf::from(flag))?;
    argv.get(at + 1).map(PathBuf::as_path)
}

/// Fragment nigdy nie niesie `--plugin-dir` z wartością o zerowej długości.
///
/// Pytamy w KAŻDYM z przypadków, nie tylko w pozytywnym: `--setting-sources ""` stoi w tym samym
/// argv i tam pusty argument jest **poprawny**, więc pomylenie tych dwóch kształtów jest realne,
/// a skutek — połknięcie następnej flagi jako wartości — nie wygląda jak błąd.
fn never_a_flag_without_a_value(argv: &[PathBuf]) {
    for (index, argument) in argv.iter().enumerate() {
        assert!(
            argument != Path::new("--plugin-dir")
                || argv
                    .get(index + 1)
                    .is_some_and(|value| !value.as_os_str().is_empty()),
            "argv carries --plugin-dir with nothing after it, so the driver's next flag becomes \
             its argument: {argv:?}"
        );
    }
}

/// Wszystko, co leży pod `root`, ścieżkami względnymi. Pusty zbiór także wtedy, gdy `root` nie
/// istnieje — o to chodzi w punkcie (d): katalog, który powstał, prędzej czy później zostanie
/// komuś podany.
fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(root, Path::new(""), &mut found);
    found.sort();
    found
}

fn walk(dir: &Path, prefix: &Path, found: &mut Vec<PathBuf>) {
    let Ok(listing) = fs::read_dir(dir) else {
        return;
    };
    for entry in listing {
        let entry = entry.unwrap();
        let relative = prefix.join(entry.file_name());
        if fs::symlink_metadata(entry.path()).unwrap().is_dir() {
            walk(&entry.path(), &relative, found);
        } else {
            found.push(relative);
        }
    }
}

#[test]
fn the_plugin_directory_lives_under_the_run_and_its_path_reaches_the_command_once()
-> Result<(), Box<dyn Error>> {
    let world = world();

    // ── Przypadek pozytywny: dwie z trzech umiejętności gospodarza ────────────────────────
    let chosen = Chosen {
        skills: vec!["alpha".to_owned(), "beta".to_owned()],
        ..Chosen::default()
    };
    let inherited = wire::from_the_host(&world.project, &world.run, &chosen)?;

    let command = ClaudeDriver::new()
        .with_inherited(inherited.flags().to_vec())
        .command(&spec(&world.project));
    let argv = argv_of(&command);
    never_a_flag_without_a_value(&argv);

    // (b) RAZ. Dwa razy znaczy dwa katalogi i CLI wybierające jeden z nich — a który, tego
    // z naszej strony nie widać ani w logu, ani na ekranie.
    let count = times(&argv, "--plugin-dir");
    assert_eq!(
        count, 1,
        "--plugin-dir appears {count} time(s) in the command we would run. Two inherited skills \
         make exactly one plugin directory, so the flag naming it stands exactly once. argv was \
         {argv:?}"
    );

    let named = value_after(&argv, "--plugin-dir")
        .ok_or("--plugin-dir was passed with nothing after it")?;

    // (a) POD KATALOGIEM TEGO BIEGU. Katalog pluginu jest wyjściem builda i ma zniknąć razem
    // z biegiem (niezmiennik 4). Sterownik, który wybiera sobie miejsce sam, kładzie go
    // w `$TMPDIR` — czyli zostawia artefakt biegu poza biegiem.
    assert!(
        named.starts_with(&world.run),
        "the flag points at {named:?}, which is not under this run's directory ({:?}). A run \
         artefact outside the run survives the run, and nothing ever deletes it \
         (docs/ARCHITECTURE.md section 8)",
        world.run
    );

    // …a już zupełnie nie w cudzym `.claude/`. Repozytorium gospodarza jest dla nas TYLKO do
    // odczytu: dziedziczymy przez czytanie, a każdy nasz bajt w jego drzewie jest zmianą, o
    // której właściciel repozytorium dowiaduje się z `git status`.
    let host_claude = world.project.join(".claude");
    assert!(
        !named.starts_with(&host_claude),
        "the plugin directory landed inside the host's own {host_claude:?}. We inherit by \
         reading; writing into somebody else's .claude is the one thing this whole module \
         promises never to do"
    );

    // (c) BAJT W BAJT, i czytane SPOD ŚCIEŻKI Z ARGV — nie spod tej, którą podał test. Dopiero
    // to wiąże obietnicę daną procesowi z tym, co naprawdę leży na dysku. Porównanie po
    // `String` z `trim` przechodzi dla implementacji, która przepuściła plik przez emiter
    // umiejętności: emiter zwraca poprawny SKILL.md, tylko INNY — przestawione pola, zdjęty
    // `argument-hint`, przecytowane skalary. Człowiek ma móc zrobić `diff` i zobaczyć zero różnic.
    for (name, source) in [("alpha", ALPHA_MD), ("beta", BETA_MD)] {
        let written = named.join("skills").join(name).join("SKILL.md");
        assert!(
            fs::symlink_metadata(&written).is_ok(),
            "argv points the process at {named:?}, and {written:?} is not there. A directory the \
             vendor cannot read is the same green as no inheritance at all: the plugin loads and \
             registers zero skills, with a healthy-looking entry in the startup event"
        );
        assert_eq!(
            fs::read(&written)?,
            source.as_bytes().to_vec(),
            "{written:?} is not byte for byte what lies in the host repository"
        );
    }

    // ── (d) Kontrola przeciw pustemu przejściu ────────────────────────────────────────────
    //
    // Repozytorium BEZ `.claude/skills`, ale bieg, który dziedziczy coś innego. To jest ostrzejszy
    // przypadek niż „nic nie wybrano": implementacja, która stawia flagę, kiedy cokolwiek zostało
    // odziedziczone (albo kiedy katalog biegu w ogóle istnieje), przechodzi każdy test na pustym
    // wyborze i wykłada się dopiero tutaj — na prawdziwym biegu, przy starcie procesu.
    let nothing_to_carry = Chosen {
        subagent: Some("release-engineer".to_owned()),
        ..Chosen::default()
    };
    let bare = wire::from_the_host(&world.bare, &world.bare_run, &nothing_to_carry)?;

    let bare_command = ClaudeDriver::new()
        .with_inherited(bare.flags().to_vec())
        .command(&spec(&world.bare));
    let bare_argv = argv_of(&bare_command);
    never_a_flag_without_a_value(&bare_argv);

    let bare_count = times(&bare_argv, "--plugin-dir");
    assert_eq!(
        bare_count, 0,
        "this project has no .claude/skills at all, and --plugin-dir still stands {bare_count} \
         time(s) in the command. The vendor would be handed a directory that does not exist, and \
         that is a process which fails to start -- not a feature that is missing. argv was \
         {bare_argv:?}"
    );

    // …i nie powstał żaden katalog, którym dałoby się tę flagę uzasadnić. Bez tej asercji „pusty
    // katalog nie trafia do argv" jest spełnialne przez implementację, która katalog i tak
    // stworzyła, tylko go nie wymieniła — a katalog, który powstał, prędzej czy później zostanie
    // komuś podany.
    let left_behind = files_under(&world.bare_run);
    assert!(
        left_behind.is_empty(),
        "nothing was inherited into the plugin directory, and this run's directory holds \
         {left_behind:?} anyway"
    );

    Ok(())
}
