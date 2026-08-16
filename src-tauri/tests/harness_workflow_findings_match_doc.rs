//! AC-6 dla T-23: lista ustaleń w dokumencie i plik grafu sprawdzają się nawzajem.
//!
//! To jest kryterium przeciw dokumentowi, który mówi „wszystkie sześć etapów da się wyrazić",
//! stojąc obok pliku z dwoma kafelkami kontrolnymi w miejscach, w których miały być sprawdzenia.
//! Sprawdzenie, że blok się parsuje i ma sześć pozycji, przechodzi na takim dokumencie w całości —
//! bo mierzy dokument samym dokumentem. Dlatego zestawiamy dwa niezależne pliki danych i wymagamy
//! odwzorowania w OBIE strony: zdanie „to się da narysować" bez kafelka w JSON-ie jest tą samą
//! awarią, co cztery ozdobne krzywe między trzema zakodowanymi na sztywno punktami (niezmiennik 17).
//!
//! Warunek (c) jest tym, który naprawdę boli, i taki ma być. Etap zgłoszony jako niewyrażalny musi
//! **nazwać brakujący rodzaj kafelka**, a ten rodzaj nie ma prawa występować w pliku. Bez tego
//! zadanie odpowiada „tak, da się" na pytanie, którego nie zadało: dopisanie trzeciego rodzaju,
//! żeby graf się zmieścił, przeszłoby wtedy jako sukces. Do tego każdy taki etap wskazuje kafelek
//! zastępczy o rodzaju `checkpoint` — i to jest cały pomiar tego zadania zapisany mechanicznie:
//! „sprawdzenie" jest dziś pytaniem do człowieka, bo Loadout nie ma czym uruchomić własnej komendy
//! i sam wystawić wyniku.
//!
//! Warunek (d) domyka to z drugiej strony. Pięć kroków, pięć pozycji, które je nazywają, żadnego
//! kroku dwa razy i żadnej pozycji celującej w krok, którego nie ma. Szósta pozycja — „workspace" —
//! nie nazywa kroku, tylko ścieżkę pola, bo etap workspace jest właściwością kroku i nie ma
//! własnego kafelka (AC-3). Rozjazd w którąkolwiek stronę jest czerwony.
//!
//! Ścieżka pola to wskaźnik JSON (RFC 6901), a nie własna składnia: `serde_json` rozwiązuje go sam,
//! więc dokument mówi o tym samym pliku, którym parser czyta graf, i nie ma tu drugiego parsera.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Sześć etapów harnessu. Ani jednego mniej: etap pominięty w dokumencie to etap, o którym nikt
/// nie musiał powiedzieć, czy da się go wyrazić.
const STAGES: [&str; 6] = [
    "workspace",
    "implement",
    "gate",
    "second-opinion",
    "fix",
    "land",
];

/// Ogrodzenie bloku danych w dokumencie: otwarcie nazywa język, zamknięcie jest nagie.
const OPENS: &str = "```json";
const CLOSES: &str = "```";

fn graph_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../.loadout/workflows/ship-task.json")
}

fn document_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/harness-as-workflow.md")
}

/// Treść pliku, z asercją o własnym komunikacie przed odczytem — `No such file or directory` jest
/// podpisem fałszywej czerwieni i bramka odrzuciłaby taką czerwień jako niebyłą.
fn read(path: &Path, what: &str) -> Result<String, Box<dyn Error>> {
    assert!(
        path.exists(),
        "the {what} has not been written yet: {}",
        path.display()
    );
    Ok(fs::read_to_string(path)?)
}

fn graph() -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_str(&read(
        &graph_path(),
        "harness workflow",
    )?)?)
}

/// Jedyny blok kodu oznaczony jako `json` w dokumencie. Dwa bloki znaczą dwa miejsca, w których
/// ta lista może się rozjechać, a wtedy test sądziłby jedno z nich i nie wiedziałby o drugim.
fn findings() -> Result<Vec<Value>, Box<dyn Error>> {
    let markdown = read(&document_path(), "harness-as-workflow document")?;

    let mut blocks: Vec<Vec<&str>> = Vec::new();
    let mut inside = false;
    for line in markdown.lines() {
        let edge = line.trim();
        if !inside && edge == OPENS {
            inside = true;
            blocks.push(Vec::new());
        } else if inside && edge == CLOSES {
            inside = false;
        } else if inside && let Some(block) = blocks.last_mut() {
            block.push(line);
        }
    }

    assert_eq!(
        blocks.len(),
        1,
        "the document carries exactly one block of data the test reads. A document nobody parses \
         drifts away from the file it describes inside a week, and two blocks are two places to \
         drift"
    );

    let collected: Vec<&str> = blocks.into_iter().flatten().collect();
    let parsed: Value = serde_json::from_str(&collected.join("\n"))?;
    parsed
        .as_array()
        .cloned()
        .ok_or_else(|| "the block in the document is not a list of findings".into())
}

/// Wartość tekstowa pola pozycji.
fn field<'a>(finding: &'a Value, name: &str) -> Result<&'a str, String> {
    finding
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("a finding has no {name}: {finding}"))
}

fn stage(finding: &Value) -> Result<&str, String> {
    field(finding, "stage")
}

fn expressible(finding: &Value) -> Result<bool, String> {
    finding
        .get("expressible")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            format!("a finding does not say whether the stage is expressible: {finding}")
        })
}

/// Kroki z pliku: identyfikator -> rodzaj.
fn steps(graph: &Value) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let steps = graph
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("the harness workflow has no list of steps")?;
    let mut out = BTreeMap::new();
    for step in steps {
        let (Some(id), Some(kind)) = (
            step.get("id").and_then(Value::as_str),
            step.get("kind").and_then(Value::as_str),
        ) else {
            return Err(
                format!("a step in the harness workflow has no id or no kind: {step}").into(),
            );
        };
        out.insert(id.to_owned(), kind.to_owned());
    }
    Ok(out)
}

/// Krok, który ta pozycja nazywa: kafelek dla etapu wyrażalnego, kafelek zastępczy dla
/// niewyrażalnego. Pozycja o właściwości kroku nie nazywa żadnego — jej etap nie ma kafelka
/// i to jest jej treść.
fn names_a_step(finding: &Value) -> Result<Option<&str>, String> {
    if !expressible(finding)? {
        return Ok(Some(field(finding, "stand_in")?));
    }
    match field(finding, "as")? {
        "agent" => Ok(Some(field(finding, "where")?)),
        "step-property" => Ok(None),
        other => Err(format!(
            "a finding says the stage is written as {other}, and this document knows only 'agent' \
             and 'step-property'"
        )),
    }
}

#[test]
fn the_document_settles_every_stage_of_the_harness_exactly_once() -> Result<(), Box<dyn Error>> {
    let findings = findings()?;

    let mut named: Vec<&str> = Vec::new();
    for finding in &findings {
        named.push(stage(finding)?);
    }
    let mut sorted = named.clone();
    sorted.sort_unstable();
    let mut expected: Vec<&str> = STAGES.to_vec();
    expected.sort_unstable();

    assert_eq!(
        sorted, expected,
        "six stages go through the harness and each one is either expressible or it is not — a \
         stage missing from the list is a stage nobody had to answer for. Got: {named:?}"
    );
    Ok(())
}

#[test]
fn a_stage_called_expressible_has_a_tile_or_a_field_behind_it() -> Result<(), Box<dyn Error>> {
    let graph = graph()?;
    let steps = steps(&graph)?;

    for finding in &findings()? {
        if !expressible(finding)? {
            continue;
        }
        let stage = stage(finding)?;
        match field(finding, "as")? {
            "agent" => {
                let step = field(finding, "where")?;
                assert_eq!(
                    steps.get(step).map(String::as_str),
                    Some("agent"),
                    "the document says the {stage} stage is an agent step called {step}, and the \
                     file has no such step. 'This one can be drawn' with nothing drawn is the \
                     same failure as four decorative curves between three hard-coded points"
                );
            }
            "step-property" => {
                let at = field(finding, "where")?;
                let value = field(finding, "value")?;
                assert_eq!(
                    graph.pointer(at).and_then(Value::as_str),
                    Some(value),
                    "the document says the {stage} stage lives at {at} and reads {value}. A \
                     field path that resolves to something else — or to nothing — means the \
                     document is describing a file that is not this one"
                );
            }
            other => return Err(format!("a finding is written as {other}").into()),
        }
    }
    Ok(())
}

#[test]
fn a_stage_the_editor_cannot_express_names_the_kind_that_is_missing() -> Result<(), Box<dyn Error>>
{
    let graph = graph()?;
    let steps = steps(&graph)?;
    let mut settled = 0;

    for finding in &findings()? {
        if expressible(finding)? {
            continue;
        }
        settled += 1;
        let stage = stage(finding)?;
        let missing = field(finding, "missing_kind")?;

        assert!(
            !missing.is_empty(),
            "the {stage} stage is reported as impossible to draw without saying what is missing. \
             When something cannot be expressed that is a finding about the editor and it has to \
             be named out loud, with the kind of tile that is absent"
        );
        assert!(
            !steps.values().any(|kind| kind == missing),
            "the document says the {stage} stage needs a {missing} tile, and the file already \
             has one. Then either the kind was quietly added to the schema so the graph would \
             fit, or the finding is stale — and both of those answer a question this task never \
             asked"
        );

        let stand_in = field(finding, "stand_in")?;
        assert_eq!(
            steps.get(stand_in).map(String::as_str),
            Some("checkpoint"),
            "with no tile that runs a command Loadout owns, the {stage} stage falls back to \
             asking a person — so {stand_in} has to be a checkpoint in the file. An agent step \
             standing in here would be Loadout asking an agent whether its own checks passed"
        );
    }

    assert!(
        settled > 0,
        "a list of findings in which every stage came out expressible is the answer this task was \
         written to distrust: the harness graph is drawable only because the checks became a \
         question to a human. If that ever stops being true, this line is the one to delete"
    );
    Ok(())
}

#[test]
fn the_document_and_the_file_name_the_same_steps() -> Result<(), Box<dyn Error>> {
    let graph = graph()?;
    let steps = steps(&graph)?;

    let mut pointed: Vec<&str> = Vec::new();
    let findings = findings()?;
    for finding in &findings {
        if let Some(step) = names_a_step(finding)? {
            assert!(
                steps.contains_key(step),
                "the document points at a step called {step}, which is not in the file"
            );
            pointed.push(step);
        }
    }

    let mut sorted = pointed.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        pointed.len(),
        "one step named by two findings means two stages of the harness share a tile, and the \
         mapping stops being a mapping. Got: {pointed:?}"
    );

    let unnamed: Vec<&String> = steps
        .keys()
        .filter(|id| !pointed.contains(&id.as_str()))
        .collect();
    assert!(
        unnamed.is_empty(),
        "every tile in the file stands for a stage of the harness, and these are not accounted \
         for anywhere in the document: {unnamed:?}"
    );
    Ok(())
}
