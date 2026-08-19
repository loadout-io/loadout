//! AC-4 dla T-54: do promptu jedzie wyłącznie `## Recurring patterns` — a naiwne szukanie
//! trafia w zdanie *o* nich.
//!
//! **Słabą wersją tego kryterium jest `assert!(!wynik.is_empty())`** albo samo „wynik zawiera
//! znacznik z sekcji patterns". Oba przechodzą dla implementacji, która zwraca **cały plik** —
//! bo cały plik zawiera to zdanie. Przechodzą też dla implementacji z naiwnym
//! `text.find("## Recurring patterns")`, jeśli fikstura nie ma cytatu blokowego z trzeciej
//! linii: wtedy wycięcie jest przypadkiem poprawne, a test milczy o pułapce, która na prawdziwym
//! pliku daje **131 bajtów zdania o regułach zamiast 1701 bajtów reguł** [zmierzone
//! u gospodarza 2026-08-19 na `backend-dev.md`].
//!
//! Rozróżniają to trzy asercje naraz: znacznik z journalu, którego w wyniku być nie może; próg
//! 20% bajtów, którego „cały plik" nie przechodzi przy dziesięciokrotnie dłuższym journalu; oraz
//! sam cytat blokowy w fiksturze, bez którego „wynik zawiera znacznik" jest spełnialne przez
//! trafienie w to zdanie. Czwarta pułapka jest po drugiej stronie: implementacja porównująca
//! nagłówek **dosłownie** (`line == "## Recurring patterns"`) przechodzi punkt (e), bo nigdy nic
//! nie znajduje — i dlatego przyrostek w nagłówku fikstury nie jest ozdobą. Nagłówka równego
//! dosłownie `## Recurring patterns` nie ma w żadnym z dziesięciu plików gospodarza.
//!
//! BUDŻET, czyli po co to w ogóle jest [zmierzone u gospodarza 2026-08-19]: `backend-dev.md` to
//! **1701 z 32922 bajtów (5,2%)**, `orchestrator.md` **2016 z 73258 bajtów (2,8%)**. Reszta
//! pliku, do 73 KB `## Run journal`, nigdy nie wchodzi do budżetu tokenów — i to jest cała
//! różnica między wstrzykiwaczem a wklejeniem pliku.
//!
//! JEDEN `#[test]`: zaślepka zwracająca pusty napis przechodzi punkt (e) — rozbity na osobne
//! zestawy dałby w warstwie `before` obraz „w połowie zielony". Przypadek pozytywny stoi więc
//! pierwszy.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use loadout_lib::inherit::scan;

const PATTERNS_MARKER: &str = "PATTERNS-ONLY-9ac7";
const JOURNAL_MARKER: &str = "JOURNAL-ONLY-4b21";

/// Trzecia linia każdego z dziewięciu plików ról u gospodarza: cytat blokowy, w którym stoi
/// **dosłownie** `` `## Recurring patterns` `` — przed prawdziwym nagłówkiem. To jest cała
/// pułapka tego kryterium.
const QUOTE_ABOUT_THE_SECTION: &str = "> Auto-loaded by ship-task orchestrator. `## Recurring patterns` is BINDING and the rest of this file is not.\n";

/// Jeden wiersz journalu. Powtórzony, bo liczy się jego DŁUGOŚĆ, nie treść: to on odpowiada za
/// stosunek 1701 do 32922 bajtów, po którym poznaje się wstrzykiwacz od wklejenia pliku.
const JOURNAL_LINE: &str = "2026-08-02 — one more entry that nobody reads twice, and that is exactly why the journal is the part which must never reach the prompt.\n";

/// Prawdziwy nagłówek niesie przyrostek. Nagłówka równego dosłownie `## Recurring patterns` nie
/// ma w żadnym z dziesięciu plików gospodarza [zmierzone 2026-08-19].
const REAL_HEADING: &str = "## Recurring patterns (BINDING — do NOT repeat)\n";

/// Plik roli rozbity na części, żeby „journal jest co najmniej dziesięć razy dłuższy" dało się
/// sprawdzić, a nie tylko zadeklarować.
struct Learnings {
    whole: String,
    patterns: String,
    journal: String,
}

fn learnings() -> Learnings {
    let patterns = format!(
        "\n- {PATTERNS_MARKER}: a migration that drops a column is never additive.\n\
         - A std mutex is never held across an await.\n\n"
    );

    // Journal jest długi, bo próg 20% bajtów ma być prawdziwy, a nie dobrany do fikstury:
    // u gospodarza ta sekcja dochodzi do 73 KB na jeden plik roli.
    let rest_of_the_journal = JOURNAL_LINE.repeat(38);
    let journal = format!(
        "\n{JOURNAL_MARKER} — 2026-08-01, task backend-11, three rounds.\n{rest_of_the_journal}"
    );

    let whole = format!(
        "# Learnings — backend-dev\n\n{QUOTE_ABOUT_THE_SECTION}\n{REAL_HEADING}{patterns}## Run journal\n{journal}"
    );

    Learnings {
        whole,
        patterns,
        journal,
    }
}

/// Plik, w którym sekcja patterns jest **ostatnia** — nie ma po niej żadnego `## `.
fn patterns_last() -> String {
    format!(
        "# Learnings — reviewer\n\n{QUOTE_ABOUT_THE_SECTION}\n{REAL_HEADING}\n\
         - {PATTERNS_MARKER}: quote the failing assertion, not the summary line.\n\
         - THE LAST LINE OF THE FILE, with no heading after it.\n"
    )
}

/// Plik **bez** sekcji patterns — ale **z** cytatem blokowym o niej. To jest normalny stan
/// cudzego repozytorium i zarazem miejsce, w którym naiwne szukanie zwraca zdanie zamiast pustki.
fn without_patterns() -> String {
    format!(
        "# Learnings — dataviz\n\n{QUOTE_ABOUT_THE_SECTION}\n## Run journal\n\n\
         {JOURNAL_MARKER} — nothing has recurred yet.\n"
    )
}

#[test]
fn only_the_patterns_section_reaches_the_prompt_and_a_file_without_one_is_not_a_failure() {
    let file = learnings();

    // Straż nad samą fiksturą: bez dziesięciokrotnie dłuższego journalu próg 20% bajtów nie
    // rozróżnia niczego, bo „cały plik" mieści się wtedy w limicie.
    assert!(
        file.journal.len() >= 10 * file.patterns.len(),
        "this fixture no longer models a real role file: the journal is {} bytes and the \
         patterns section {}",
        file.journal.len(),
        file.patterns.len()
    );

    let cut =
        scan::recurring_patterns(&file.whole).expect("cutting a section out of text that has one");

    // (a) Sekcja patterns naprawdę przyjechała. Naiwne `find` trafia w cytat blokowy z trzeciej
    // linii i zwraca zdanie o tym, że reguły są wiążące — 131 bajtów zamiast 1701.
    assert!(
        cut.contains(PATTERNS_MARKER),
        "the patterns section did not come through. A naive text.find(\"## Recurring \
         patterns\") lands on the block quote three lines above the real heading, and the agent \
         gets a sentence about the rules instead of the rules: {cut:?}"
    );

    // (b) Journal nie ma prawa się tu znaleźć — i to on jest całym powodem, dla którego ta
    // funkcja istnieje.
    assert!(
        !cut.contains(JOURNAL_MARKER),
        "the run journal reached the prompt. At the host that is up to 73 KB per role, none of \
         which is a rule"
    );

    // (c) Nagłówki są granicami, nie treścią.
    assert!(
        !cut.contains("## Run journal"),
        "the next heading came along with the section"
    );
    assert!(
        !cut.trim_start().starts_with("## "),
        "the section's own heading is in the result: {cut:?}"
    );

    // (d) Mniej niż 20% bajtów pliku. To jest asercja, której „zwróć cały plik" nie przechodzi,
    // a sama obecność znacznika — tak.
    assert!(
        cut.len() * 5 < file.whole.len(),
        "the cut is {} of {} bytes. At the host the same ratio is 1701 of 32922 (5,2%) for \
         backend-dev.md and 2016 of 73258 (2,8%) for orchestrator.md",
        cut.len(),
        file.whole.len()
    );

    // (f) Sekcja będąca ostatnią w pliku jest cięta do końca pliku, a nie gubiona.
    let last = patterns_last();
    let cut_last =
        scan::recurring_patterns(&last).expect("cutting a section that runs to the end of file");
    assert!(
        cut_last.contains(PATTERNS_MARKER),
        "a patterns section with no heading after it came back empty: {cut_last:?}"
    );
    assert!(
        cut_last.contains("THE LAST LINE OF THE FILE"),
        "the section was cut short of the end of the file: {cut_last:?}"
    );

    // (e) Plik bez sekcji — pusty wynik i `Ok`, nigdy błąd. Ten plik NIESIE cytat blokowy, więc
    // naiwne szukanie zwróciłoby tu zdanie zamiast pustki.
    let none = scan::recurring_patterns(&without_patterns())
        .expect("a role file with no patterns section is a normal state, not an error");
    assert!(
        none.is_empty(),
        "a file with no patterns section produced {none:?} — the block quote about the section \
         is not the section"
    );
}
