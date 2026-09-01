//! T-82 AC-2: importer może utworzyć `Check` wyłącznie z literalnej komendy i literalnego
//! licznika przejść w tym samym, zapamiętanym pliku źródłowym.
//!
//! Kod wyjścia bez licznika nie jest dowodem (niezmienniki 19 i 20). Równie ważna jest kontrola
//! negatywna: tekst „nie uruchamiaj …" oraz prozatorska parafraza nie są komendami. Importer,
//! który wyławia każdy napis w backtickach, potrafiłby zbudować wykonywalny krok dokładnie z
//! polecenia zakazanego przez autora. Pochodzenie jest już częścią `ImportItem` z T-78, więc test
//! nie wymyśla drugiego pola w `CheckStep::extra`.

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::import::{ImportSourceRole, ImportStatus, ItemKind};
use loadout_lib::workflow::{Step, file};

const WORKFLOW_NAME: &str = "Release Train";
const SOURCE_PATH: &str = ".claude/commands/release-train.md";
const COMMAND: &str = "./verify.sh quick";
const PROOF: &str = r"(\d+) passed";
const QUESTION: &str = "Ship this release?";

const EXACT_SOURCE: &str = r"---
name: Release Train
description: Check the release, then ask a person
---

# Release Train

Run this command exactly as written:

command: `./verify.sh quick`
proof: `(\d+) passed`

question: `Ship this release?`
";

const UNRESOLVED_CASES: [(&str, &str, &str); 12] = [
    (
        "prohibited-check",
        r"---
name: Prohibited Check
description: This command is explicitly forbidden
---
Do not run `./verify.sh full`; there is no such check in this setup.
proof: `(\d+) passed`
",
        "./verify.sh full",
    ),
    (
        "paraphrased-check",
        r"---
name: Paraphrased Check
description: This routine only describes an intention
---
Run the quick verification suite and require a passing-test count.
",
        "quick verification",
    ),
    (
        "unproved-check",
        r"---
name: Unproved Check
description: This command has no execution proof
---
command: `./verify.sh quick`
",
        "./verify.sh quick",
    ),
    (
        "loose-command",
        r"---
name: Loose Command
description: Prose is not an executable declaration
---
Run `./verify.sh quick` before continuing.
proof: `(\d+) passed`
",
        "./verify.sh quick",
    ),
    (
        "never-rerun",
        r"---
name: Never Rerun
description: A warning must not become a command
---
Never rerun `./verify.sh full` after release.
proof: `(\d+) passed`
",
        "./verify.sh full",
    ),
    (
        "task-is-not-command",
        r"---
name: Task Is Not Command
description: A task belongs to an agent
---
task: `./verify.sh quick`
proof: `(\d+) passed`
",
        "./verify.sh quick",
    ),
    (
        "unterminated-command",
        r"---
name: Unterminated Command
description: An open code span is ambiguous
---
command: `./verify.sh quick
proof: `(\d+) passed`
",
        "./verify.sh quick",
    ),
    (
        "failed-proof",
        r"---
name: Failed Proof
description: A failure count cannot prove success
---
command: `./verify.sh quick`
proof: `(\d+) failed`
",
        "failed",
    ),
    (
        "unsafe-proof-suffix",
        r"---
name: Unsafe Proof Suffix
description: A passing fragment does not make an arbitrary pattern safe
---
command: `./verify.sh quick`
proof: `(\d+) passed|.*`
",
        "|.*",
    ),
    (
        "indented-command-label",
        r"---
name: Indented Command Label
description: A Markdown code block is not a workflow declaration
---
    command: `./verify.sh quick`
proof: `(\d+) passed`
",
        "./verify.sh quick",
    ),
    (
        "two-command-spans",
        r"---
name: Two Command Spans
description: Import cannot choose between two values
---
command: `./verify.sh quick` `./verify.sh full`
proof: `(\d+) passed`
",
        "./verify.sh quick",
    ),
    (
        "loose-question",
        r"---
name: Loose Question
description: Approval needs its exact label too
---
command: `./verify.sh quick`
proof: `(\d+) passed`
Then ask the person: `Ship this release?`
",
        "Ship this release?",
    ),
];

#[test]
fn a_literal_command_proof_and_source_become_one_check() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    write_source(repo.path(), SOURCE_PATH, EXACT_SOURCE)?;

    let preview = loadout_lib::import::translate::preview(repo.path())?;
    let workflow = preview
        .draft
        .workflows
        .iter()
        .find(|workflow| workflow.name == WORKFLOW_NAME)
        .ok_or_else(|| {
            format!(
                "the literal command and proof in {SOURCE_PATH} did not produce {WORKFLOW_NAME}"
            )
        })?;

    let checks: Vec<_> = workflow
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::Check(check) => Some(check),
            Step::Agent(_) | Step::Checkpoint(_) | Step::Serve(_) => None,
        })
        .collect();
    assert_eq!(
        checks.len(),
        1,
        "one literal command must become one check, not zero and not a check per mention"
    );
    assert_eq!(checks[0].command, COMMAND);
    assert_eq!(
        checks[0].proof, PROOF,
        "the importer must preserve the source's proof instead of inventing a default counter"
    );

    let questions: Vec<_> = workflow
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::Checkpoint(checkpoint) => checkpoint.question.as_deref(),
            Step::Agent(_) | Step::Check(_) | Step::Serve(_) => None,
        })
        .collect();
    assert_eq!(
        questions,
        vec![QUESTION],
        "human approval is a checkpoint; it is not another agent kind"
    );

    let item = item_from(&preview.draft.items, Path::new(SOURCE_PATH))?;
    assert_eq!(item.kind, ItemKind::Workflow);
    assert_eq!(item.status, ImportStatus::Ready);
    assert!(
        item.target.is_some(),
        "a reconstructed workflow needs a concrete target in the import plan"
    );
    let witnessed = fs::read_to_string(repo.path().join(SOURCE_PATH))?;
    assert!(
        witnessed.contains(&format!("`{COMMAND}`")) && witnessed.contains(&format!("`{PROOF}`")),
        "the Definition source recorded on the typed item must itself contain both values; provenance cannot point at a neighboring file"
    );
    Ok(())
}

#[test]
fn prohibited_paraphrased_and_unproved_commands_stay_unresolved() -> Result<(), Box<dyn Error>> {
    for (name, source, behavior) in UNRESOLVED_CASES {
        let repo = tempfile::tempdir()?;
        let relative = format!(".claude/commands/{name}.md");
        write_source(repo.path(), &relative, source)?;
        let preview = loadout_lib::import::translate::preview(repo.path())?;

        assert!(
            preview.draft.workflows.iter().all(|workflow| {
                workflow
                    .steps
                    .iter()
                    .all(|step| !matches!(step, Step::Check(_)))
            }),
            "{name} contains no evidenced command, but the importer made an executable check"
        );
        let item = item_from(&preview.draft.items, Path::new(&relative))?;
        assert_eq!(
            item.status,
            ImportStatus::NeedsChoice,
            "{name} must remain visible for a person's decision"
        );
        assert!(
            item.status_message
                .to_ascii_lowercase()
                .contains(&behavior.to_ascii_lowercase()),
            "the unresolved item must name the command or behavior instead of hiding it behind a generic warning. It said: {}",
            item.status_message
        );
    }
    Ok(())
}

#[test]
fn review_is_still_refused_as_a_step_kind() -> Result<(), Box<dyn Error>> {
    let document = r#"{
      "format": 1,
      "id": "review-is-not-a-kind",
      "name": "Review is an agent job",
      "steps": [{ "kind": "review", "id": "review", "name": "Review" }],
      "links": []
    }"#;
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("review.json");
    fs::write(&path, document)?;

    let refusal = match file::load(&path) {
        Ok(_) => {
            return Err(
                "kind=review was accepted, which hard-codes a ceremony stage into the engine"
                    .into(),
            );
        }
        Err(refusal) => refusal.to_string(),
    };
    assert!(
        refusal.to_ascii_lowercase().contains("review"),
        "the refusal must name the unknown kind so the person can repair the file. It said: {refusal}"
    );
    Ok(())
}

fn write_source(root: &Path, relative: &str, content: &str) -> Result<(), Box<dyn Error>> {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().ok_or("workflow source has no parent")?)?;
    fs::write(path, content)?;
    Ok(())
}

fn item_from<'a>(
    items: &'a [loadout_lib::import::ImportItem],
    source: &Path,
) -> Result<&'a loadout_lib::import::ImportItem, Box<dyn Error>> {
    items
        .iter()
        .find(|item| {
            item.sources.iter().any(|candidate| {
                candidate.path == source && candidate.role == ImportSourceRole::Definition
            })
        })
        .ok_or_else(|| {
            format!(
                "{} disappeared instead of becoming a typed import item",
                source.display()
            )
            .into()
        })
}
