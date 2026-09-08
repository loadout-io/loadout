//! CT-06: po rozpoczęciu bieg czyta własny pakiet, nie dzisiejszą bibliotekę.

use std::error::Error;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use loadout_lib::bridge::Answer;
use loadout_lib::context::files::{folder_of, library_root};
use loadout_lib::engine::step::StepState;
use serde_json::json;
use sha2::{Digest as _, Sha256};

use super::step_receives_selected_context::{Bench, Seen, drivers, pin, refusal_of, run, step};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deleting_the_library_after_adapter_start_does_not_change_the_read()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let own = bench.publish(
        "Frozen brief",
        "Keep using the bytes selected at Start.",
        "frozen",
        "Frozen",
        "FROZEN-REQUIREMENT survives deletion.",
        "FROZEN-SOURCE-CONTENT survives deletion after Start.",
    )?;
    let foreign = bench.publish(
        "Foreign brief",
        "This set belongs to another scope.",
        "foreign",
        "Foreign",
        "FOREIGN-REQUIREMENT never crosses the scope.",
        "FOREIGN-SOURCE-CONTENT must be refused.",
    )?;
    let mut protected = step(
        "protected",
        "CTX-FROZEN read the frozen material.",
        pin(&own),
    );
    protected["folder"] = json!({"use":"fresh-copy"});
    let workflow = bench.workflow(
        "frozen-context",
        &json!({
            "format": 2,
            "id": "ct-06-frozen",
            "name": "Frozen context",
            "steps": [protected],
            "links": [],
            "executionInputs": {
                "schema": 1,
                "isolateContexts": true,
                "contexts": {"protected": {"task":"Use only the selected brief."}},
                "stepContexts": {"protected":"protected"}
            }
        }),
    )?;
    let seen = Arc::new(Seen::default());
    let context_library = library_root(bench.home.path());
    let changed = folder_of(&context_library, &own.set_id)?.join("draft.json");
    let report = run(
        &bench,
        workflow,
        drivers(
            Arc::clone(&seen),
            None,
            Some((context_library.clone(), changed)),
            Some(format!("{}--source--{}", foreign.set_id, foreign.source_id)),
        ),
        1,
    )
    .await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "run={:?} started={} adapter={:?}",
        bench.run_errors(),
        seen.started.load(Ordering::SeqCst),
        seen.errors()
    );
    assert_eq!(seen.started.load(Ordering::SeqCst), 1);
    assert!(
        !context_library.exists(),
        "the fixture did not remove the live library"
    );
    assert!(
        seen.errors().is_empty(),
        "the controlled adapter failed: {:?}",
        seen.errors()
    );
    let visit = seen
        .visits()
        .pop()
        .ok_or("the protected step did not run")?;
    assert!(!visit.item_ids.is_empty());
    assert!(
        visit.read.matches("FROZEN-SOURCE-CONTENT").count() == 2,
        "the same frozen bytes were not returned after both edit and deletion: {visit:?}"
    );
    assert!(!visit.read.contains("FOREIGN-"));
    assert!(matches!(visit.foreign, Some(Answer::Refused(_))));
    assert!(
        visit
            .extra_dirs
            .iter()
            .all(|path| !path.to_string_lossy().contains("context-sources")),
        "the private package escaped through global extra_dirs: {:?}",
        visit.extra_dirs
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_referenced_source_refuses_start_before_the_first_process()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish(
        "Missing brief",
        "Refuse incomplete material.",
        "missing",
        "Missing source",
        "MISSING-REQUIREMENT names its exact source.",
        "MISSING-SOURCE-CONTENT must not disappear silently.",
    )?;
    let library = library_root(bench.home.path());
    let draft = folder_of(&library, &material.set_id)?.join("draft.json");
    let mut saved: serde_json::Value = serde_json::from_slice(&fs::read(&draft)?)?;
    saved["sources"] = json!([]);
    fs::write(&draft, serde_json::to_vec_pretty(&saved)?)?;
    let workflow = bench.workflow(
        "missing-context",
        &json!({
            "format": 2,
            "id": "ct-06-missing",
            "name": "Missing context",
            "steps": [step("missing", "CTX-MISSING must not run.", pin(&material))],
            "links": []
        }),
    )?;
    let seen = Arc::new(Seen::default());
    let error = refusal_of(
        run(
            &bench,
            workflow,
            drivers(Arc::clone(&seen), None, None, None),
            1,
        )
        .await,
        "Start accepted a ready version whose referenced source is missing",
    )?;

    assert_eq!(seen.started.load(Ordering::SeqCst), 0);
    let said = error.to_string();
    assert!(
        said.contains("Missing brief"),
        "the refusal did not name the set: {said}"
    );
    assert!(
        said.contains("missing"),
        "the refusal did not name the problem: {said}"
    );
    assert!(
        said.contains("Restore it or choose another ready version"),
        "the refusal did not give the person a next move: {said}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_referenced_source_with_no_readable_body_refuses_before_the_first_process()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish(
        "Empty source brief",
        "Refuse a source record whose bytes vanished.",
        "empty-source",
        "Empty source",
        "EMPTY-REQUIREMENT still points at this source.",
        "EMPTY-SOURCE-CONTENT must be present.",
    )?;
    let library = library_root(bench.home.path());
    let draft = folder_of(&library, &material.set_id)?.join("draft.json");
    let mut saved: serde_json::Value = serde_json::from_slice(&fs::read(&draft)?)?;
    saved["sources"][0]["text"] = json!("");
    fs::write(&draft, serde_json::to_vec_pretty(&saved)?)?;
    let workflow = bench.workflow(
        "empty-source-context",
        &json!({
            "format": 2,
            "id": "ct-06-empty-source",
            "name": "Empty source context",
            "steps": [step("empty", "CTX-MISSING must not run.", pin(&material))],
            "links": []
        }),
    )?;
    let seen = Arc::new(Seen::default());
    let error = refusal_of(
        run(
            &bench,
            workflow,
            drivers(Arc::clone(&seen), None, None, None),
            1,
        )
        .await,
        "Start accepted a referenced source whose readable body vanished",
    )?;

    assert_eq!(seen.started.load(Ordering::SeqCst), 0);
    assert!(error.to_string().contains("has no readable text"));
    assert!(
        error
            .to_string()
            .contains("Restore it or choose another ready version")
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_prepared_topic_file_refuses_before_the_first_process()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish(
        "Incomplete version",
        "Refuse an incomplete immutable version.",
        "missing-topic-file",
        "Missing topic file",
        "TOPIC-FILE-REQUIREMENT must remain exact.",
        "TOPIC-FILE-SOURCE must remain available.",
    )?;
    let library = library_root(bench.home.path());
    let folder = folder_of(&library, &material.set_id)?;
    fs::remove_file(
        folder
            .join("versions")
            .join(&material.revision)
            .join("topics")
            .join(format!("{}.md", material.topic_id)),
    )?;
    let workflow = bench.workflow(
        "missing-topic-file-context",
        &json!({
            "format": 2,
            "id": "ct-06-missing-topic-file",
            "name": "Missing topic file",
            "steps": [step("missing", "CTX-MISSING must not run.", pin(&material))],
            "links": []
        }),
    )?;
    let seen = Arc::new(Seen::default());
    let error = refusal_of(
        run(
            &bench,
            workflow,
            drivers(Arc::clone(&seen), None, None, None),
            1,
        )
        .await,
        "Start accepted an immutable version with a missing topic file",
    )?;

    assert_eq!(seen.started.load(Ordering::SeqCst), 0);
    assert!(error.to_string().contains("Incomplete version"));
    assert!(
        error
            .to_string()
            .contains("Restore it or choose another ready version")
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selecting_one_pdf_page_does_not_package_the_other_page() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let material = bench.publish(
        "Scoped PDF",
        "Give this step only the referenced page.",
        "scoped-pdf",
        "Selected page",
        "PDF-REQUIREMENT points only at page one.",
        "the text-source placeholder is replaced below",
    )?;
    let library = library_root(bench.home.path());
    let folder = folder_of(&library, &material.set_id)?;
    let source_root = folder.join("sources").join(&material.source_id).join("r1");
    fs::create_dir_all(source_root.join("pages"))?;
    let original = b"PDF-ORIGINAL must never be packaged";
    let page_one = b"PDF-PAGE-ONE is selected.";
    let page_two = b"PDF-PAGE-TWO belongs to another topic.";
    fs::write(source_root.join("original.pdf"), original)?;
    fs::write(source_root.join("pages/page-0001.txt"), page_one)?;
    fs::write(source_root.join("pages/page-0002.txt"), page_two)?;
    let draft = folder.join("draft.json");
    let mut saved: serde_json::Value = serde_json::from_slice(&fs::read(&draft)?)?;
    saved["sources"][0]["kind"] = json!("pdf");
    saved["sources"][0]["text"] = json!("");
    saved["sources"][0]["preparation"] = json!({"state":"ready"});
    saved["sources"][0]["file"] = json!({
        "path": format!("sources/{}/r1/original.pdf", material.source_id),
        "revision": "r1",
        "mime": "application/pdf",
        "bytes": original.len(),
        "fingerprint": format!("{:x}", Sha256::digest(original)),
        "derived": page_one.len() + page_two.len(),
        "pages": 2
    });
    fs::write(&draft, serde_json::to_vec_pretty(&saved)?)?;
    let version = folder.join("versions").join(&material.revision);
    let findings = version.join("findings.json");
    let mut saved_findings: serde_json::Value = serde_json::from_slice(&fs::read(&findings)?)?;
    saved_findings[0]["sources"][0]["part"] = json!("page 1");
    fs::write(&findings, serde_json::to_vec_pretty(&saved_findings)?)?;
    let manifest = version.join("manifest.json");
    let mut saved_manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest)?)?;
    saved_manifest["findings"][0]["sources"][0]["part"] = json!("page 1");
    fs::write(&manifest, serde_json::to_vec_pretty(&saved_manifest)?)?;
    let workflow = bench.workflow(
        "scoped-pdf-context",
        &json!({
            "format": 2,
            "id": "ct-06-scoped-pdf",
            "name": "Scoped PDF context",
            "steps": [step("pdf", "CTX-ALPHA read the selected PDF page.", pin(&material))],
            "links": []
        }),
    )?;
    let seen = Arc::new(Seen::default());
    let report = run(
        &bench,
        workflow,
        drivers(
            Arc::clone(&seen),
            None,
            None,
            Some(format!(
                "{}--source--{}--page-0002",
                material.set_id, material.source_id
            )),
        ),
        1,
    )
    .await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "run={:?} started={} adapter={:?}",
        bench.run_errors(),
        seen.started.load(Ordering::SeqCst),
        seen.errors()
    );
    assert!(seen.errors().is_empty(), "{:?}", seen.errors());
    let visit = seen.visits().pop().ok_or("the PDF step did not run")?;
    assert!(visit.read.contains("PDF-PAGE-ONE"));
    assert!(!visit.read.contains("PDF-PAGE-TWO"));
    assert!(!visit.read.contains("PDF-ORIGINAL"));
    assert!(matches!(visit.foreign, Some(Answer::Refused(_))));
    let package = fs::read_to_string(report.dir.join("context-sources/manifest.json"))?;
    assert!(package.contains("page-0001"));
    assert!(!package.contains("page-0002"));
    assert!(!package.contains("original.pdf"));
    Ok(())
}
