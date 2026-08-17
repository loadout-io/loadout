//! AC-3 dla T-16: trzy sekcje o stałych nazwach, w stałej kolejności, nic z treści nie ginie.
//!
//! Kontrakt [T6 §10.2] mówi: „jeśli agent pominie nagłówek, Loadout wstawia go pusty i oznacza
//! krok". Naprawa jest tania, ale ma dokładnie jeden trudny warunek — **nie wolno przy niej
//! zgubić ani jednego znaku tego, co agent napisał.** Model, który dostał instrukcję o trzech
//! sekcjach, i tak co jakiś czas przyśle samą prozę albo sekcje w swojej kolejności [T6 §11.1].
//!
//! **Słabą wersją tego kryterium jest `assert!(file.contains("## Open"))`.** Przechodzi ją
//! implementacja doklejająca brakujące nagłówki na koniec, w dowolnej kolejności — czyli taka,
//! po której `## Answer` bywa trzecie, a następny agent czyta odpowiedź dopiero po dowodach.
//! Przechodzi ją także taka, która przy przestawianiu sekcji gubi ich treść: nagłówki są,
//! plik wygląda poprawnie, a zdania nie ma.
//!
//! Rozróżniają dwie rzeczy: porównanie **offsetów bajtowych** trzech nagłówków oraz asercja,
//! że treść każdej sekcji leży między swoim nagłówkiem a następnym.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use loadout_lib::memory::handoff::{self, Kind, MetaDraft, Section};

/// Ciało A: sama proza, zero nagłówków. Najczęstsza rzecz, jaką przyśle model [T6 §11.1].
const BODY_A: &str = "\
Login goes through TenantMiddleware before AuthGuard, so an unresolved
tenant surfaces as 401, not 400.
";

/// Ciało B: są dwa nagłówki, w odwrotnej kolejności, i nie ma trzeciego.
const BODY_B: &str = "\
## Evidence
- src/auth/tenant.middleware.ts:41 -- resolve() throws before the guard runs

## Answer
The tenant is resolved before the guard runs, so the 401 is the correct code.
";

/// Zdania z ciała B, każde ze swojej sekcji. Asercje pytają, czy zostały przy swoim nagłówku.
const B_EVIDENCE_LINE: &str =
    "- src/auth/tenant.middleware.ts:41 -- resolve() throws before the guard runs";
const B_ANSWER_LINE: &str =
    "The tenant is resolved before the guard runs, so the 401 is the correct code.";

/// Ciało C: komplet trzech sekcji we właściwej kolejności. Nic do naprawy.
const BODY_C: &str = "\
## Answer
The tenant is resolved before the guard runs.

## Evidence
- src/auth/tenant.middleware.ts:41

## Open
- Unclear whether the mobile client relies on the 401.
";

fn draft(step: u32) -> MetaDraft {
    MetaDraft {
        run: "run_7f3a".to_owned(),
        step,
        from: "research-auth".to_owned(),
        to: vec!["planner".to_owned()],
        kind: Kind::Findings,
        title: "Auth flow findings".to_owned(),
        reads: vec![],
    }
}

/// Ciało zapisanego pliku: wszystko za zamykającym `---` i pustym wierszem separatora.
fn body_of(file: &str) -> &str {
    assert!(
        file.starts_with("---\n"),
        "a handoff opens with `---` on byte 0"
    );
    let mut at = 4;
    let mut body = None;
    while at < file.len() {
        let end = file[at..].find('\n').map_or(file.len(), |i| at + i + 1);
        if file[at..end].trim_end() == "---" {
            body = file.get(end + 1..);
            break;
        }
        at = end;
    }
    body.expect("the front-matter block never closes, or nothing follows it")
}

/// Offset wiersza, który jest **dokładnie** nagłówkiem `## <name>`. Po wierszach, nie po
/// podłańcuchu: `## Answer` zacytowane w środku zdania nie jest nagłówkiem.
fn heading_at(body: &str, name: &str) -> Option<usize> {
    let head = format!("## {name}");
    let mut at = 0;
    while at < body.len() {
        let end = body[at..].find('\n').map_or(body.len(), |i| at + i + 1);
        if body[at..end].trim_end() == head {
            return Some(at);
        }
        at = end;
    }
    None
}

/// Offsety trzech nagłówków, z komunikatem nazywającym ten, którego brakuje.
fn headings(body: &str) -> (usize, usize, usize) {
    let mut found = Vec::new();
    for name in ["Answer", "Evidence", "Open"] {
        let at = heading_at(body, name);
        assert!(
            at.is_some(),
            "`## {name}` is not in the written body. The three names are fixed [T6 §10.2] — a \
             missing one is a section the next agent will never think to look for. The body \
             reads:\n{body}"
        );
        found.push(at.unwrap_or_default());
    }
    (found[0], found[1], found[2])
}

/// Treść sekcji: wszystko między jej nagłówkiem a następnym nagłówkiem `## ` albo końcem ciała.
fn section_body<'a>(body: &'a str, name: &str) -> &'a str {
    let Some(at) = heading_at(body, name) else {
        return "";
    };
    let after_head = body[at..].find('\n').map_or(body.len(), |i| at + i + 1);
    let rest = &body[after_head..];
    if rest.starts_with("## ") {
        return "";
    }
    let end = rest.find("\n## ").map_or(rest.len(), |i| i + 1);
    &rest[..end]
}

fn write(run_dir: &Path, step: u32, body: &str) -> (handoff::Written, String) {
    let written = handoff::write_handoff(run_dir, draft(step), body)
        .expect("write_handoff refused a body well under the cap");
    let file = std::fs::read_to_string(&written.path)
        .expect("write_handoff reported a path that cannot be read back");
    (written, file)
}

#[test]
fn prose_with_no_headings_becomes_answer_and_two_empty_sections() {
    let run_dir = tempfile::tempdir().unwrap();
    let (written, file) = write(run_dir.path(), 2, BODY_A);
    let body = body_of(&file);

    assert_eq!(
        written.repaired,
        vec![Section::Answer, Section::Evidence, Section::Open],
        "the agent sent no headings at all, so all three were inserted — and the write says so, \
         because that counter is the only way anyone learns how often the shape has to be \
         repaired [T6 §11.1]"
    );

    let (answer, evidence, open) = headings(body);
    assert!(
        answer < evidence && evidence < open,
        "the three headings run Answer, Evidence, Open by byte offset; they sit at {answer}, \
         {evidence} and {open}. Appending what is missing to the end of the file passes every \
         `contains` check and still puts the answer after the evidence"
    );

    let prose = BODY_A.trim_end();
    let at = body.find(prose);
    assert!(
        at.is_some(),
        "the agent's prose is not in the body, character for character. Repairing the shape is \
         not licence to rewrite the text. The body reads:\n{body}"
    );
    let at = at.unwrap_or_default();
    assert!(
        at > answer && at + prose.len() <= evidence,
        "the prose sits at byte {at}, outside `## Answer` (which runs from {answer} to \
         {evidence}). Prose with no heading is the answer — that is the only section it can \
         belong to"
    );

    for name in ["Evidence", "Open"] {
        assert!(
            section_body(body, name).trim().is_empty(),
            "`## {name}` was inserted empty, because the agent wrote nothing for it. It holds \
             {:?} instead, which means text was moved out of Answer and into a section the \
             agent never wrote",
            section_body(body, name)
        );
    }
}

#[test]
fn sections_in_the_wrong_order_are_reordered_and_keep_their_own_text() {
    let run_dir = tempfile::tempdir().unwrap();
    let (written, file) = write(run_dir.path(), 3, BODY_B);
    let body = body_of(&file);

    assert_eq!(
        written.repaired,
        vec![Section::Open],
        "two of the three headings arrived, so exactly one was inserted. Reporting more means \
         the write cannot tell what the agent sent from what it added"
    );

    let (answer, evidence, open) = headings(body);
    assert!(
        answer < evidence && evidence < open,
        "the agent sent Evidence before Answer; the file runs Answer, Evidence, Open. They sit \
         at {answer}, {evidence} and {open}"
    );

    let answer_line = body.find(B_ANSWER_LINE);
    let evidence_line = body.find(B_EVIDENCE_LINE);
    assert!(
        answer_line.is_some() && evidence_line.is_some(),
        "reordering the sections dropped one of their bodies. This is the failure that leaves a \
         file with all three headings and none of the text. The body reads:\n{body}"
    );
    let answer_line = answer_line.unwrap_or_default();
    let evidence_line = evidence_line.unwrap_or_default();

    assert!(
        answer < answer_line && answer_line < evidence,
        "the answer text sits at byte {answer_line}; `## Answer` runs from {answer} to \
         {evidence}. Text that moves without its heading is text attributed to the wrong \
         section, which is worse than text that is missing"
    );
    assert!(
        evidence < evidence_line && evidence_line < open,
        "the evidence text sits at byte {evidence_line}; `## Evidence` runs from {evidence} to \
         {open}"
    );
}

#[test]
fn a_body_that_already_has_the_shape_is_written_untouched() {
    let run_dir = tempfile::tempdir().unwrap();
    let (written, file) = write(run_dir.path(), 4, BODY_C);

    assert!(
        written.repaired.is_empty(),
        "all three sections arrived in order, so nothing was repaired. Reporting a repair that \
         did not happen makes the counter that watches [T6 §11.1] useless. It reported {:?}",
        written.repaired
    );
    assert_eq!(
        body_of(&file),
        BODY_C,
        "nothing needed repairing and nothing was over the cap, so the body is the agent's \
         bytes: no re-wrapping, no inserted blank lines, no normalised bullets"
    );
}
