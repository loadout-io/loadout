#[test]
fn subworkflow_is_flattened_before_save() -> Result<(), Box<dyn std::error::Error>> {
    use loadout_lib::import::translate::flatten;
    use loadout_lib::workflow::{CheckpointStep, Link, Point, Step, WorkflowFile};
    let template = WorkflowFile {
        format: 1,
        id: "review".into(),
        name: "Review".into(),
        description: None,
        steps: vec![Step::Checkpoint(CheckpointStep {
            id: "ask".into(),
            name: "Ask".into(),
            question: None,
            at: Point { x: 0.0, y: 0.0 },
            extra: serde_json::Map::new(),
        })],
        links: vec![Link {
            from: "outside".into(),
            to: "ask".into(),
            max_turns: None,
        }],
        extra: serde_json::Map::new(),
    };
    let first = flatten(&template, "left");
    let second = flatten(&template, "right");
    assert_ne!(first.id, second.id);
    let first_step = match &first.steps[0] {
        Step::Checkpoint(step) => step,
        Step::Agent(_) | Step::Check(_) => return Err("the imported step changed kind".into()),
    };
    assert_eq!(first_step.id, "left.ask");
    assert_eq!(first.links[0].from, "outside");
    assert_eq!(first.links[0].to, "left.ask");
    assert_eq!(first.extra["expandedFrom"], "review");
    Ok(())
}
