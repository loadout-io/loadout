//! AC-2 dla T-16: limit 8 KB tnie na granicy sekcji, a pełny tekst ląduje w `attachments/`.
//!
//! Cicha porażka, przed którą to stoi, jest opisana w [T6 §11.2]: ciało ucięte w połowie zdania
//! na 8192 bajcie przechodzi każdy test na „≤ 8 KB" i gubi dokładnie to jedno zdanie, dla
//! którego przekazanie powstało. Plik dalej wygląda idealnie.
//!
//! **Słabą wersją tego kryterium jest `assert!(body.len() <= 8192 && attachment.exists())`.**
//! Przechodzi ją cięcie w połowie słowa. Przechodzi ją attachment zawierający tekst **już
//! ucięty**, czyli plik, w którym zgubionego zdania nie ma nigdzie. I przechodzi ją
//! implementacja „zawsze pisz attachment", która przy każdym mieszczącym się ciele zostawia
//! na dysku kopię, której nikt nigdy nie otworzy (niezmiennik 21).
//!
//! Rozróżniają cztery rzeczy: nieobecność znacznika ze środka `Evidence` w zapisanym pliku,
//! obecność nagłówka `## Evidence` z jednym wierszem wskaźnika pod nim, bajtowa równość
//! attachmentu z **pełnym oryginałem**, i przypadek trzeci — ciało równo 8192 B, na którym
//! „zawsze pisz attachment" pada.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use loadout_lib::memory::handoff::{self, Kind, MetaDraft, Written};

/// Znacznik w środku sekcji `Evidence`. Jego nieobecność w zapisanym pliku jest dowodem, że
/// cięcie poszło po granicy sekcji, a nie „gdzieś koło 8192".
const EVIDENCE_MARK: &str = "MARK-MIDDLE-OF-EVIDENCE-6c1f";
/// To samo dla przypadku drugiego: znacznik głęboko w jedynej sekcji.
const ANSWER_MARK: &str = "MARK-DEEP-INSIDE-ANSWER-9d20";

/// Wiersz wypełniacza. **Numerowany**, bo AC-2 pyta, czy cięcie trafiło w granicę wiersza —
/// a to widać dopiero wtedy, gdy każdy wiersz da się rozpoznać z osobna.
fn filler(name: &str, n: usize) -> String {
    format!("{name} line {n:04} ....................\n")
}

/// Sekcja o **dokładnie** `total` bajtach, z opcjonalnym znacznikiem w wierszu mniej więcej
/// w połowie. Ostatni wiersz to same kropki: dopycha sekcję do bajta i nadal jest pełnym
/// wierszem, więc granica wiersza istnieje wszędzie tam, gdzie cięcie mogłoby wypaść.
fn section(name: &str, total: usize, mark: Option<&str>) -> String {
    let mut out = format!("## {name}\n");
    assert!(total > out.len(), "a section shorter than its own heading");

    let mut pending = mark;
    let mut n = 0usize;
    loop {
        let (line, is_mark) = match pending {
            Some(m) if out.len() >= total / 2 => (format!("{m}\n"), true),
            _ => (filler(name, n), false),
        };
        if out.len() + line.len() > total {
            break;
        }
        out.push_str(&line);
        if is_mark {
            pending = None;
        } else {
            n += 1;
        }
    }
    assert!(pending.is_none(), "the marker did not fit into the section");

    let left = total - out.len();
    if left > 0 {
        out.push_str(&".".repeat(left - 1));
        out.push('\n');
    }
    assert_eq!(out.len(), total, "the fixture did not hit its byte target");
    out
}

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

/// Offset wiersza, który jest **dokładnie** nagłówkiem `## <name>`. Wyszukiwanie po wierszach,
/// nie po podłańcuchu: `## Answer` w środku zdania nie jest nagłówkiem.
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

/// Treść sekcji: wszystko między jej nagłówkiem a następnym nagłówkiem `## ` albo końcem ciała.
fn section_body<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let at = heading_at(body, name)?;
    let after_head = body[at..].find('\n').map_or(body.len(), |i| at + i + 1);
    let rest = &body[after_head..];
    if rest.starts_with("## ") {
        return Some("");
    }
    let end = rest.find("\n## ").map_or(rest.len(), |i| i + 1);
    Some(&rest[..end])
}

/// Attachment: ścieżka i wiersz wskaźnika, który ma na niego wskazywać.
///
/// Wzór wskaźnika jest wprost z AC-2 — `Moved to attachments/<plik>`, dokładnie jeden wiersz —
/// a nazwa pliku to `<stem przekazania>__full.md`.
fn attachment_of(written: &Written, run_dir: &Path) -> (PathBuf, String) {
    let attachment = written
        .attachment
        .clone()
        .expect("the body went over the cap, so the full text has to be somewhere on disk");
    let name = attachment
        .file_name()
        .and_then(|n| n.to_str())
        .expect("the attachment path has no file name")
        .to_owned();
    let stem = written
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("the handoff path has no file stem")
        .to_owned();

    assert_eq!(
        name,
        format!("{stem}__full.md"),
        "the attachment is named after the handoff it belongs to, so a person reading the run \
         directory can pair them without opening either"
    );
    assert_eq!(
        std::fs::canonicalize(attachment.parent().unwrap_or(run_dir)).unwrap(),
        std::fs::canonicalize(run_dir.join("attachments")).unwrap(),
        "the full text lives in the run's `attachments/` directory [ARCHITECTURE §8]"
    );

    (attachment, format!("Moved to attachments/{name}"))
}

#[test]
fn whole_sections_survive_and_everything_after_the_cut_moves_to_the_attachment() {
    let run_dir = tempfile::tempdir().unwrap();

    let answer = section("Answer", 3_000, None);
    let evidence = section("Evidence", 6_000, Some(EVIDENCE_MARK));
    let open = section("Open", 200, None);
    let input = format!("{answer}{evidence}{open}");
    assert_eq!(
        input.len(),
        9_200,
        "the fixture is meant to be over the cap"
    );

    let written = handoff::write_handoff(run_dir.path(), draft(2), &input)
        .expect("write_handoff refused an over-cap body instead of cutting it");
    let file = std::fs::read_to_string(&written.path).unwrap();
    let body = body_of(&file);

    assert!(
        written.truncated,
        "a 9 200 byte body does not fit under {}, and a write that does not say so leaves \
         nobody to watch the counter [T6 §11.2]",
        handoff::BODY_CAP
    );

    // 1. Sekcja, która się zmieściła, jest w pliku CAŁA i bez zmian.
    assert!(
        body.starts_with(&answer),
        "`## Answer` fits under the cap on its own, so it survives byte for byte. The body \
         starts with {:?}",
        body.get(..80).unwrap_or(body)
    );

    // 2. Sekcja, która się nie zmieściła, zostaje nagłówkiem i JEDNYM wierszem wskaźnika.
    let (attachment, pointer) = attachment_of(&written, run_dir.path());
    for name in ["Evidence", "Open"] {
        let content = section_body(body, name);
        assert!(
            content.is_some(),
            "`## {name}` is missing from the written body. Cutting a section away together with \
             its heading leaves the next agent with no sign that anything was ever there"
        );
        let lines: Vec<&str> = content
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        assert_eq!(
            lines,
            vec![pointer.as_str()],
            "under `## {name}` there is exactly one line and it is the pointer to the full \
             text. Anything else is either content that was lost or a section that quietly \
             says nothing"
        );
    }

    // 3. Treść, która nie weszła, NIE jest w pliku — ani w kawałku.
    assert_eq!(
        file.matches(EVIDENCE_MARK).count(),
        0,
        "a marker from the middle of `## Evidence` is still in the written file, so the cut \
         landed somewhere around byte {} rather than on a section boundary",
        handoff::BODY_CAP
    );

    // 4. Attachment trzyma PEŁEN oryginał, nie to, co zostało po cięciu.
    let stored = std::fs::read(&attachment).unwrap();
    assert!(
        stored == input.as_bytes(),
        "the attachment holds the agent's body byte for byte — that is the whole point of \
         writing it. It holds {} bytes and the body was {}",
        stored.len(),
        input.len()
    );

    // 5. Liczby w front-matterze opisują to, co zapisaliśmy.
    let meta = handoff::read_handoff(&written.path).unwrap().meta;
    assert_eq!(
        meta.bytes,
        body.len(),
        "`bytes` is the length of the body that landed on disk, not of what the agent sent"
    );
    assert!(
        meta.bytes <= handoff::BODY_CAP,
        "the written body is {} bytes, over the cap of {}",
        meta.bytes,
        handoff::BODY_CAP
    );
    assert_eq!(
        meta.est_tokens,
        meta.bytes.div_ceil(4),
        "`est_tokens` is derived from `bytes`, ~4 bytes per unit [T6 §10.2]"
    );
}

#[test]
fn a_single_oversized_section_is_cut_at_a_line_boundary() {
    let run_dir = tempfile::tempdir().unwrap();

    let input = section("Answer", 9_000, Some(ANSWER_MARK));
    let written = handoff::write_handoff(run_dir.path(), draft(3), &input)
        .expect("write_handoff refused a single over-cap section");
    let file = std::fs::read_to_string(&written.path).unwrap();
    let body = body_of(&file);

    assert!(
        written.truncated,
        "a 9 000 byte section does not fit under the cap"
    );

    let (attachment, pointer) = attachment_of(&written, run_dir.path());
    let content = section_body(body, "Answer");
    assert!(
        content.is_some(),
        "`## Answer` is missing from the written body"
    );
    let content = content.unwrap_or_default();

    let pointer_line = format!("{pointer}\n");
    assert!(
        content.ends_with(&pointer_line),
        "the pointer is the last line of the cut section, on its own line and with nothing \
         between it and the last line that was kept. The section ends with {:?}",
        content
            .get(content.len().saturating_sub(120)..)
            .unwrap_or(content)
    );
    let kept = content.strip_suffix(&pointer_line).unwrap_or_default();

    let original = section_body(&input, "Answer").unwrap_or_default();
    assert!(
        original.starts_with(kept),
        "what was kept is the beginning of what the agent wrote, unchanged. It is not: the file \
         holds {} bytes of `## Answer` that do not open the original",
        kept.len()
    );
    assert!(
        kept.ends_with('\n'),
        "the cut lands on a line boundary. This one ends mid-line, on {:?} — that is the \
         half-sentence failure this criterion exists for",
        kept.lines().next_back().unwrap_or_default()
    );
    assert!(
        kept.len() < original.len(),
        "nothing was cut at all, so the body cannot be under the cap"
    );
    assert_eq!(
        file.matches(ANSWER_MARK).count(),
        0,
        "the marker sits past the cut, so it cannot be in the written file"
    );

    let stored = std::fs::read(&attachment).unwrap();
    assert!(
        stored == input.as_bytes(),
        "the attachment holds the full 9 000 byte section, not the cut one. It holds {} bytes",
        stored.len()
    );

    let meta = handoff::read_handoff(&written.path).unwrap().meta;
    assert!(
        meta.bytes <= handoff::BODY_CAP && meta.bytes == body.len(),
        "`bytes` is {} for a body of {} and a cap of {}",
        meta.bytes,
        body.len(),
        handoff::BODY_CAP
    );
}

#[test]
fn a_body_exactly_at_the_cap_writes_no_attachment() {
    let run_dir = tempfile::tempdir().unwrap();

    // 3 000 + 4 000 + 1 192 = 8 192, komplet sekcji we właściwej kolejności, więc AC-3 niczego
    // tu nie dopisuje i długość ciała jest dokładnie limitem. Ta asercja przypina też samą
    // stałą: 8 KB to ~2 000 jednostek długości, ~1% okna 200k [T6 §10.2, §13 pyt. 2].
    let input = format!(
        "{}{}{}",
        section("Answer", 3_000, None),
        section("Evidence", 4_000, None),
        section("Open", 1_192, None)
    );
    assert_eq!(
        input.len(),
        handoff::BODY_CAP,
        "the fixture sits on exactly 8 192 bytes; moving BODY_CAP moves what every downstream \
         step is allowed to be told"
    );

    let written = handoff::write_handoff(run_dir.path(), draft(4), &input)
        .expect("write_handoff refused a body that is exactly at the cap");

    assert!(
        !written.truncated,
        "a body of exactly {} bytes is not over the cap, and reporting it as cut sends every \
         reader looking for text that was never removed",
        handoff::BODY_CAP
    );
    assert_eq!(
        written.attachment, None,
        "nothing was cut, so there is nothing for an attachment to hold. Writing one anyway is \
         an artefact no script ever reads (invariant 21) — and it is exactly what an \
         implementation that always writes the attachment does"
    );

    let stray: Vec<PathBuf> = std::fs::read_dir(run_dir.path().join("attachments"))
        .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    assert!(
        stray.is_empty(),
        "no attachment was reported, yet the run directory holds {stray:?}"
    );

    let file = std::fs::read_to_string(&written.path).unwrap();
    assert_eq!(
        body_of(&file),
        input,
        "nothing was over the cap, so the body is the agent's bytes unchanged"
    );

    let meta = handoff::read_handoff(&written.path).unwrap().meta;
    assert_eq!(
        meta.bytes,
        handoff::BODY_CAP,
        "`bytes` is the length of the body on disk"
    );
    assert_eq!(
        meta.est_tokens,
        meta.bytes.div_ceil(4),
        "`est_tokens` is derived from `bytes`"
    );
}
