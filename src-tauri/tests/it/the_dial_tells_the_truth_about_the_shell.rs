//! Dial mówi o powłoce tyle, ile naprawdę daje — ani słowa więcej.
//!
//! # Skąd ten plik
//!
//! Zmierzone 2026-08-30 na `claude 2.1.251`, trzema sondami:
//!
//! ```text
//! acceptEdits + --allowedTools "…,Bash(git *)"    -> `echo hello-from-bash` WYKONANE
//! dontAsk     + --allowedTools "…,Bash(git *)"    -> WYKONANE
//! dontAsk     + --allowedTools BEZ `Bash` w ogóle -> WYKONANE
//! ```
//!
//! Trzecia rozstrzyga: `--allowedTools` **nie jest bramą** dla narzędzi wbudowanych w trybach,
//! które nie pytają — a Loadout używa wyłącznie takich (`dontAsk`, `acceptEdits`,
//! `bypassPermissions`). Jedyną prawdziwą bramą jest `--tools`.
//!
//! Do tego dnia kod twierdził co innego: „`Bash(git *)` to git i **tylko** git". Testy, które tego
//! pilnowały, sprawdzały OBECNOŚĆ NAPISU w argv — czyli wzorzec, przed którym stoi niezmiennik 20
//! („test sprawdza zachowanie, nie obecność stringa"). Napis był na miejscu przez cały czas
//! i przez cały czas nie ograniczał niczego.
//!
//! # Czego ten plik NIE robi
//!
//! Nie zawęża dialu. „Tylko git" jest przy dzisiejszej powierzchni vendora niewyrażalne:
//! `permissions.deny` działa, ale `deny: ["Bash"]` zabiera narzędzie w całości, a `allow` obok
//! go nie odzyskuje. Zabranie `Bash` z tego szczebla byłoby za to zmianą zachowania, o którą
//! nikt nie prosił — właściciel poprosił 2026-08-30 o coś przeciwnego: „lider ma mieć opcję też
//! write jeśli będę mu kazać coś napisać albo coś odpalić, nie ma tylko czytać".
//!
//! Ten plik pilnuje więc jednej rzeczy: żeby drabina dialu nie zaczęła **udawać** ograniczenia,
//! którego nie ma. Dzień, w którym ktoś doda tu wpis wyglądający na zawężenie powłoki, jest
//! dniem, w którym ten plik ma zapytać, czy zostało zmierzone.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom, co w pozostałych
// plikach tego celu.
#![allow(clippy::expect_used)]

use loadout_lib::engine::drivers::Policy;
use loadout_lib::engine::drivers::claude::tools_for;

/// Czy ta polityka daje powłokę **naprawdę** — czyli czy `Bash` jest na twardej liście
/// dostępności.
fn has_a_shell(policy: Policy) -> bool {
    tools_for(policy).contains(&"Bash")
}

#[test]
fn look_only_is_the_only_rung_without_a_shell() {
    assert!(
        !has_a_shell(Policy::ReadOnly),
        "Look only is the one promise on this dial that the vendor really keeps: with `Bash` \
         off the availability list, the tool is not in the session at all"
    );
    assert!(
        has_a_shell(Policy::EditInFolder),
        "and both rungs above it hand over a real shell. Anyone reading Bash(git *) in argv \
         and concluding otherwise is reading a string that restricts nothing — measured three \
         ways on 2026-08-30"
    );
    assert!(has_a_shell(Policy::Unrestricted));
}

#[test]
fn the_dial_has_two_answers_about_the_shell_and_not_three() {
    let rungs = [Policy::ReadOnly, Policy::EditInFolder, Policy::Unrestricted];
    let shells: Vec<bool> = rungs.into_iter().map(has_a_shell).collect();

    assert_eq!(
        shells,
        vec![false, true, true],
        "three positions, TWO answers about the shell. This is the sentence the code told wrong \
         until 2026-08-30, and it is the sentence a person needs when they choose a rung: below \
         the line there is no shell, above it there is a whole one. A third answer appearing \
         here means somebody believes they narrowed it — and that belief has to be measured on a \
         live CLI before it is written down, because the last one was not"
    );
}
