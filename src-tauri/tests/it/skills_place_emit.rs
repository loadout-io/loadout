//! AC-2 dla T-18: emiter zdejmuje wszystkie czternaście pól spoza specyfikacji i mówi,
//! co zdjął.
//!
//! Jedno przeoczone pole spoza szóstki wywraca **każdą** ścieżkę spec-strict komunikatem
//! `Unexpected fields in frontmatter: …` [T5 §4.2] — czyli daje „działa u mnie" i nie działa
//! u pięciu pozostałych vendorów. Dlatego wejście tego testu niesie komplet czternastu.
//!
//! **Słabą wersją tego kryterium jest `assert!(!doc.contains("argument-hint"))`.** Przechodzi
//! ją emiter, który wypluł pusty plik, i taki, który po drodze zgubił `description` — czyli
//! jedyne pole, po którym model decyduje, czy w ogóle sięgnąć po umiejętność.
//!
//! Rozróżniają trzy rzeczy naraz: równość **listy kluczy razem z kolejnością**, długość
//! zwróconej listy zdjętych pól i obecność treści zdjętych pól w ciele.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use loadout_lib::skills::Skill;
use loadout_lib::skills::place;

const NAME: &str = "pdf";
const DESCRIPTION: &str = "Extracts text and tables from PDF files. Use it when the user \
                           points at a .pdf and asks what is inside.";
const LICENSE: &str = "Apache-2.0";
const COMPATIBILITY: &str = "Needs pdftotext on PATH.";
const ALLOWED_TOOLS: &str = "Read Grep";

const ARGUMENT_HINT: &str = "<file.pdf> [--pages N]";
const FIRST_PARAGRAPH: &str = "The first paragraph says what to do.";
const BODY: &str = "The first paragraph says what to do.\n\nThe second one gives an example.\n";

/// Zdanie, którym `context: fork` wraca do ciała [T5 §4.2].
const FORK_LINE: &str = "Run this as an isolated task.";

/// Sześć pól specyfikacji w kolejności emisji. **Wypisane literalnie**, nie wzięte
/// z `SPEC_FIELDS`: kryterium sprawdzające implementację jej własną tablicą przechodzi po
/// każdej zmianie tej tablicy, łącznie z przestawieniem dwóch pól.
const SPEC_KEYS: [&str; 6] = [
    "name",
    "description",
    "license",
    "compatibility",
    "metadata",
    "allowed-tools",
];

/// Czternaście pól, które Claude Code przyjmuje, a specyfikacja nie zna
/// [T5 §4.2 + fact-check §3]. Też literalnie, i z tego samego powodu.
const NON_SPEC: [(&str, &str); 14] = [
    ("when_to_use", "when the user names a PDF"),
    ("argument-hint", ARGUMENT_HINT),
    ("arguments", "file"),
    ("disable-model-invocation", "true"),
    ("user-invocable", "true"),
    ("disallowed-tools", "Bash"),
    ("model", "claude-opus-5"),
    ("effort", "high"),
    ("context", "fork"),
    ("agent", "pdf-reader"),
    ("background", "false"),
    ("hooks", "PostToolUse: ./scripts/run.sh"),
    ("paths", "**/*.pdf"),
    ("shell", "/bin/zsh"),
];

/// Umiejętność z kompletem: sześć pól specyfikacji i wszystkie czternaście spoza niej.
fn loaded_skill() -> Skill {
    Skill {
        name: NAME.to_owned(),
        description: DESCRIPTION.to_owned(),
        license: Some(LICENSE.to_owned()),
        compatibility: Some(COMPATIBILITY.to_owned()),
        metadata: BTreeMap::from([("team".to_owned(), "docs".to_owned())]),
        allowed_tools: Some(ALLOWED_TOOLS.to_owned()),
        body: BODY.to_owned(),
        files: Vec::new(),
        extras: NON_SPEC
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
    }
}

fn split(doc: &str) -> (&str, &str) {
    let rest = doc
        .strip_prefix("---\n")
        .expect("a SKILL.md opens with `---` on byte 0");
    let end = rest
        .find("\n---")
        .expect("the front-matter block never closes: no line reads `---` after the opening one");
    let after = &rest[end + 1..];
    let newline = after
        .find('\n')
        .expect("the closing `---` is not on a line of its own");
    (&rest[..end], &after[newline + 1..])
}

/// Klucze najwyższego poziomu, w kolejności z pliku. Wiersze wcięte i wiersze listy pomijamy:
/// pary pod `metadata` są jej **wartością**, nie kluczami front-mattera.
fn keys(doc: &str) -> Vec<String> {
    split(doc)
        .0
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with([' ', '\t', '-']))
        .filter_map(|line| line.split_once(':'))
        .map(|(key, _)| key.trim().to_owned())
        .collect()
}

fn value(doc: &str, key: &str) -> Option<String> {
    split(doc).0.lines().find_map(|line| {
        let (name, rest) = line.split_once(':')?;
        (name == key).then(|| rest.trim().to_owned())
    })
}

#[test]
fn the_front_matter_carries_exactly_the_six_spec_keys_in_that_order() {
    let (doc, _) = place::emit(&loaded_skill());

    assert_eq!(
        keys(&doc),
        SPEC_KEYS.map(ToOwned::to_owned).to_vec(),
        "the emitted front-matter is not the six spec keys in order. Order is part of the \
         contract: SKILL.md in project scope lands in the team's repo, and a header that \
         reshuffles itself on every Update turns `git diff` into noise nobody reads.\n\
         The whole block reads:\n{}",
        split(&doc).0
    );

    // Cudzysłowy zdejmujemy, bo cytowanie skalara jest wyborem emitera; treść nie jest.
    let description = value(&doc, "description").expect("`description` is not in the block");
    assert_eq!(
        description.trim_matches('"'),
        DESCRIPTION,
        "`description` came out changed. It is the one field the model reads to decide \
         whether to reach for the skill at all — rewrapped, truncated or re-worded, the skill \
         stops triggering and nothing anywhere says so"
    );
}

#[test]
fn all_fourteen_non_spec_fields_come_back_named_as_stripped() {
    let (doc, stripped) = place::emit(&loaded_skill());

    let mut got = stripped.clone();
    got.sort_unstable();
    let mut want: Vec<&str> = NON_SPEC.iter().map(|(key, _)| *key).collect();
    want.sort_unstable();

    assert_eq!(
        stripped.len(),
        14,
        "emit() reported {} stripped fields, not fourteen. `hooks` in particular runs code and \
         can arrive with a skill pasted from the internet — the emitter is the last place it \
         can be taken off. Reported: {stripped:?}",
        stripped.len()
    );
    assert_eq!(
        got, want,
        "the reported list is not the fourteen fields from T5 fact-check §3"
    );

    let emitted = keys(&doc);
    for (field, _) in NON_SPEC {
        assert!(
            !emitted.iter().any(|key| key == field),
            "`{field}` survived into the emitted front-matter, which is exactly what makes a \
             spec-strict path refuse the whole file"
        );
    }
}

#[test]
fn the_hint_and_the_fork_come_back_in_the_body_instead_of_vanishing() {
    let (doc, _) = place::emit(&loaded_skill());
    let body = split(&doc).1;

    let hint = format!("Arguments: {ARGUMENT_HINT}");
    assert!(
        body.lines().any(|line| line == hint),
        "`argument-hint` was dropped without a trace. Stripped is not deleted: the hint is the \
         only place that says how the skill is called, and the other five vendors read it as \
         prose or not at all [T5 §4.2]. The body reads:\n{body}"
    );
    assert!(
        body.lines().any(|line| line == FORK_LINE),
        "`context: fork` was dropped without a trace; it should read `{FORK_LINE}`. \
         The body reads:\n{body}"
    );

    let paragraph = body
        .find(FIRST_PARAGRAPH)
        .expect("the authored body is not in the emitted file at all");
    assert!(
        body.find(&hint).expect("hint line") < paragraph,
        "the arguments line sits after the first paragraph, so the agent reads the instructions \
         before it learns how it was called"
    );
    assert!(
        body.find(FORK_LINE).expect("fork line") < paragraph,
        "`{FORK_LINE}` sits after the first paragraph"
    );
    assert!(
        body.contains(BODY),
        "the authored body is not in the emitted file byte for byte. Folding two lines in \
         front of it is the whole change; re-wrapping the rest is not"
    );
}

#[test]
fn a_skill_with_no_optional_fields_emits_two_keys_not_six_empty_ones() {
    let bare = Skill {
        name: NAME.to_owned(),
        description: DESCRIPTION.to_owned(),
        ..Skill::default()
    };
    let (doc, stripped) = place::emit(&bare);

    assert_eq!(
        keys(&doc),
        vec!["name".to_owned(), "description".to_owned()],
        "an absent optional field came out as an empty key. `license:` with nothing after it \
         is not the same file as no `license:` line — it is a value the next reader has to \
         guess about.\nThe whole block reads:\n{}",
        split(&doc).0
    );
    assert!(
        stripped.is_empty(),
        "nothing was carried in, so nothing could be stripped, yet emit() reported {stripped:?}"
    );
}
