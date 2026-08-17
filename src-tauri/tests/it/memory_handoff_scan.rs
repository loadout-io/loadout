//! AC-5 dla T-16: katalog biegu daje się odczytać bez bazy, także gdy pliki pisał ktoś inny.
//!
//! To jest niezmiennik 4 postawiony przed sądem: „pliki są prawdą, `loadout.db` jest indeksem
//! — kasujesz bazę i nic nie ginie" [ARCHITECTURE §2 pyt. 2]. Test **wypisuje pliki jako
//! literalne stringi**, nigdy przez `write_handoff`: skan, który czyta tylko to, co sam
//! zapisał, nie odpowiada na to pytanie ani trochę.
//!
//! Drugi niezmiennik pod spodem to 5. Katalog biegu potrafi zawierać plik od starszego albo
//! nowszego Loadouta, plik po ręcznej edycji i śmieć od systemu. `serde(deny_unknown_fields)`
//! na strukturze meta przewraca skan na **jednym** takim pliku, a użytkownik widzi pustą listę
//! zamiast błędu.
//!
//! **Słabą wersją tego kryterium jest skan, który zwraca nazwy plików i przelicza całą resztę
//! z zawartości.** Przechodzi każdą asercję „pole ma sensowną wartość", bo sam te wartości
//! wylicza. Rozróżnia rekord 02: `bytes` musi pochodzić **z pliku**, a rozjazd z faktyczną
//! długością ma być zaraportowany, nie wygładzony.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use loadout_lib::memory::handoff::{self, Handoff, Kind, Status};

/// Plik 01: wszystko znane, wszystko na miejscu.
const FM_01: &str = "\
id: h_01K9F3Q0MZ
run: run_7f3a
step: 1
from: orchestrator
to: [research-auth, research-db]
kind: brief
title: What we are building
status: current
supersedes: null
reads: []
created: 2026-08-15T10:22:03Z
";

const BODY_01: &str = "\
## Answer
Two researchers, then a planner.

## Evidence
- tasks/T-16.md

## Open
- none
";

/// Plik 02: nieznany klucz w środku bloku, nieznany `kind`, i `bytes`, które kłamie.
const FM_02: &str = "\
id: h_01K9F3Q1NP
run: run_7f3a
step: 2
from: research-auth
to: [planner]
x-loadout-future: 1
kind: telepathy
title: Auth flow findings
status: superseded
supersedes: h_01K9F3Q0MZ
reads: [h_01K9F3Q0MZ]
created: 2026-08-15T10:31:02Z
";

const BODY_02: &str = "\
## Answer
The tenant is resolved before the guard runs.

## Evidence
- src/auth/tenant.middleware.ts:41

## Open
- Unclear whether the mobile client relies on the 401.
";

/// Deklarowana długość ciała pliku 02, celowo niezgodna z faktyczną.
const DECLARED_02: usize = 4242;

/// Nieznany klucz pliku 02 — z gałęzi Loadouta, której jeszcze nie ma.
const UNKNOWN_KEY: &str = "x-loadout-future";
/// Nieznany rodzaj pliku 02.
const UNKNOWN_KIND: &str = "telepathy";

/// Plik 03: opcjonalnego klucza `supersedes` nie ma w ogóle. To nie to samo, co `null`,
/// i jedno i drugie ma się odczytać jako `None`.
const FM_03: &str = "\
id: h_01K9F3Q2M7
run: run_7f3a
step: 3
from: planner
to: [implementer]
kind: plan
title: The plan
status: current
reads: [h_01K9F3Q1NP]
created: 2026-08-15T10:44:10Z
";

const BODY_03: &str = "\
## Answer
Do the middleware first.

## Evidence
- h_01K9F3Q1NP

## Open
- none
";

/// Składa plik: `---`, front-matter, dwa pola długości, `---`, pusty wiersz, ciało.
fn compose(front_matter: &str, bytes: usize, body: &str) -> String {
    format!(
        "---\n{front_matter}bytes: {bytes}\nest_tokens: {}\n---\n\n{body}",
        bytes.div_ceil(4)
    )
}

/// Zakłada katalog biegu, którego Loadout nigdy nie pisał: trzy przekazania, śmieć od Findera
/// i katalog `attachments/` z plikiem `.md`, który przekazaniem nie jest.
fn plant(run_dir: &Path) {
    let handoffs = run_dir.join("handoffs");
    std::fs::create_dir_all(&handoffs).unwrap();

    std::fs::write(
        handoffs.join("01__orchestrator__brief.md"),
        compose(FM_01, BODY_01.len(), BODY_01),
    )
    .unwrap();
    std::fs::write(
        handoffs.join("02__research-auth__findings.md"),
        compose(FM_02, DECLARED_02, BODY_02),
    )
    .unwrap();
    std::fs::write(
        handoffs.join("03__planner__plan.md"),
        compose(FM_03, BODY_03.len(), BODY_03),
    )
    .unwrap();

    // Finder zostawia to w każdym katalogu, który ktoś kiedyś otworzył. Nie jest markdownem,
    // nie jest tekstem i nie jest błędem.
    std::fs::write(handoffs.join(".DS_Store"), [0u8, 1, 2, 3]).unwrap();

    // Plik `.md` obok przekazań, który przekazaniem nie jest — rekurencyjny spacer po katalogu
    // biegu zwróciłby go jako czwarty rekord i nikt by nie zauważył, bo wygląda jak reszta.
    let attachments = run_dir.join("attachments");
    std::fs::create_dir_all(&attachments).unwrap();
    std::fs::write(
        attachments.join("02__research-auth__findings__full.md"),
        "the full text of a handoff that was cut; not a handoff itself\n",
    )
    .unwrap();
}

fn names(records: &[Handoff]) -> Vec<String> {
    records
        .iter()
        .map(|record| {
            record
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

fn scan(run_dir: &Path) -> Vec<Handoff> {
    plant(run_dir);
    handoff::scan_run_dir(run_dir).expect(
        "the scan failed on a run directory it did not write. One unreadable file must never \
         take the whole listing with it (invariant 5)",
    )
}

#[test]
fn three_handoffs_come_back_in_step_order_and_the_junk_does_not() {
    let run_dir = tempfile::tempdir().unwrap();
    let records = scan(run_dir.path());

    assert_eq!(
        names(&records),
        vec![
            "01__orchestrator__brief.md",
            "02__research-auth__findings.md",
            "03__planner__plan.md",
        ],
        "the scan returns the three handoffs in the order their names give them, and nothing \
         else. `.DS_Store` is not a handoff and neither is the `.md` file under `attachments/` \
         — a recursive walk that returns four records looks right until someone reads the \
         fourth one"
    );

    assert_eq!(
        records.first().map(|record| record.body.as_str()),
        Some(BODY_01),
        "the body is what stands in the file after the front-matter and the blank separator, \
         byte for byte"
    );
    assert_eq!(
        records.first().map(|record| record.meta.status),
        Some(Status::Current),
        "`status` is read from the file, not assumed. It is the field that decides whether a \
         handoff is still injected [T6 §9], and it has to survive `rm loadout.db`"
    );
    assert_eq!(
        records.get(1).map(|record| record.meta.status),
        Some(Status::Superseded),
        "file 02 says `status: superseded` and the scan has to say so too"
    );
}

#[test]
fn an_unknown_key_and_an_unknown_kind_are_carried_not_refused() {
    let run_dir = tempfile::tempdir().unwrap();
    let records = scan(run_dir.path());
    let second = records.get(1).expect("the scan lost file 02 entirely");

    assert_eq!(
        second.meta.kind,
        Kind::Other(UNKNOWN_KIND.to_owned()),
        "`kind: {UNKNOWN_KIND}` is not one of the seven, and that is not an error — it is a \
         file from an older or newer Loadout, or one someone edited by hand (invariant 5). It \
         is carried as itself, so the value is still on screen and still in the file"
    );
    assert_eq!(
        second.meta.extra.get(UNKNOWN_KEY).map(String::as_str),
        Some("1"),
        "the key nobody knows lands in `extra` and keeps its value. Dropping it silently means \
         rewriting this file later erases a field somebody else's Loadout depends on. `extra` \
         holds {:?}",
        second.meta.extra
    );
    assert_eq!(
        second.meta.extra.len(),
        1,
        "only the unknown key belongs in `extra`; the thirteen contract fields have their own \
         places. `extra` holds {:?}",
        second.meta.extra
    );

    assert_eq!(
        records
            .get(2)
            .and_then(|record| record.meta.supersedes.clone()),
        None,
        "file 03 has no `supersedes:` line at all. A missing optional key reads as nothing, \
         exactly like the explicit `null` in file 01 — anything else and the scan invents a \
         chain that was never written"
    );
    assert_eq!(
        records
            .first()
            .and_then(|record| record.meta.supersedes.clone()),
        None,
        "file 01 says `supersedes: null`, which is nothing"
    );
    assert_eq!(
        records.get(2).map(|record| record.meta.kind.clone()),
        Some(Kind::Plan),
        "file 03 carries a kind from the closed set of seven [T6 §10.2]"
    );
}

#[test]
fn a_file_that_lies_about_its_own_length_reports_the_gap() {
    let run_dir = tempfile::tempdir().unwrap();
    let records = scan(run_dir.path());
    let second = records.get(1).expect("the scan lost file 02 entirely");

    assert_eq!(
        second.meta.bytes, DECLARED_02,
        "`bytes` is the number that stands in the file. A scan that recomputes it from the \
         body can never disagree with anything, so every assertion about it passes and none of \
         them means a thing — this is the one field that proves the values came off disk"
    );
    assert_eq!(
        second.actual_bytes,
        second.body.len(),
        "`actual_bytes` is what the body really measures, counted at read time"
    );
    assert!(
        second.bytes_mismatch(),
        "file 02 declares {} bytes and carries {}. A truncated write, a hand edit, an older \
         Loadout — the gap is the only sign any of them left, and smoothing it over throws \
         away the sign",
        second.meta.bytes,
        second.actual_bytes
    );

    for (position, label) in [(0usize, "01"), (2usize, "03")] {
        let entry = records
            .get(position)
            .expect("the scan lost a well-formed file");
        assert!(
            !entry.bytes_mismatch(),
            "file {label} declares its own length correctly, so reporting a gap here is a \
             false alarm — and a check that cries wolf on the good files is a check people \
             switch off. It declares {} and carries {}",
            entry.meta.bytes,
            entry.actual_bytes
        );
    }
}
