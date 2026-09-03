//! AC-1 dla T-34: każda surowa linia agenta ląduje w pliku, także ta, której nie rozumiemy.
//!
//! To jest ten plik, który użytkownik wysyła jako dowód, i to on pozwala skasować `loadout.db`
//! bez straty (`AGENTS.md` niezmiennik 4, `docs/ARCHITECTURE.md` §2 pyt. 2). Do dziś nie
//! powstawał: `store::rebuild` (T-06) czyta `logs/agent-<id>.jsonl` od chwili swojego
//! powstania, a produkcyjna pętla sterownika nie zapisywała ani bajtu — czyli plik był czytany
//! przez kod, którego nikt nigdy nie nakarmił (niezmiennik 21, czytany od drugiej strony).
//!
//! Dlatego to kryterium biegnie **przez sterownik**, a nie przez `stream::pump` z gotowym
//! czytnikiem: pompa ma tee od T-05 i ma na to własne kryterium (`stream_raw_tee.rs`). Pytanie,
//! na które odpowiada ten plik, brzmi inaczej — czy tee dzieje się w **prawdziwym biegu**, po
//! prawdziwym procesie, w miejscu, w którym szuka go odbudowa.
//!
//! **Słaba wersja tego kryterium to `assert!(path.exists() && !bytes.is_empty())`.** Przechodzi
//! ją implementacja zapisująca wyłącznie linie, które zrozumiała — a taki plik kłamie tym
//! mocniej, im nowszy jest vendor: typy zdarzeń przybywają co tydzień, po cichu
//! (niezmiennik 5). Rozróżniają dwie rzeczy naraz i obie są potrzebne: **równość bajtowa**
//! całego strumienia i obecność linii o nieznanym `type`.
//!
//! Pięć pułapek siedzi w fiksturze i każda pada inaczej:
//!
//! - **nieznany `type`** — implementacja tee'ująca po parsowaniu gubi dokładnie tę linię,
//!   której potrzebuje zgłoszenie błędu;
//! - **linia `type:"user"` z `tool_result`** (2026-09, Z-14) — bez `--replay-user-messages`
//!   ten typ linii nie jest echem naszego wejścia, tylko **wynikiem czynności**: to z niego
//!   powstaje `ToolEnd`, a z niego `FileEdit`. Filtr prywatności odrzucający ją po samym
//!   `type` zostawia odbudowie historię bez ani jednego wiersza `Edited` i `Ran`, a paczce
//!   diagnostycznej wyjście padłej komendy;
//! - **linia kończąca się `CRLF`** — `BufReader::lines()` zjada `\r`, a po takim przejściu
//!   bajtowej identyczności nie da się już osiągnąć; jest to zarazem jedyny z tych czterech
//!   błędów, którego porównanie napisów po `trim()` nie widzi w ogóle;
//! - **escape `<`** — runda przez `serde_json` rozwija go do gołego znaku;
//! - **`0.14836290000000002`** — runda przez `f64` skraca tę liczbę i suma kosztów biegu robi
//!   się krzywa na zawsze, bez śladu.
//!
//! Test odpala prawdziwy proces (atrapę `claude`), więc **nie** jest `#[ignore]`: linia
//! `check:` tego kryterium nie podaje `--include-ignored`, a cel, który melduje
//! `0 passed`, nie jest dowodem (niezmiennik 19).

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::engine::drivers::claude::{ClaudeDriver, Transcript};
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, Policy, RunSpec};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Sufit na każde pojedyncze oczekiwanie. Regresja ma się objawić jako **czerwony test**, nie
/// jako zawieszenie: bramka czyta rc 124 jako „nic się nie wykonało", a to nie jest dowód.
const LIMIT: Duration = Duration::from_secs(20);

/// Ile miejsca mają kanały. Z zapasem, bo pełny kanał zatrzymuje pętlę czytającą, a zatrzymana
/// pętla wygląda dokładnie jak zawieszony agent.
const CHANNEL: usize = 256;

/// Krok, którego to strumień. Po tym identyfikatorze nazywa się plik i po nim `store::rebuild`
/// wie, do którego kroku należą zdarzenia.
const STEP: &str = "01996500-0000-7000-8000-00000000000a";

/// Nazwa katalogu biegu z `docs/ARCHITECTURE.md` §8: `<ts>__<id>`.
const RUN_DIR: &str = "2026-08-16T09-00-00Z__01996500";

/// Agent, którego strumień to jest.
const AGENT: &str = "builder";

/// Prompt pierwszej tury. Dłuższy niż dolna granica igły prywatności, więc **jest** skanowany
/// jako podciąg — a mimo to żadna linia fikstury go nie niesie.
const TURN: &str = "say what this folder is for";

/// Ile linii ma fikstura. Kryterium mówi **sześć**, więc sześć stoi w stałej, a nie
/// w komentarzu: przycięta fikstura przechodziłaby na krótszej sekwencji i nikt by tego nie
/// zauważył.
const LINES: usize = 6;

/// Typ zdarzenia, którego nikt nigdy nie wysłał. Poprawny JSON, nieznany `type` — dokładnie to,
/// co vendor dokłada co tydzień.
const UNKNOWN_TYPE: &str = r#""type":"quantum_flux""#;

/// Blok, którym CLI oddaje wynik czynności. Jedzie linią `type:"user"` i **niczym innym**.
const TOOL_RESULT: &str = r#""type":"tool_result""#;

/// Escape `JSON`-owy znaku mniejszości: ukośnik i `u003c`. Zapisany z podwójnym ukośnikiem, bo
/// pojedynczy byłby escape'em **Rusta**, a na drucie ma stać ten z `JSON`-a.
const ESCAPED: &str = "\\u003c";

/// Liczba, która nie przeżywa rundy przez `f64` w drugą stronę.
const LONG_NUMBER: &str = "0.14836290000000002";

/// Sześć linii, które atrapa wypisuje na stdout — i dokładnie to, co ma znaleźć się w tee.
///
/// Sklejone z `concat!`, a nie napisane jednym literałem, bo escape `JSON`-owy musiałby wtedy
/// stać dosłownie — a jedno „posprzątanie" pliku przez edytor skasowałoby całą pułapkę bez
/// śladu w diffie. Czwarta linia kończy się `CRLF`; plik ma więc sześć zakończeń `\n` i jeden
/// `\r`, który przeżywa albo nie przeżywa czytnika.
const STREAM: &str = concat!(
    r#"{"session_id":"01996500-0000-7000-8000-0000000000aa","type":"system","subtype":"init","model":"opus","tools":["Read","Bash"],"capabilities":["interrupt_receipt_v1"]}"#,
    "\n",
    r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"it splits on every "#,
    "\\u003c",
    r#" it meets"}]}}"#,
    "\n",
    r#"{"type":"quantum_flux","payload":{"a":1}}"#,
    "\n",
    r#"{"type":"system","subtype":"hook_started","hook_name":"SessionStart:startup"}"#,
    "\r\n",
    r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01","content":"1 file checked"}]}}"#,
    "\n",
    r#"{"type":"result","subtype":"success","is_error":false,"terminal_reason":"completed","num_turns":2,"duration_ms":6220,"total_cost_usd":0.14836290000000002,"result":"done"}"#,
    "\n",
);

/// Krok drugiej tury tego pliku. Osobny, bo osobny jest jego plik dowodowy.
const SHORT_STEP: &str = "01996500-0000-7000-8000-00000000000b";

/// Katalog biegu drugiej tury.
const SHORT_RUN_DIR: &str = "2026-08-16T09-05-00Z__01996501";

/// Tura krótsza niż dolna granica igły prywatności — i **cała** przyczyna kryterium 3.
///
/// Dwie litery. Jako igła skanowana w każdej linii trafiają w `"input_tokens"`, więc linia
/// kończąca turę znikała z pliku dowodowego po tym, jak człowiek napisał agentowi „ok".
const SHORT_TURN: &str = "ok";

/// Nazwa pola, które ta tura wycinała ze strumienia. Podciąg `ok` stoi w nim na drugiej pozycji.
const COUNTED_LENGTH: &str = r#""input_tokens":1420"#;

/// Dwie linie po turze `ok`: początek i wynik. Ani jedna nie niesie treści tej tury.
const SHORT_STREAM: &str = concat!(
    r#"{"session_id":"01996500-0000-7000-8000-0000000000ab","type":"system","subtype":"init","model":"opus","tools":["Read"]}"#,
    "\n",
    r#"{"type":"result","subtype":"success","is_error":false,"terminal_reason":"completed","num_turns":1,"duration_ms":1200,"total_cost_usd":0.002,"usage":{"input_tokens":1420,"output_tokens":37},"result":"done"}"#,
    "\n",
);

/// Atrapa `claude`: odbiera kopertę stdinem i wypisuje przygotowany strumień, bajt w bajt.
///
/// Strumień leży **w pliku obok skryptu**, a nie w treści skryptu: powłoka rozwijałaby w nim
/// escape'y i cudzysłowy, a wtedy „bajt w bajt" mierzyłoby `printf`, nie tee. Kopertę czytamy
/// przed pierwszym `printf`, tak jak prawdziwe CLI — proces, który wychodzi przed jej
/// odebraniem, mierzy zerwany potok, a nie zapis.
const DUMMY: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "2.1.233 (Claude Code)"
  exit 0
fi

here="$(dirname "$0")"
IFS= read -r envelope
printf '%s\n' "$envelope" >> "$here/stdin.log"

cat "$here/stream.jsonl"
exit 0
"#;

/// Zapisuje wykonywalny skrypt i zwraca jego ścieżkę.
///
/// Plik ze skryptem, nigdy `sh -c "…"` i nigdy kopia binarki systemowej: skopiowany plik
/// systemowy dostaje na `macOS` `SIGKILL` od podpisu kodu [T7 §8.2].
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// `RunSpec` jednej tury.
fn spec(run_id: Uuid, cwd: &Path, said: &str) -> RunSpec {
    RunSpec {
        run_id,
        cwd: cwd.to_path_buf(),
        prompt: said.to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Czy `haystack` zawiera `needle` jako ciąg bajtów.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Ile linii niesie ten ciąg bajtów. Liczone po `\n`, więc `\r` nie ma tu głosu.
///
/// `fold`, a nie `filter(…).count()`, i to nie jest kwestia gustu: to drugie jest dla clippy
/// „naiwnym liczeniem bajtów" i pod `-D warnings` **wywraca pełną bramkę**, która chodzi
/// z `--all-targets`. Jedyna podpowiedź, jaką lint daje, to nowa zależność (`bytecount`) za
/// policzenie znaków końca linii w pięciolinijkowej fiksturze — czyli lekarstwo droższe od
/// choroby. Liczone jest dokładnie to samo, co przedtem: bajty `\n`, bez żadnego innego.
fn newlines(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .fold(0, |count, byte| count + usize::from(*byte == b'\n'))
}

/// Kawałek bajtów wokół podanego przesunięcia, czytelnie i krótko.
fn around(bytes: &[u8], at: Option<usize>) -> String {
    let at = at.unwrap_or_default();
    let from = at.saturating_sub(20);
    let to = at.saturating_add(20).min(bytes.len());
    String::from_utf8_lossy(&bytes[from..to]).into_owned()
}

/// Porównuje dwa ciągi bajtów i pada **krótko**.
///
/// `assert_eq!` na dwóch wektorach po kilkanaście kilobajtów wypisuje kilkanaście tysięcy
/// liczb, a raport bramki przestaje dać się przeczytać — dokładnie w tej chwili, w której ktoś
/// ma go przeczytać.
fn assert_same_bytes(written: &[u8], expected: &[u8], why: &str) {
    let divergence = written
        .iter()
        .zip(expected)
        .position(|(left, right)| left != right);
    assert!(
        divergence.is_none(),
        "{why} They part company at byte {divergence:?}. There the file reads {:?} and the \
         stream read {:?}",
        around(written, divergence),
        around(expected, divergence),
    );
    assert_eq!(
        written.len(),
        expected.len(),
        "{why} The file holds {} bytes and the process wrote {}. A shorter file with an equal \
         prefix is a reader that ate the CR, or a tee that stopped at the first line nobody \
         could parse",
        written.len(),
        expected.len(),
    );
}

/// Puszcza jeden krok przez sterownik i wraca dopiero wtedy, gdy pętla czytająca skończyła.
///
/// Zamknięcie kanału zdarzeń jest **jedynym** uczciwym punktem synchronizacji: pętla porzuca
/// oba nadajniki, kiedy strumień się skończył, więc dopiero po nim wolno pytać dysk o plik.
/// Czekanie na `wait()` nie wystarcza — wynik tury przychodzi z linii `result`, a po niej może
/// jeszcze coś dojechać.
async fn run_one_step(
    home: &Path,
    run_dir: &Path,
    stream: &str,
    said: &str,
    step: &str,
) -> Result<(), Box<dyn Error>> {
    let binary = write_script(home, "claude", DUMMY)?;
    fs::write(home.join("stream.jsonl"), stream)?;

    // `logs/` powstaje razem z katalogiem biegu, tak jak w `commands::run` — sterownik ma tam
    // dopisać plik, a nie wymyślać układ katalogów.
    fs::create_dir_all(run_dir.join("logs"))?;

    let (events_tx, mut events) = mpsc::channel(CHANNEL);
    // Odbiornik wierszy żyje do końca funkcji. To kryterium jest o ścieżce DYSKU, a ścieżka
    // dysku nie ma prawa zależeć od tego, czy widok nadąża [T7 §4.1].
    let (lines_tx, _lines) = mpsc::channel(CHANNEL);

    let driver = ClaudeDriver::with_binary(binary).with_transcript(Transcript {
        run_dir: run_dir.to_path_buf(),
        step: step.to_owned(),
        agent: AGENT.to_owned(),
        lines: lines_tx,
    });

    let mut handle: Box<dyn AgentHandle> = timeout(
        LIMIT,
        driver.start(spec(Uuid::now_v7(), home, said), events_tx),
    )
    .await??;

    timeout(LIMIT, async { while events.recv().await.is_some() {} }).await?;

    // Koniec sesji, nie koniec tury: bez tego czasownika skończony krok zostawia żywy proces
    // [T1 §2]. Kod wyjścia jest tu bez znaczenia — to kryterium jest o pliku.
    let _code = timeout(LIMIT, handle.close()).await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_raw_line_is_in_the_file_including_the_one_nobody_understands()
-> Result<(), Box<dyn Error>> {
    // ── Fikstura naprawdę niesie pięć pułapek, o których jest to kryterium ─────────────────
    let expected = STREAM.as_bytes();
    assert_eq!(
        newlines(expected),
        LINES,
        "the criterion says six lines, so a shorter fixture would prove less than it claims"
    );
    assert!(
        contains(expected, UNKNOWN_TYPE.as_bytes()),
        "without a line of an unknown type this test cannot see a tee that writes only what it \
         parsed - and that is the failure it exists to catch"
    );
    assert!(
        contains(expected, TOOL_RESULT.as_bytes()),
        "without a line carrying the outcome of an action this test cannot see a filter that \
         judges a line by its type alone, and that filter is the whole reason this fixture grew \
         a sixth line"
    );
    assert!(
        contains(expected, b"\r\n"),
        "without a line ending in CRLF this test cannot see line-ending normalisation, the one \
         failure a trimmed string comparison also misses"
    );
    assert!(
        contains(expected, ESCAPED.as_bytes()),
        "without the JSON escape this test cannot see a round trip through serde_json"
    );
    assert!(
        contains(expected, LONG_NUMBER.as_bytes()),
        "without the long number this test cannot see a round trip through f64 formatting"
    );

    let home = tempfile::tempdir()?;
    let run_dir = home.path().join(".loadout").join("runs").join(RUN_DIR);
    run_one_step(home.path(), &run_dir, STREAM, TURN, STEP).await?;

    // ── Plik stoi tam, gdzie szuka go odbudowa ────────────────────────────────────────────
    let logs = run_dir.join("logs");
    let tee = logs.join(format!("agent-{STEP}.jsonl"));
    let written: Vec<String> = fs::read_dir(&logs)?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        tee.exists(),
        "a real step ran and left no transcript at {}. store::rebuild reads exactly this name \
         (T-06), so a run without this file is a run whose events live only as long as \
         loadout.db does - which is invariant 4 being false. The logs directory holds {written:?}",
        tee.display(),
    );

    // ── Bajt w bajt to, co wyszło z procesu ───────────────────────────────────────────────
    //
    // Brak pliku czytamy jako pustkę celowo: „tee nie powstało" ma paść na porównaniu bajtów,
    // a nie na błędzie wejścia-wyjścia, który bramka słusznie czyta jako fałszywą czerwień.
    let teed = fs::read(&tee).unwrap_or_default();
    assert_eq!(
        newlines(&teed),
        LINES,
        "the process wrote {LINES} lines and the file holds {}. One short means either the line \
         nobody could parse or the line carrying what an action returned was dropped on the way in",
        newlines(&teed),
    );
    assert!(
        contains(&teed, UNKNOWN_TYPE.as_bytes()),
        "the line with a type nobody has ever sent is missing from the file. The tee happens \
         BEFORE decoding precisely so that this line survives: it is the one a bug report needs, \
         and vendors add event types every week, quietly (invariant 5)"
    );
    assert!(
        contains(&teed, TOOL_RESULT.as_bytes()),
        "the line carrying what an action returned is missing from the file. Without \
         --replay-user-messages this shape is never an echo of what we typed - it is how the \
         agent app reports that a file was written or a command finished, and rebuilding from a \
         file without it leaves a person with a history that shows neither"
    );
    assert_same_bytes(
        &teed,
        expected,
        "the transcript is not what the child wrote. This file is the one a user attaches to a \
         bug report and the one that makes deleting loadout.db safe, so the moment it stops \
         being byte for byte the stream, the index stops being a rebuildable cache. A \
         difference in the middle is a round trip through serde_json (key order, the escape, \
         the long number); a difference in length is a reader that ate the CR.",
    );

    Ok(())
}

/// Kryterium 3 z Z-14: dwuliterowa tura przestaje wycinać cudze linie.
///
/// Skan podciągów jest drugim, niezależnym strażnikiem prywatności i jego pomyłka jest cicha:
/// dopasowuje się do **dowolnego** miejsca w linii, więc im krótsza tura, tym więcej cudzego
/// tekstu wygląda jak nasz. „ok" trafia w `input_tokens`, więc po tej jednej odpowiedzi
/// człowieka wynik tury znikał z pliku dowodowego — a plik krótszy o wynik czyta się dokładnie
/// jak bieg, który się nie skończył.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_two_letter_turn_does_not_cut_the_line_that_ends_it() -> Result<(), Box<dyn Error>> {
    // ── Pułapka naprawdę siedzi w fiksturze ───────────────────────────────────────────────
    let expected = SHORT_STREAM.as_bytes();
    assert!(
        contains(expected, COUNTED_LENGTH.as_bytes()),
        "without a field whose own name contains the turn this test measures nothing: the whole \
         defect is a needle so short that it matches somebody else's word"
    );
    assert!(
        contains(COUNTED_LENGTH.as_bytes(), SHORT_TURN.as_bytes()),
        "the turn has to be a substring of that field, or the fixture is not the trap it claims \
         to be"
    );

    let home = tempfile::tempdir()?;
    let run_dir = home
        .path()
        .join(".loadout")
        .join("runs")
        .join(SHORT_RUN_DIR);
    run_one_step(home.path(), &run_dir, SHORT_STREAM, SHORT_TURN, SHORT_STEP).await?;

    let tee = run_dir
        .join("logs")
        .join(format!("agent-{SHORT_STEP}.jsonl"));
    let teed = fs::read(&tee).unwrap_or_default();
    assert!(
        contains(&teed, COUNTED_LENGTH.as_bytes()),
        "the line that ends the turn is missing from the file, and the only thing it had in \
         common with what the person typed was two letters inside a field name. The file a user \
         attaches to a bug report has to keep the answer, and rebuilding a run without it is \
         rebuilding a run that never finished"
    );
    assert_same_bytes(
        &teed,
        expected,
        "the transcript is not what the child wrote. A short answer from a person is still an \
         answer, and it may not cost the run its own output.",
    );

    Ok(())
}
