//! WF-19: kontrolka zakresu nie przyjmuje całego zestawu ani nie akceptuje kodu.
use loadout_lib::{commands::lab, lab::EvalSet};
use serde_json::{Value, json};
use std::error::Error;

fn definition() -> Result<EvalSet, serde_json::Error> {
    serde_json::from_value(json!({
        "format":2,"id":"scope","name":"Scope","subject":{"kind":"workflow","id":"subject"},
        "cases":[
            {"id":"accepted","name":"Accepted","task":"Do it","expect":[],"command":"node test.cjs","proof":"passed: (\\d+)","status":"in-use","because":"Reviewed"},
            {"id":"draft","name":"Candidate","task":"Do it","expect":[],"command":"","proof":"","status":"suggested","because":"Pending human approval",
             "examiner":{"kind":"python","program":"/usr/bin/python3","source":"print('not yet approved')"}}
        ],
        "variants":[],"futureSetting":{"preserve":true}
    }))
}

#[test]
fn changing_scope_preserves_every_case_status_and_unknown_field() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    let original = definition()?;
    let revision = lab::save_set_inner(project.path(), &original, None)?;
    let saved = lab::save_protection_inner(project.path(), "scope", true, Some(&revision))?;
    let mut expected = serde_json::to_value(&original)?;
    expected["protected"] = Value::Bool(true);
    assert_eq!(serde_json::to_value(&saved.set)?, expected);
    let reopened = lab::read_set_inner(project.path(), "scope")?;
    assert_eq!(serde_json::to_value(reopened.set)?, expected);
    assert_eq!(reopened.revision, saved.revision);
    Ok(())
}

#[test]
fn a_stale_scope_form_refuses_without_overwriting_a_later_case_edit() -> Result<(), Box<dyn Error>>
{
    let project = tempfile::tempdir()?;
    let original = definition()?;
    let revision = lab::save_set_inner(project.path(), &original, None)?;
    let mut newer = original;
    newer.cases[1].task = "A later change must survive".to_owned();
    let newer_revision = lab::save_set_inner(project.path(), &newer, Some(&revision))?;
    let outcome = lab::save_protection_inner(project.path(), "scope", true, Some(&revision));
    assert!(
        outcome.is_err(),
        "a stale form rewrote the current comparison"
    );
    let reopened = lab::read_set_inner(project.path(), "scope")?;
    assert_eq!(reopened.revision, newer_revision);
    assert_eq!(
        serde_json::to_value(reopened.set)?,
        serde_json::to_value(newer)?
    );
    Ok(())
}
