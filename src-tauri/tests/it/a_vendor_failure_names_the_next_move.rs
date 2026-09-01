//! Awaria vendora, którą umiemy rozpoznać, mówi człowiekowi, co ma zrobić.
//!
//! # Zgłoszenie
//!
//! Właściciel, 2026-08-30, po drugim trafieniu w tę samą ścianę: „znowu to jak zmieniłem projekt
//! na murmur". Widział wtedy:
//!
//! ```text
//! The lead agent could not start: The Codex App Server rejected its config/read request with
//! code -32603: failed to resolve feature override precedence: config defines `[permissions]`
//! profiles but does not set `default_permissions`
//! ```
//!
//! Każde słowo tego zdania jest prawdą i nic z niego nie wynika. Numer kodu, nazwa metody
//! i wewnętrzna fraza vendora nie mówią, że trzeba poprawić JEDNĄ LINIĘ we własnym pliku — ani
//! gdzie ten plik leży. Odmowa bez nazwania następnego ruchu zostawia człowieka tam, gdzie był
//! (DESIGN §8), a żargon vendora nie ma prawa trafić na ekran (niezmiennik 14).
//!
//! # Dlaczego rozpoznanie idzie po TREŚCI, nie po kodzie
//!
//! `-32603` to u tego vendora „internal error" i znaczy dowolną z wielu rzeczy. Tłumaczenie po
//! numerze przypisywałoby tę jedną radę awariom, których ona nie dotyczy — czyli mówiłoby
//! człowiekowi, żeby poprawił plik, który jest w porządku.
//!
//! # Czego to kryterium pilnuje najmocniej
//!
//! Trzeciego przypadku: awaria, której NIE znamy, ma dojechać **dosłownie**. Zdanie zmyślone
//! o cudzej awarii jest gorsze od cudzego żargonu, bo wygląda na wiedzę.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom, co w pozostałych
// plikach tego celu.
#![allow(clippy::expect_used)]

use loadout_lib::engine::drivers::codex::what_to_do_about_it;

/// Dosłowna treść, którą oddał App Server — przepisana ze zgłoszenia właściciela.
const MEASURED: &str = "failed to resolve feature override precedence: config defines \
                        `[permissions]` profiles but does not set `default_permissions`";

#[test]
fn the_permissions_failure_says_which_file_and_which_line() {
    let said = what_to_do_about_it(MEASURED).expect("this failure is one we have already met");

    assert!(
        said.contains("~/.codex/config.toml"),
        "it has to name the FILE. Without it the person is looking for a setting in Loadout, \
         which has none: {said}"
    );
    assert!(
        said.contains("default_permissions"),
        "and the line to add, because that is the whole fix: {said}"
    );
    assert!(
        said.contains("Claude"),
        "and the way past it right now. A person mid-task wants to keep working, not to edit a \
         config file first: {said}"
    );
    assert!(
        !said.contains("-32603") && !said.contains("config/read"),
        "and none of the vendor's own words reach the screen (invariant 14). They are true and \
         they help nobody: {said}"
    );
}

#[test]
fn a_failure_we_have_never_met_is_passed_through_untouched() {
    assert!(
        what_to_do_about_it("the model provider is over capacity right now").is_none(),
        "a failure we do not recognise keeps the vendor's own words. Inventing advice about \
         somebody else's failure is worse than repeating their jargon, because it looks like \
         knowledge and sends the person to fix a file that is fine"
    );
}
