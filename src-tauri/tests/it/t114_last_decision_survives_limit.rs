//! AC-4 for T-114: the last explicit loop decision survives handoff truncation exactly once.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::fs;

use loadout_lib::memory::handoff::{self, Kind, MetaDraft};

fn draft(step: u32) -> MetaDraft {
    MetaDraft {
        run: "01990000-0000-7000-8000-000000001114".to_owned(),
        step,
        from: "Judge".to_owned(),
        to: vec!["Implement".to_owned()],
        kind: Kind::Findings,
        title: "The verdict".to_owned(),
        reads: Vec::new(),
    }
}

fn oversized(prefix: &str, decision: &str) -> String {
    format!(
        "## Answer\n{prefix}{}\n{decision}\n\n## Evidence\n\n## Open\n",
        "a measured line that must stay in the full copy\n".repeat(240)
    )
}

#[test]
fn a_late_pass_or_fail_is_kept_once_and_the_full_copy_is_exact() -> Result<(), Box<dyn Error>> {
    for (step, decision) in [(1, "outcome: pass"), (2, "outcome: fail")] {
        let run = tempfile::tempdir()?;
        let original = oversized("", decision);
        let written = handoff::write_handoff(run.path(), draft(step), &original)?;
        let body = handoff::read_handoff(&written.path)?.body;

        assert!(
            written.truncated,
            "the fixture must cross the 8 KB handoff limit"
        );
        assert_eq!(
            body.lines().filter(|line| line.trim() == decision).count(),
            1,
            "the last deciding line was lost or duplicated in the cut body: {body:?}"
        );
        assert!(
            body.len() <= handoff::BODY_CAP,
            "preserving the decision grew the cut body past its byte limit"
        );
        let attachment = written.attachment.ok_or("a cut body has no full copy")?;
        assert_eq!(fs::read(&attachment)?, original.as_bytes());
    }
    Ok(())
}

#[test]
fn an_already_kept_last_decision_is_not_appended_again() -> Result<(), Box<dyn Error>> {
    let run = tempfile::tempdir()?;
    let original = oversized("outcome: pass\n", "outcome: pass");
    let written = handoff::write_handoff(run.path(), draft(3), &original)?;
    let body = handoff::read_handoff(&written.path)?.body;

    assert!(written.truncated);
    assert_eq!(
        body.lines()
            .filter(|line| line.trim() == "outcome: pass")
            .count(),
        1,
        "a decision already present before the cut must not be duplicated"
    );
    Ok(())
}

#[test]
fn an_oversized_preamble_shares_the_cap_with_sections_and_the_decision()
-> Result<(), Box<dyn Error>> {
    let run = tempfile::tempdir()?;
    let preamble_line = "preamble evidence that must end only on its line boundary\n";
    let original = format!(
        "{}## Answer\nKept answer.\n\n## Evidence\nReceipt.\n\n## Open\noutcome: fail\n",
        preamble_line.repeat(180)
    );
    let written = handoff::write_handoff(run.path(), draft(4), &original)?;
    let body = handoff::read_handoff(&written.path)?.body;

    assert!(written.truncated, "the preamble alone crosses the body cap");
    assert!(
        body.len() <= handoff::BODY_CAP,
        "the preamble must share the byte budget with section headers, pointers, and the final decision"
    );
    assert_eq!(
        body.lines()
            .filter(|line| line.trim() == "outcome: fail")
            .count(),
        1,
        "trimming the preamble must still preserve the last decision exactly once"
    );
    for heading in ["## Answer", "## Evidence", "## Open"] {
        assert_eq!(
            body.lines().filter(|line| *line == heading).count(),
            1,
            "trimming the preamble must preserve the required {heading} header"
        );
    }
    assert!(
        body.lines()
            .any(|line| line.starts_with("Moved to attachments/")),
        "a cut preamble must leave a visible pointer to the full copy"
    );
    let attachment = written.attachment.ok_or("a cut body has no full copy")?;
    assert_eq!(fs::read(&attachment)?, original.as_bytes());

    let kept_preamble = body
        .split("## Answer\n")
        .next()
        .ok_or("the cut body has no preamble boundary")?;
    assert!(
        kept_preamble.is_empty()
            || (kept_preamble.ends_with('\n')
                && kept_preamble
                    .lines()
                    .all(|line| line == preamble_line.trim_end())),
        "the preamble must be dropped or cut only after a complete line"
    );
    Ok(())
}

#[test]
fn no_decision_is_invented_and_a_short_body_is_untouched() -> Result<(), Box<dyn Error>> {
    let long_run = tempfile::tempdir()?;
    let without = oversized("", "not a decision");
    let cut = handoff::write_handoff(long_run.path(), draft(4), &without)?;
    assert!(cut.truncated);
    assert!(
        !handoff::read_handoff(&cut.path)?
            .body
            .lines()
            .any(|line| line.trim().starts_with("outcome:"))
    );

    let short_run = tempfile::tempdir()?;
    let short = "## Answer\nDone.\n\n## Evidence\nReceipt.\n\n## Open\nNone.\n";
    let kept = handoff::write_handoff(short_run.path(), draft(5), short)?;
    assert!(!kept.truncated);
    assert!(kept.attachment.is_none());
    assert_eq!(handoff::read_handoff(&kept.path)?.body, short);
    Ok(())
}
