//! Prompt systemowy lidera nie obiecuje więcej, niż pozwala jego polityka.
//!
//! # Po co to istnieje
//!
//! `BRIEF` jest dziś **jedną stałą** i mówi „You may read files and write draft files when asked".
//! Przy liderze, któremu człowiek dał `look only`, to zdanie staje się nieprawdą — a model, który
//! obieca zapis i go nie wykona, zostawia człowieka czekającego na plik, który nie powstanie.
//! Model nie ma skąd wiedzieć, że mu nie wolno: dial bezpieczeństwa jedzie do vendora osobno,
//! flagami, a prompt systemowy mówi swoje.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(brief.contains("/run"))`. Przechodzi dla jednej stałej — czyli dla dzisiejszego stanu,
//! który to kryterium ma zmienić. Rozstrzygają: (a) brak obietnicy zapisu przy `ReadOnly` razem
//! z (d), czyli wymaganiem, żeby przynajmniej dwie z trzech wersji NAPRAWDĘ się różniły. Bez (d)
//! cały plik mierzyłby jedną stałą trzy razy i był zielony od pierwszego dnia.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `chat_never_starts_a_run` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use std::collections::BTreeSet;

use loadout_lib::commands::chat::{BRIEF, Lead};
use loadout_lib::engine::drivers::Policy;
use loadout_lib::library::agents::{Agent, FileAccess};

/// Obietnica zapisu — **czytana z dzisiejszego briefu**, nie wpisana z palca.
///
/// Kontrola niżej wymaga, żeby ta fraza naprawdę stała w [`BRIEF`]. Bez niej „przy `ReadOnly` tej
/// frazy nie ma" byłoby zielone także dla frazy, której nie ma nigdzie — czyli dla asercji
/// o niczym (niezmiennik 20: sprawdzamy zachowanie, nie obecność stringa, a string, którego nikt
/// nie produkuje, jest zachowaniem, którego nikt nie ma).
const PROMISE: &str = "write draft files";

/// Że lider nie ogłasza startu, którego nie było.
///
/// 2026-08-30 — TA IGŁA BRZMIAŁA WCZEŚNIEJ „cannot start" I BYŁA WTEDY PRAWDĄ: rozmowa nie miała
/// żadnej drogi do biegu. Rozstrzygnięcie właściciela („rusza samo") tę drogę otworzyło, więc
/// tamta fraza znikła z briefu — a ochrona, której naprawdę pilnowała, została i brzmi teraz
/// wprost o obietnicy. Igła musiała pójść za nią, bo inaczej ten plik sądziłby zdanie, którego
/// nikt już nie produkuje (niezmiennik 20).
const CANNOT: &str = "never say you have started";

/// Czym praca się zaczyna. Model bez tego słowa ma narzędzie, o którym nie wie — a to jest
/// dokładnie tyle, co go nie mieć (niezmiennik 16 w wersji dla promptu).
const NEXT_MOVE: &str = "start_workflow";

/// Lider o tym dialu bezpieczeństwa.
///
/// Przez definicję agenta, bo tak liczy się polityka w produkcji: `file_access` z zapisanego pliku
/// przechodzi tabelą biegu na [`Policy`]. Zbudowanie briefu wprost z `Policy` pomijałoby dokładnie
/// ten krok, który AC-1 dowodzi — a wtedy oba kryteria mierzyłyby dwie różne drogi.
fn lead_with(access: FileAccess) -> Lead {
    Lead {
        agent: Agent {
            file_access: access,
            instructions: "You look after the repository in this folder.".to_owned(),
            ..Agent::example()
        },
    }
}

#[test]
fn the_phrases_this_file_judges_are_the_ones_the_brief_uses_today() {
    // KONTROLA WYROCZNI. Wszystkie trzy igły są czytane z dzisiejszej stałej, a nie wymyślone:
    // gdyby brief został przepisany innymi słowami, ten przypadek pada głośno, zamiast pozwolić
    // pozostałym trzem przechodzić na frazach, których nikt już nie produkuje.
    assert!(
        BRIEF.contains(PROMISE),
        "today's brief no longer carries the write promise this file tests for the ABSENCE of. \
         Point the constant at whatever phrase promises writing now — otherwise the ReadOnly case \
         below passes on a string nobody ever emits."
    );
    assert!(
        BRIEF.to_lowercase().contains(CANNOT),
        "today's brief no longer says plainly that starting work is not something the lead can do"
    );
    assert!(
        BRIEF.contains(NEXT_MOVE),
        "today's brief no longer names what DOES start work, so a refusal leaves the person where \
         they were (DESIGN §8)"
    );
}

#[test]
fn read_only_makes_no_promise_it_cannot_keep() {
    // ── (a) PRZY `ReadOnly` NIE MA OBIETNICY ZAPISU ─────────────────────────────────────────
    let lead = lead_with(FileAccess::LookOnly);
    assert_eq!(
        lead.policy(),
        Policy::ReadOnly,
        "the control for this whole case: `look only` has to be the read-only dial, or the brief \
         judged below is not the brief for the policy this test names"
    );

    let brief = lead.brief();
    assert!(
        !brief.contains(PROMISE),
        "a lead that may only read was told it can write draft files. The person then waits for a \
         file that will never appear, and the only thing that could have told them otherwise was \
         this sentence. It said:\n{brief}"
    );
    assert!(
        brief.contains(&lead.agent.instructions),
        "the policy-shaped brief dropped the agent's own instructions. Trimming the brief is not a \
         licence to forget who the lead is."
    );
}

#[test]
fn the_two_dials_that_allow_writing_still_promise_it() {
    // ── (b) PRZY `EditInFolder` I `Unrestricted` OBIETNICA JEST ─────────────────────────────
    //
    // Bez tego przypadku (a) przechodzi dla implementacji, która wyciera obietnicę ZAWSZE — a to
    // jest ta sama klasa błędu w drugą stronę: lider, który umie zapisać szkic i mówi, że nie
    // umie, odpowiada „napisz to sobie sam" na prośbę, którą mógł wykonać.
    for (access, expected) in [
        (FileAccess::AskFirst, Policy::EditInFolder),
        (FileAccess::WorkFreely, Policy::Unrestricted),
    ] {
        let lead = lead_with(access);
        assert_eq!(
            lead.policy(),
            expected,
            "the control for this case: {access:?} has to be {expected:?}"
        );
        let brief = lead.brief();
        assert!(
            brief.contains(PROMISE),
            "a lead allowed to write files was not told so ({expected:?}). It said:\n{brief}"
        );
    }
}

#[test]
fn every_version_says_who_starts_the_work_and_at_least_two_differ() {
    let briefs: Vec<String> = [
        FileAccess::LookOnly,
        FileAccess::AskFirst,
        FileAccess::WorkFreely,
    ]
    .into_iter()
    .map(|access| lead_with(access).brief())
    .collect();

    // ── (c) ZDANIE O TYM, KTO ZACZYNA PRACĘ, STOI W KAŻDEJ Z TRZECH ─────────────────────────
    for brief in &briefs {
        assert!(
            brief.to_lowercase().contains(CANNOT),
            "one of the three versions stopped saying that the lead cannot start a run. That is a \
             property of the structure — `commands/chat` knows nothing about runs — and the prompt \
             is not allowed to contradict it. It said:\n{brief}"
        );
        assert!(
            brief.contains(NEXT_MOVE),
            "one of the three versions stopped naming what DOES start work, so \"run it\" gets a \
             refusal instead of an instruction. It said:\n{brief}"
        );
    }

    // ── (d) KONTROLA PRZECIW PUSTEMU PRZEJŚCIU: PRZYNAJMNIEJ DWIE SIĘ RÓŻNIĄ ────────────────
    //
    // To jest jedyne zdanie w tym pliku, którego nie przechodzi dzisiejszy stan. Bez niego cały
    // plik jest zielony od pierwszego dnia, bo jedna stała spełnia (b) i (c), a (a) spełnia po
    // jednym skasowanym zdaniu — dla wszystkich trzech polityk naraz.
    let distinct: BTreeSet<&String> = briefs.iter().collect();
    assert!(
        distinct.len() >= 2,
        "all three policies got the SAME system prompt, so this file is measuring one constant \
         three times. The whole point of the criterion is that what the lead is promised depends \
         on what it is allowed to do."
    );
}
