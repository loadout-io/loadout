//! Wyjście kroku „sprawdź" ma SUFIT — a werdykt i tak powstaje z całego strumienia.
//!
//! # Wada, przed którą to stoi
//!
//! Krok sprawdzający czyta oba potoki do EOF i trzyma wszystko, co komenda napisała, w jednym
//! buforze bez sufitu. `cargo test --tests` na tym drzewie pisze kilkanaście megabajtów, a rzecz,
//! która pisze bez opamiętania, pisze do [`GIVE_UP_AFTER`] — czyli przez pół godziny. Ten sam
//! tekst leci potem w całości do przekazania i do `run.json`, więc płaci za niego pamięć aplikacji,
//! dysk biegu i oczy człowieka naraz.
//!
//! # SŁABA WERSJA i dlaczego jest słaba
//!
//! Sam sufit: „`output.len() <= 65 536`". Przechodzi dla implementacji, która **przycina bufor
//! na wejściu do werdyktu** — a wtedy wzorzec dowodu stojący w pierwszej linii (`error: no test
//! target matched`, `test result: ok. 3 passed`) wypada z okna razem z resztą początku i suita,
//! która przeszła, czyta się jako suita, która nie ruszyła. Werdykt spada wtedy z powrotem na sam
//! kod wyjścia, przed czym stoi niezmiennik 19.
//!
//! Rozróżnia to wyłącznie para asercji z pierwszego testu: dowód **nie występuje** w zwróconym
//! wyjściu, a `matched` i `passed` są mimo to `true`. Jedna bez drugiej jest zielona dla złej
//! implementacji; obie naraz są nie do przejścia inaczej niż licząc werdykt ze strumienia,
//! zanim bajty zostaną odrzucone.
//!
//! # I druga słaba wersja: cisza
//!
//! Przycięcie, które nie mówi, że przycięło, jest gorsze od braku przycięcia. Człowiek dostaje
//! wtedy tekst zaczynający się w połowie zdania i nie ma jak odróżnić „komenda tyle napisała"
//! od „Loadout resztę wyrzucił" — a to jest różnica, po której szuka się przyczyny w dwóch
//! zupełnie innych miejscach. Dlatego zdanie o pominięciu jest asertowane tam, gdzie CZŁOWIEK
//! je widzi (niezmiennik 29): w `run.json` i w ciele przekazania, nie w wartości zwróconej
//! przez funkcję.
//!
//! # Dlaczego stałe są tu LITERAŁAMI
//!
//! Ani marker, ani sufit nie są importowane ze sterownika. Stała wzięta z `command.rs` zmienia
//! się razem z `command.rs`, więc test przestałby mierzyć umowę, a zacząłby mierzyć równość
//! nazwy z samą sobą — i milczałby o dniu, w którym zdanie dla człowieka zamieni się w żargon
//! albo sufit urośnie dziesięciokrotnie.
//!
//! # Dlaczego to NIE jest `#[ignore]`
//!
//! Procesy, które ten plik odpala, to `/bin/sh` i kilka skryptów w `tempfile::tempdir()` —
//! sekundy i zero pieniędzy, w przeciwieństwie do testów z prawdziwym `claude`.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::command::{CheckHow, CheckReport, CheckSpec, CommandDriver};
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, DecodedEvent, Probe, RunSpec};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::GroupProof;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Zdanie, którym przycięte wyjście mówi człowiekowi, czego w nim nie ma.
///
/// Po angielsku, bo to jest tekst interfejsu (decyzja D5), i bez żargonu: „truncated",
/// „buffer" ani „64 KiB" nie mówią nic komuś, kto właśnie szuka, dlaczego jego testy padły.
const OMITTED: &str = "[Loadout omitted earlier output from this check.]";

/// Sufit wyjścia jednego kroku „sprawdź", razem z markerem — 64 KiB.
const CEILING: usize = 65_536;

/// Wzorzec dowodu, ten sam we wszystkich przebiegach: `(\d+)` znaczy „co najmniej jedna cyfra".
const PROOF: &str = r"(\d+) passed";

/// Zdanie z licznikiem przejść, które komenda pisze na SAMYM POCZĄTKU, a potem zasypuje.
///
/// To jest cały pomiar: ono ma zniknąć ze zwróconego tekstu i ma zostać w werdykcie.
const EARLY: &str = "test result: ok. 3 passed; 0 failed";

/// Ostatnie słowo strumienia skarg i ostatnie słowo strumienia wyjścia.
const ERR_TAIL: &str = "the last thing the complaints stream said";
const OUT_TAIL: &str = "the last thing the output stream said";

/// Sufit cierpliwości dla testów na prawdziwym zegarze. Bez niego regresja objawia się jako
/// zawieszenie, bramka zwraca rc 124, a to jest fałszywa czerwień, nie dowód.
const PATIENCE: Duration = Duration::from_secs(30);

/// Licznik przejść w pierwszej linii, 256 KiB wypełniacza, na koniec ostatnie słowa obu potoków.
///
/// Wypełniacz nie ma ANI JEDNEJ cyfry: gdyby miał, wzorzec dowodu trafiłby także w zachowany
/// ogon i test nie mierzyłby już niczego. Przerwy przed każdym z dwóch ostatnich zdań są
/// synchronizacją, nie ozdobą — bez nich oba stoją w buforach naraz i to, które wyjdzie
/// pierwsze, zależy od tego, który poll wypadł pierwszy.
const FLOOD: &str = r#"#!/bin/sh
echo "test result: ok. 3 passed; 0 failed"
s=xxxxxxxxxxxxxxxx
s=$s$s$s$s
s=$s$s$s$s
s=$s$s$s$s
s=$s$s$s$s
i=0
while [ $i -lt 64 ]; do
  echo "$s"
  i=$((i + 1))
done
sleep 1
echo "the last thing the complaints stream said" >&2
sleep 1
echo "the last thing the output stream said"
exit 0
"#;

/// To samo zalanie, bez ostatnich zdań i bez przerw: droga produkcyjna mierzy przekazanie,
/// a nie kolejność potoków, więc nie ma po co czekać dwóch sekund.
const FLOOD_QUIETLY: &str = r#"#!/bin/sh
echo "test result: ok. 3 passed; 0 failed"
s=xxxxxxxxxxxxxxxx
s=$s$s$s$s
s=$s$s$s$s
s=$s$s$s$s
s=$s$s$s$s
i=0
while [ $i -lt 64 ]; do
  echo "$s"
  i=$((i + 1))
done
exit 0
"#;

/// Ćwierć megabajta tekstu, w którym każdy znak jest wielobajtowy — a na końcu JEDEN znak
/// rozcięty między dwoma odczytami.
///
/// Trzy bajty czterobajtowego znaku, przerwa, czwarty bajt. Przerwa jest tu jedyną rzeczą,
/// która czyni ten pomiar deterministycznym: bez niej oba kawałki leżą w buforze potoku razem
/// i czytelnik dostaje je jednym `read`, czyli nie mierzy nic. Z nią granica porcji przechodzi
/// przez środek znaku ZAWSZE, i to w zachowanym oknie, nie w odrzuconym początku.
const MANY_CHARACTERS: &str = r#"#!/bin/sh
s="żółć🙂"
s=$s$s$s$s
s=$s$s$s$s
s=$s$s$s$s
s=$s$s$s$s
i=0
while [ $i -lt 96 ]; do
  echo "$s"
  i=$((i + 1))
done
printf '\360\237\231'
sleep 1
printf '\202\n'
echo "2 passed"
exit 0
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_flood_keeps_the_tail_and_says_what_it_dropped() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let script = write_script(dir.path(), "flood.sh", FLOOD)?;
    let report = one_check(dir.path(), &script.display().to_string()).await?;

    // ── (a) SUFIT ─────────────────────────────────────────────────────────────────────────
    assert!(
        report.output.len() <= CEILING,
        "a check step may hand back at most {CEILING} bytes; this one handed back {}. The command \
         wrote a quarter of a megabyte, and a command that writes without end writes for half an \
         hour",
        report.output.len()
    );

    // ── (b) I MÓWI, ŻE PRZYCIĄŁ ───────────────────────────────────────────────────────────
    // Zdanie stoi na SAMYM POCZĄTKU, bo tam człowiek zaczyna czytać, i kończy się nową linią:
    // ciało przekazania tnie się po granicy WIERSZA, więc marker bez własnego wiersza wypadłby
    // z niego pierwszy.
    assert!(
        report.output.starts_with(OMITTED),
        "output that lost its beginning has to say so, in the first thing a person reads. It \
         starts with: {:?}",
        &report.output[..report.output.len().min(120)]
    );

    // ── (c) POMINIĘTE BAJTY NAPRAWDĘ ZNIKNĘŁY ─────────────────────────────────────────────
    assert!(
        !report.output.contains(EARLY),
        "the first line of a quarter-megabyte flood cannot still be here — a ceiling that keeps \
         the beginning keeps everything"
    );

    // ── (d) A WERDYKT JE WIDZIAŁ. To jest ta para, której nie da się przejść przycinając ──
    //     bufor przed werdyktem: wzorca w oddanym tekście już nie ma, a odpowiedź jest „tak".
    assert!(
        report.matched,
        "the pass count was in the stream, so the proof matched — reading `matched` back out of \
         the TRIMMED text turns every long check into a suite that never ran (invariant 19)"
    );
    assert!(
        report.passed,
        "exit code zero AND a proof that matched is the one shape of a check that passed; this \
         one came back as failed with exit code {:?}",
        report.exit_code
    );
    assert_eq!(
        report.exit_code,
        Some(0),
        "the script ends with `exit 0`, so anything else means it never got there"
    );

    // ── (e) OBA POTOKI DOSZŁY DO EOF i ich ostatnie słowa zostały ─────────────────────────
    assert!(
        report.output.contains(ERR_TAIL),
        "the complaints stream has to be drained to EOF and its last words kept; `npm` writes its \
         summary there, and a pipe nobody empties stops the child on `write`"
    );
    assert!(
        report.output.contains(OUT_TAIL),
        "and so does the output stream — the window keeps the END, which is where the answer is"
    );

    // ── (f) OKNO SIĘGA KOŃCA STRUMIENIA ───────────────────────────────────────────────────
    // Które z dwóch zdań wypadło ostatnie, ten test nie mierzy: to są dwa niezależne potoki,
    // a kolejność ich odczytu jest kolejnością budzeń, nie umową. Mierzone jest to, o co
    // naprawdę chodzi — że za ostatnim słowem komendy nie ucięto już nic.
    let ends = report.output.trim_end();
    assert!(
        ends.ends_with(OUT_TAIL) || ends.ends_with(ERR_TAIL),
        "the kept window has to reach the very end of the stream. It ends with: {:?}",
        &ends[ends.len().saturating_sub(120)..]
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_short_check_keeps_every_byte_of_both_streams() -> Result<(), Box<dyn Error>> {
    // STRAŻNIK SUFITU. Bez niego umowę spełnia także implementacja, która przycina ZAWSZE —
    // dokleja marker do wyjścia, które w całości mieściło się w oknie, i kłamie o pominięciu,
    // którego nie było. Dziewięć sprawdzeń z dziesięciu wygląda dokładnie tak.
    let dir = tempfile::tempdir()?;
    let said = "1 passed\n";
    let complained = "a warning nobody asked for\n";
    let report = one_check(
        dir.path(),
        "echo \"1 passed\"; echo \"a warning nobody asked for\" >&2; exit 0",
    )
    .await?;

    assert!(
        !report.output.contains(OMITTED),
        "nothing was dropped here, so nothing may claim it was: {:?}",
        report.output
    );
    assert_eq!(
        report.output.len(),
        said.len() + complained.len(),
        "a short check comes back byte for byte — both streams, nothing added, nothing cut. It \
         came back as {:?}",
        report.output
    );
    assert!(
        report.output.contains(said.trim_end()),
        "the output stream is in there: {:?}",
        report.output
    );
    assert!(
        report.output.contains(complained.trim_end()),
        "and so is the complaints stream — the verdict reads both (invariant 19 needs the half \
         that `npm` writes). It came back as {:?}",
        report.output
    );
    assert!(report.passed, "exit code zero and a pass count is a pass");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_flood_of_characters_never_becomes_question_marks() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let script = write_script(dir.path(), "characters.sh", MANY_CHARACTERS)?;
    let report = one_check(dir.path(), &script.display().to_string()).await?;

    assert!(
        report.output.len() <= CEILING,
        "the ceiling holds for text that is not ASCII too; this came back as {} bytes",
        report.output.len()
    );
    // Znak rozcięty między dwoma odczytami. Dekodowanie porcji osobno zamienia go w DWA znaki
    // zastępcze — trzy bajty bez końca i jeden ogon bez początku — i wygląda to jak wyjście
    // komendy, nie jak nasza wada.
    assert!(
        !report.output.contains('\u{FFFD}'),
        "a character split across two reads has to survive as itself; decoding each chunk on its \
         own puts a replacement character in the middle of somebody's test output and it reads as \
         THEIR problem"
    );
    assert!(
        report.output.contains('🙂'),
        "and it has to be the character the command actually wrote"
    );
    assert!(
        report.passed,
        "the pass count is the last line, so it is in the window and the check passed"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_check_killed_by_a_signal_keeps_the_match_it_already_saw() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    // Wiersz powłoki, nie skrypt, i to jest wymóg pomiaru: komenda złożona zostaje w powłoce,
    // którą wystartował Loadout, więc `$$` jest pid-em LIDERA — tego procesu, którego status
    // czytamy. Skrypt w osobnym pliku bywa `exec`owany i wtedy `$$` znaczy raz jedno, raz drugie.
    let driver = CommandDriver::new();
    let spec = CheckSpec {
        command: "echo \"4 passed\"; sleep 0.2; kill -9 $$".to_owned(),
        proof: PROOF.to_owned(),
        cwd: dir.path().to_path_buf(),
        required_tests: Vec::new(),
    };
    let mut live = driver.start(&spec)?;
    let cancel = CancellationToken::new();
    let end = tokio::time::timeout(PATIENCE, live.settle(&cancel))
        .await
        .map_err(|_| format!("the check did not come back within {PATIENCE:?}"))?;

    let described = format!("{:?}", end.how);
    let CheckHow::Ran(report) = end.how else {
        return Err(format!("the command was killed, not stopped by a person: {described}").into());
    };
    assert!(
        report.matched,
        "the pass count was printed BEFORE the signal, so the proof matched — what the command \
         managed to say still counts"
    );
    assert_eq!(
        report.exit_code, None,
        "a process that died from a signal has no exit code at all, and `None` is not zero"
    );
    assert!(
        !report.passed,
        "so it did not pass: half a verdict is not a verdict (invariant 19)"
    );

    // I grupa jest MARTWA, z dowodem — nie z potwierdzenia, że sygnał poszedł (niezmiennik 6).
    let proof = tokio::time::timeout(PATIENCE, live.cancel())
        .await
        .map_err(|_| "proving the group dead hung")?;
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "the group of a killed check has to come back proven dead; it came back {proof:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_check_that_never_ends_still_comes_back_overdue_and_dead() -> Result<(), Box<dyn Error>> {
    // Zegar stoi, więc pół godziny limitu mija natychmiast; prawdziwe pół godziny w suicie jest
    // niewykonalne, a `GIVE_UP_AFTER` nie schodzi niżej bez zmiany stałej produkcyjnej.
    let dir = tempfile::tempdir()?;
    let driver = CommandDriver::new();
    let spec = CheckSpec {
        // Dużo tekstu, a potem cisza: to jest kształt komendy, która wisi. Sufit wyjścia nie ma
        // prawa zmienić tego, co się z nią dzieje po limicie czasu.
        command: "i=0; while [ $i -lt 64 ]; do echo \"still working\"; i=$((i + 1)); done; \
                  sleep 3600"
            .to_owned(),
        proof: PROOF.to_owned(),
        cwd: dir.path().to_path_buf(),
        required_tests: Vec::new(),
    };
    let mut live = driver.start(&spec)?;
    let cancel = CancellationToken::new();
    // 2026-08-31 — bez zewnętrznego `timeout`: pod zatrzymanym zegarem byłby kolejnym terminem,
    // a eskalacja supervisora czeka też na odpowiedź prawdziwego jądra. Auto-advance potrafi wtedy
    // dobiec do strażnika, zanim system zdąży potwierdzić ESRCH, więc test mierzyłby własny zegar
    // zamiast deadline'u kroku. Zawieszenie nadal kończy budżet zawężonego checka.
    let end = live.settle(&cancel).await;

    let described = format!("{:?}", end.how);
    let CheckHow::Overdue(proof) = end.how else {
        return Err(format!(
            "a command that never ends has to come back Overdue, and the deadline has to be OURS: \
             {described}"
        )
        .into());
    };
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "and it carries the proof that the group is gone, not a report that a signal was sent \
         (invariants 6 and 10). It carried {proof:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_person_reading_the_handoff_is_told_the_output_was_cut() -> Result<(), Box<dyn Error>> {
    // NIEZMIENNIK 29: to samo zdanie, ale tam, gdzie widzi je człowiek — w `run.json` i w pliku
    // przekazania, po całym prawdziwym biegu, nie w wartości zwróconej przez funkcję.
    let bench = Bench::new()?;
    let script = bench.script("flood.sh", FLOOD_QUIETLY)?;
    let workflow = bench.workflow("one-check", &one_check_workflow(&script))?;
    let store = Store::open(&bench.db())?;

    let deps = RunDeps {
        home: bench.home.path(),
        library: bench.home.path().to_path_buf(),
        project: bench.project.path(),
        store: &store,
        drivers: no_drivers(),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };
    let report = one_run(&deps, &request).await??;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "the command exited zero and printed its pass count in the first line — the fact that the \
         line is no longer in the kept output may not change the verdict"
    );
    assert_eq!(report.outcome, Outcome::Done, "so the run finished clean");

    // ── (a) ZDANIE W KSIĘDZE BIEGU ────────────────────────────────────────────────────────
    let run: Value = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
    let step = run
        .get("steps")
        .and_then(Value::as_array)
        .and_then(|steps| steps.first().cloned())
        .ok_or("run.json describes no steps")?;
    let summary = step
        .get("summary")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("the check step wrote no result at all: {step}"))?;
    assert!(
        summary.starts_with(OMITTED),
        "the first thing a person reads about this step has to say that the beginning is gone. \
         run.json says: {summary:?}"
    );

    // ── (b) I W PLIKU, KTÓRY CZYTA NASTĘPNY KROK ──────────────────────────────────────────
    let files = every_file(&report.dir)?;
    let handoff = files
        .iter()
        .find(|(path, _)| path.to_string_lossy().contains("/handoffs/"))
        .ok_or("this run left no handoff at all")?;
    assert!(
        handoff.1.contains(OMITTED),
        "the handoff body is what the next step and the person both read; it says: {:?}",
        handoff.1
    );

    // ── (c) ZAŁĄCZNIK TEŻ MA SUFIT ────────────────────────────────────────────────────────
    // Załącznik trzyma ORYGINAŁ przekazania, więc bez sufitu w sterowniku ćwierć megabajta
    // ląduje na dysku biegu mimo przyciętego ciała.
    for (path, text) in &files {
        assert!(
            text.len() <= CEILING,
            "no file of this run may carry more than {CEILING} bytes of command output; {} has \
             {}",
            path.display(),
            text.len()
        );
    }

    // ── (d) A POMINIĘTEGO DOWODU NIE MA NIGDZIE ───────────────────────────────────────────
    // Nie „nie ma go w tym jednym polu", tylko nie ma go w ŻADNYM pliku katalogu biegu:
    // przycięcie, po którym wyrzucone bajty leżą obok w drugim pliku, nie jest przycięciem.
    for (path, text) in &files {
        assert!(
            !text.contains(EARLY),
            "the dropped beginning is still on disk, in {}",
            path.display()
        );
    }
    Ok(())
}

/// Jeden krok „sprawdź" na prawdziwym procesie, od startu do werdyktu.
async fn one_check(cwd: &Path, command: &str) -> Result<CheckReport, Box<dyn Error>> {
    let driver = CommandDriver::new();
    let spec = CheckSpec {
        command: command.to_owned(),
        proof: PROOF.to_owned(),
        cwd: cwd.to_path_buf(),
        required_tests: Vec::new(),
    };
    let cancel = CancellationToken::new();
    let end = tokio::time::timeout(PATIENCE, driver.run(&spec, &cancel))
        .await
        .map_err(|_| format!("the check did not come back within {PATIENCE:?}"))??;
    let described = format!("{:?}", end.how);
    match end.how {
        CheckHow::Ran(report) => Ok(report),
        _ => Err(format!("the command ran to the end, so this has to be Ran: {described}").into()),
    }
}

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę.
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Każdy plik katalogu biegu razem z treścią, rekurencyjnie.
///
/// Bajty czytane stratnie, bo w katalogu biegu bywa i tekst, i co innego — a pytanie brzmi
/// „czy TEN napis gdzieś tam jest", nie „czy wszystko jest tekstem".
fn every_file(dir: &Path) -> io::Result<Vec<(PathBuf, String)>> {
    let mut found = Vec::new();
    let mut left = vec![dir.to_path_buf()];
    while let Some(next) = left.pop() {
        for entry in fs::read_dir(&next)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                left.push(path);
            } else {
                let raw = fs::read(&path)?;
                found.push((path, String::from_utf8_lossy(&raw).into_owned()));
            }
        }
    }
    Ok(found)
}

/// Plik workflow z dokładnie jednym krokiem — sprawdzającym.
fn one_check_workflow(script: &Path) -> String {
    // Ścieżka bezwzględna wprost w komendzie: środowisko dziecka jest czyszczone, więc przez
    // zmienną nie przejdzie, a katalog roboczy kroku jest folderem projektu, nie tym tempdirem.
    format!(
        r#"{{
  "format": 1,
  "id": "wf_bounded_check",
  "name": "Run the checks",
  "steps": [
    {{
      "kind": "check",
      "id": "s_check",
      "name": "Run the checks",
      "command": "{}",
      "proof": "(\\d+) passed",
      "folder": {{ "use": "project" }},
      "at": {{ "x": 24, "y": 24 }}
    }}
  ],
  "links": []
}}"#,
        script.display()
    )
}

/// Jeden bieg z limitem cierpliwości. Zewnętrzny `Result` mówi „bieg wrócił", wewnętrzny — czym.
async fn one_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
) -> Result<Result<RunReport, loadout_lib::commands::RunError>, Box<dyn Error>> {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let drain = async move {
        let _ = pump.await;
    };

    let both = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(deps, request, sink), drain)
    })
    .await
    .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
    Ok(both.0)
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium.
struct Bench {
    home: TempDir,
    project: TempDir,
    scripts: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        let scripts = TempDir::new()?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self {
            home,
            project,
            scripts,
        })
    }

    fn script(&self, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
        write_script(self.scripts.path(), name, body)
    }

    fn workflow(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{slug}.json"));
        fs::write(&path, text)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}

/// Fabryka sterowników, która nie umie oddać ani jednego.
///
/// Krok „sprawdź" nie ma vendora, więc w tym biegu nikt nie ma powodu prosić o sterownik —
/// a gdyby poprosił, tura kończy się nazwaną odmową zamiast cichym dublerem, który udaje,
/// że wszystko jest w porządku.
fn no_drivers() -> Drivers {
    Arc::new(|_| Arc::new(NoDriver) as Arc<dyn AgentDriver>)
}

#[derive(Debug)]
struct NoDriver;

#[async_trait]
impl AgentDriver for NoDriver {
    fn id(&self) -> &'static str {
        "none"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: None,
        })
    }

    async fn start(
        &self,
        _spec: RunSpec,
        _events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        Err(anyhow::anyhow!(
            "a check step has no vendor, so nothing in this run may ask for one"
        ))
    }
}
