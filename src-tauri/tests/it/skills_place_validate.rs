//! AC-3 dla T-18: niepoprawna umiejętność nie dotyka dysku, a komunikat brzmi jak
//! u walidatora referencyjnego.
//!
//! Cztery komunikaty poniżej są przepisane bajt w bajt z `agentskills` — jedynej wyroczni,
//! jaka istnieje — i były odtworzone na przebudowanych fikstach [T5 §6.2 i fact-check].
//! Trzymamy je dosłownie, bo użytkownik zobaczy je też wtedy, gdy odmówi sam vendor,
//! a dwa różne zdania o tej samej przyczynie to dwa różne zgłoszenia.
//!
//! **Słabą wersją tego kryterium jest `assert!(validate(..).is_err())`.** Przechodzi ją
//! walidacja, która najpierw kopiuje, a dopiero potem sprawdza — czyli zostawia po odmowie
//! katalog, którego nikt nie posprząta. Przechodzi ją też jedno wspólne „invalid skill" na
//! dziewięć różnych przyczyn, po którym nie wiadomo, co poprawić.
//!
//! Rozróżniają: równość tekstu dla czterech przypadków, **wzajemna różność** komunikatów
//! dla wszystkich dziewięciu reguł, i asercja, że po odmowie żaden z dwóch katalogów
//! docelowych nie powstał.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;
use std::fs;

use loadout_lib::skills::place;
use loadout_lib::skills::{Error, Roots, Scope, Skill, SkillDoc};

const DESCRIPTION: &str = "Extracts text and tables from PDF files.";

const MISSING_DESCRIPTION: &str = "Missing required field in frontmatter: description";
const DIR_MISMATCH: &str =
    "Directory name 'name-mismatch' must match skill name 'totally-different'";
const NOT_LOWERCASE: &str = "Skill name 'Upper-Name' must be lowercase";
const UNEXPECTED_FIELDS: &str = concat!(
    "Unexpected fields in frontmatter: argument-hint, context, disable-model-invocation. ",
    "Only ['allowed-tools', 'compatibility', 'description', 'license', 'metadata', 'name'] ",
    "are allowed.",
);

fn doc(fields: &[(&str, &str)]) -> SkillDoc {
    SkillDoc {
        fields: fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
        body: "Do the thing.\n".to_owned(),
    }
}

/// Poprawny dokument o zadanej nazwie — punkt wyjścia dla reguł, które psują jedno pole.
fn named(name: &str) -> SkillDoc {
    doc(&[("name", name), ("description", DESCRIPTION)])
}

fn messages(dir_name: &str, document: &SkillDoc) -> Vec<String> {
    place::validate_strict(dir_name, document)
        .err()
        .unwrap_or_default()
}

/// Komunikaty odmowy sklejone w jedno zdanie — albo zdanie o tym, że odmowy nie było.
fn refusal(dir_name: &str, document: &SkillDoc) -> String {
    match place::validate_strict(dir_name, document) {
        Ok(()) => format!("<accepted '{dir_name}'>"),
        Err(said) => said.join(" | "),
    }
}

#[test]
fn the_four_verified_messages_come_back_word_for_word() {
    assert_eq!(
        messages("pdf", &doc(&[("name", "pdf")])),
        vec![MISSING_DESCRIPTION.to_owned()],
        "a skill with no `description` is a skill the model has no reason to ever load, and \
         the reference validator says so in exactly these words"
    );

    assert_eq!(
        messages("name-mismatch", &named("totally-different")),
        vec![DIR_MISMATCH.to_owned()],
        "the directory name is what the user types as `/name`; when it disagrees with `name` \
         in the file, Claude Code takes the directory and the listing takes the field"
    );

    // Tu jedyny raz `contains`, nie równość: `Upper-Name` łamie także wyrażenie
    // `^[a-z0-9]+(-[a-z0-9]+)*$`, więc drugi komunikat o tym samym wejściu jest uczciwy.
    assert!(
        messages("Upper-Name", &named("Upper-Name")).contains(&NOT_LOWERCASE.to_owned()),
        "an uppercase name has to be refused in these words; it came back as {:?}",
        messages("Upper-Name", &named("Upper-Name"))
    );

    let with_extras = doc(&[
        ("name", "pdf"),
        ("description", DESCRIPTION),
        ("argument-hint", "<file.pdf>"),
        ("context", "fork"),
        ("disable-model-invocation", "true"),
    ]);
    assert_eq!(
        messages("pdf", &with_extras),
        vec![UNEXPECTED_FIELDS.to_owned()],
        "three Claude-only fields have to be named one by one, with the allowed six listed \
         after them. This is the message the other five vendors' spec-strict paths produce, \
         and it is the difference between `works on my machine` and works"
    );
}

#[test]
fn nine_causes_get_nine_different_messages() {
    let long_name = "a".repeat(65);
    let long_description = "d".repeat(1025);
    let long_compatibility = "c".repeat(501);

    let over_long_description = doc(&[("name", "pdf"), ("description", &long_description)]);
    let over_long_compatibility = doc(&[
        ("name", "pdf"),
        ("description", DESCRIPTION),
        ("compatibility", &long_compatibility),
    ]);
    let with_extras = doc(&[
        ("name", "pdf"),
        ("description", DESCRIPTION),
        ("argument-hint", "<file.pdf>"),
        ("context", "fork"),
        ("disable-model-invocation", "true"),
    ]);

    let cases = [
        ("no description", refusal("pdf", &doc(&[("name", "pdf")]))),
        (
            "directory disagrees with name",
            refusal("name-mismatch", &named("totally-different")),
        ),
        (
            "name is not lowercase",
            refusal("Upper-Name", &named("Upper-Name")),
        ),
        ("fields outside the six", refusal("pdf", &with_extras)),
        ("name over 64", refusal(&long_name, &named(&long_name))),
        (
            "description over 1024",
            refusal("pdf", &over_long_description),
        ),
        (
            "compatibility over 500",
            refusal("pdf", &over_long_compatibility),
        ),
        (
            "name carries a reserved word",
            refusal("claude-pdf", &named("claude-pdf")),
        ),
        (
            "the folder is the reserved one",
            refusal("synced", &named("synced")),
        ),
    ];

    for (cause, said) in &cases {
        assert!(
            !said.starts_with("<accepted"),
            "`{cause}` was accepted. Every one of these nine is a skill that installs cleanly \
             and then does not work, which is the failure this whole task exists to prevent"
        );
    }

    let distinct: HashSet<&String> = cases.iter().map(|(_, said)| said).collect();
    assert_eq!(
        distinct.len(),
        cases.len(),
        "two of the nine causes share a message, so the person reading it cannot tell which \
         rule they broke. One cause, one sentence:\n{}",
        cases
            .iter()
            .map(|(cause, said)| format!("  {cause}: {said}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_reserved_folder_is_refused_in_any_capitalisation() {
    // Claude Code pomija folder `synced` niezależnie od wielkości liter [T5 fact-check,
    // „Worth adding"], więc umiejętność w nim jest poprawna, zainstalowana i niewidoczna.
    //
    // Oba wejścia łamią dokładnie te same reguły nazwy (wielkie litery, wyrażenie), więc
    // różnicę robi wyłącznie reguła zarezerwowanego folderu. Porównujemy liczbę komunikatów,
    // nie ich treść: treść cytuje nazwę, a nazwy z definicji się różnią.
    let reserved = messages("SYNCED", &named("SYNCED"));
    let control = messages("NOTSYNC", &named("NOTSYNC"));

    assert!(
        reserved.len() > control.len(),
        "`SYNCED` and `NOTSYNC` were refused for exactly the same reasons, so the reserved \
         folder rule only looks at lowercase. A skill written in `Synced/` installs, validates \
         and is skipped in silence.\n  SYNCED:  {reserved:?}\n  NOTSYNC: {control:?}"
    );
}

/// Umiejętności, których `plan()` nie ma prawa przyjąć — każda łamie jedną regułę.
fn refused_skills() -> Vec<(&'static str, Skill)> {
    let valid = |name: &str| Skill {
        name: name.to_owned(),
        description: DESCRIPTION.to_owned(),
        ..Skill::default()
    };
    vec![
        ("name is not lowercase", valid("Upper-Name")),
        ("name over 64", valid(&"a".repeat(65))),
        ("name carries a reserved word", valid("claude-pdf")),
        ("the folder is the reserved one", valid("synced")),
        (
            "no description",
            Skill {
                description: String::new(),
                ..valid("pdf")
            },
        ),
        (
            "description over 1024",
            Skill {
                description: "d".repeat(1025),
                ..valid("pdf")
            },
        ),
        (
            "compatibility over 500",
            Skill {
                compatibility: Some("c".repeat(501)),
                ..valid("pdf")
            },
        ),
    ]
}

#[test]
fn a_refused_skill_leaves_neither_destination_behind() {
    for (cause, skill) in refused_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let roots = Roots {
            home: home.clone(),
            project: None,
            data: tmp.path().join("data"),
        };

        let outcome = place::plan(&skill, Scope::Global, &roots);
        assert!(
            matches!(&outcome, Err(Error::Invalid { .. })),
            "`{cause}` came back as {outcome:?}. A refusal has to be a refusal to validate — \
             an io error means the write had already started"
        );

        for vendor in [".claude", ".agents"] {
            let path = home.join(vendor).join("skills");
            assert!(
                fs::symlink_metadata(&path).is_err(),
                "`{cause}` was refused, and {} exists anyway. Validation runs before the first \
                 write, not halfway through it: a directory created `to check permissions` is \
                 a directory nobody cleans up",
                path.display()
            );
        }
    }
}

/// 2026-08-22 — REGUŁA WYDAWNICZA NIE MA PRAWA WYWRACAĆ BIEGU.
///
/// Bieg właściciela stanął na zdaniu „its SKILL.md could not be read as a skill" dla pliku,
/// który jest całkowicie poprawną umiejętnością Claude Code — miał tylko `user-invocable: false`
/// w nagłówku. Krok pytał `validate_strict`, a ta odpowiada na pytanie „czy wolno nam to
/// ZAPISAĆ", nie „czy da się tego UŻYĆ". W bibliotece właściciela dwanaście z czternastu
/// zaimportowanych umiejętności niosło takie pole, więc każda wywracała bieg przy pierwszym
/// sięgnięciu po nią.
#[test]
fn a_vendor_field_does_not_stop_the_run_but_still_stops_a_publish() {
    let with_a_vendor_field = doc(&[
        ("name", "design-system-reference"),
        ("description", DESCRIPTION),
        ("user-invocable", "false"),
    ]);

    assert_eq!(
        place::validate_usable("design-system-reference", &with_a_vendor_field),
        Ok(()),
        "an unknown key is not an error when we are READING somebody else's file (invariant 5); \
         the import copies SKILL.md byte for byte on purpose, so refusing here made the two \
         halves of the product disagree about the same file"
    );
    assert!(
        place::validate_strict("design-system-reference", &with_a_vendor_field).is_err(),
        "and the publishing rule stays exactly as strict: what Loadout writes itself carries \
         spec fields only"
    );
}

/// Czego „da się użyć" nie przepuszcza — bo inaczej byłoby zgodą na wszystko.
#[test]
fn a_file_that_is_not_a_skill_is_still_turned_down_at_the_step() {
    for (cause, document, dir_name) in [
        ("no description at all", doc(&[("name", "pdf")]), "pdf"),
        (
            "a name that disagrees with its folder",
            named("totally-different"),
            "name-mismatch",
        ),
        (
            "the folder Claude Code skips in silence",
            named("synced"),
            "synced",
        ),
    ] {
        assert!(
            place::validate_usable(dir_name, &document).is_err(),
            "`{cause}` was accepted as usable. Each one of these installs cleanly and then does \
             not work, which is the failure this whole rule exists to prevent"
        );
    }
}
