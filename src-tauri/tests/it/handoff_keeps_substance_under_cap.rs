//! WP-04a: a short opening cannot turn substantive handoff sections into pointers only.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;

use loadout_lib::memory::handoff::{self, Handoff, Kind, MetaDraft, Written};

const OPENING: &str = "I read the header parser first.\n\n";
const ANSWER_LINE_START: &str = "Answer substance line ";

fn draft(step: u32) -> MetaDraft {
    MetaDraft {
        run: "run_wp_04a".to_owned(),
        step,
        from: "research-memory".to_owned(),
        to: vec!["planner".to_owned()],
        kind: Kind::Findings,
        title: "Memory handoff findings".to_owned(),
        reads: vec![],
    }
}

fn long_answer() -> String {
    let mut answer = String::from("## Answer\n");
    for line in 0..240 {
        let _ = writeln!(
            answer,
            "Answer substance line {line:03}: measured details stay with the reader."
        );
    }
    answer
}

fn sections() -> String {
    format!(
        "{}## Evidence\nReceipt one.\nReceipt two.\n\n## Open\nOne question.\n",
        long_answer()
    )
}

fn write_and_read(
    run_dir: &Path,
    step: u32,
    body: &str,
) -> Result<(Written, Handoff), Box<dyn Error>> {
    let written = handoff::write_handoff(run_dir, draft(step), body)?;
    let saved = handoff::read_handoff(&written.path)?;
    Ok((written, saved))
}

fn heading_at(body: &str, name: &str) -> Option<usize> {
    let heading = format!("## {name}");
    let mut at = 0;
    while at < body.len() {
        let end = body[at..].find('\n').map_or(body.len(), |i| at + i + 1);
        if body[at..end].trim_end() == heading {
            return Some(at);
        }
        at = end;
    }
    None
}

fn section_body<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let at = heading_at(body, name)?;
    let after_heading = body[at..].find('\n').map_or(body.len(), |i| at + i + 1);
    let rest = &body[after_heading..];
    if rest.starts_with("## ") {
        return Some("");
    }
    let end = rest.find("\n## ").map_or(rest.len(), |i| i + 1);
    Some(&rest[..end])
}

fn answer_lines(body: &str) -> Vec<&str> {
    section_body(body, "Answer")
        .unwrap_or_default()
        .lines()
        .filter(|line| line.starts_with(ANSWER_LINE_START))
        .collect()
}

#[test]
fn an_opening_line_does_not_cost_the_reader_the_answer() -> Result<(), Box<dyn Error>> {
    let with_opening_run = tempfile::tempdir()?;
    let empty_opening_run = tempfile::tempdir()?;
    let section_text = sections();
    let with_opening = format!("{OPENING}{section_text}");

    let (written, saved_with_opening) = write_and_read(with_opening_run.path(), 2, &with_opening)?;
    let (_, saved_without_opening) = write_and_read(empty_opening_run.path(), 2, &section_text)?;
    let with_lines = answer_lines(&saved_with_opening.body);
    let without_lines = answer_lines(&saved_without_opening.body);

    assert!(written.truncated, "the fixture must cross the body cap");
    assert!(
        !with_lines.is_empty(),
        "the written handoff kept the short opening but replaced every readable Answer line \
         with a pointer:\n{}",
        saved_with_opening.body
    );
    assert_eq!(
        with_lines, without_lines,
        "a short opening should cost only its own bytes, not the substantive slice reserved \
         for the first oversized section"
    );
    assert!(
        saved_with_opening
            .body
            .lines()
            .any(|line| line.starts_with("Moved to attachments/")),
        "the omitted remainder must be visibly named and located"
    );
    Ok(())
}

#[test]
fn a_cut_opening_still_hands_over_the_short_answer_under_it() -> Result<(), Box<dyn Error>> {
    let run = tempfile::tempdir()?;
    let mut opening = String::new();
    for line in 0..100 {
        let _ = writeln!(opening, "Preamble line {line:03} {}", ".".repeat(40));
    }
    opening.push_str(&"x".repeat(4_000));
    opening.push('\n');
    let input = format!(
        "{opening}## Answer\nKept answer.\n\n## Evidence\nReceipt.\n\n## Open\nOne question.\n"
    );

    let (written, saved) = write_and_read(run.path(), 3, &input)?;

    assert!(written.truncated, "the opening alone must cross its budget");
    assert!(
        section_body(&saved.body, "Answer")
            .unwrap_or_default()
            .lines()
            .any(|line| line == "Kept answer."),
        "cutting the opening left room for the short Answer, but the written handoff hid it:\n{}",
        saved.body
    );
    assert!(
        saved
            .body
            .lines()
            .any(|line| line.starts_with("Moved to attachments/")),
        "the cut opening must still leave a visible route to the full text"
    );
    Ok(())
}

#[test]
fn an_empty_opening_keeps_the_start_of_a_long_answer() -> Result<(), Box<dyn Error>> {
    let run = tempfile::tempdir()?;
    let input = sections();
    let (written, saved) = write_and_read(run.path(), 4, &input)?;

    assert!(written.truncated, "the fixture must cross the body cap");
    assert!(
        answer_lines(&saved.body)
            .first()
            .is_some_and(|line| line.starts_with("Answer substance line 000:")),
        "the reader did not get the beginning of the long Answer:\n{}",
        saved.body
    );
    assert!(
        saved
            .body
            .lines()
            .any(|line| line.starts_with("Moved to attachments/")),
        "the omitted remainder must be visibly named and located"
    );
    Ok(())
}

#[test]
fn a_body_that_fits_is_written_word_for_word() -> Result<(), Box<dyn Error>> {
    let run = tempfile::tempdir()?;
    let input = "A short opening.\n\n## Answer\nDone.\n\n## Evidence\nReceipt.\n\n## Open\nNone.\n";
    let (written, saved) = write_and_read(run.path(), 5, input)?;

    assert!(
        !written.truncated,
        "a body under the cap was reported as cut"
    );
    assert!(
        written.attachment.is_none(),
        "an untouched body must not create a full-copy artifact"
    );
    assert_eq!(
        saved.body, input,
        "a body that fits must reach the reader byte for byte"
    );
    Ok(())
}

#[test]
fn a_verdict_at_the_end_of_a_long_body_survives_a_short_opening() -> Result<(), Box<dyn Error>> {
    let run = tempfile::tempdir()?;
    let input = format!(
        "{OPENING}{}## Evidence\nReceipt.\n\n## Open\noutcome: fail\n",
        long_answer()
    );
    let (written, saved) = write_and_read(run.path(), 6, &input)?;

    assert!(written.truncated, "the fixture must cross the body cap");
    assert_eq!(
        saved
            .body
            .lines()
            .filter(|line| line.trim() == "outcome: fail")
            .count(),
        1,
        "the final verdict must survive the cut exactly once"
    );
    for heading in ["Answer", "Evidence", "Open"] {
        let expected = format!("## {heading}");
        assert_eq!(
            saved.body.lines().filter(|line| *line == expected).count(),
            1,
            "the required {heading} section disappeared or was duplicated"
        );
    }
    assert_eq!(saved.meta.run, "run_wp_04a");
    assert_eq!(saved.meta.from, "research-memory");
    assert_eq!(saved.meta.to, vec!["planner"]);
    assert_eq!(saved.meta.title, "Memory handoff findings");
    Ok(())
}

#[test]
fn the_same_answer_is_cut_the_same_way_twice() -> Result<(), Box<dyn Error>> {
    let first_run = tempfile::tempdir()?;
    let second_run = tempfile::tempdir()?;
    let input = format!("{OPENING}{}", sections());

    // 2026-09-08 — fresh directories keep the attachment name stable; a retry in one directory
    // receives a numeric suffix, and that legitimate pointer difference is not cap nondeterminism.
    let (_, first) = write_and_read(first_run.path(), 7, &input)?;
    let (_, second) = write_and_read(second_run.path(), 7, &input)?;

    assert_eq!(
        first.body, second.body,
        "the same input and attachment name must produce the same visible bytes"
    );
    Ok(())
}
