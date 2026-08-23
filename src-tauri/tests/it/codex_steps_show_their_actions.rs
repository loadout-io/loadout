//! AC-1 dla T-97: krok Codeksa pokazuje, CO ZROBIŁ, nie tylko co powiedział.
//!
//! # Po co to istnieje
//!
//! `CodexDriver` wypuszczał [`DecodedEvent`] z `tool: None`, więc kurator nie miał z czego wybrać
//! wariantu wiersza i transkrypt kroku Codeksa był **samą prozą**: ani jednego `Ran`, `Edited`
//! czy `Searched`, choć strumień niesie je wszystkie. Ta sama awaria u Claude'a została zmierzona
//! 2026-08-18 i naprawiona przez `stream::decode`; tutaj domyka ją bliźniak — `decode_codex`.
//!
//! DRUGI DEKODER PRZED TYM SAMYM KURATOREM, nie drugi kurator. Reguły zwijania (okno 2 s,
//! licznik, wariant wiersza) zostają jedną maszyną w `engine::line` (niezmiennik 15), a jedyne,
//! co dokłada nowy szew, to fakty, których [`AgentEvent`] świadomie nie niesie: rodzina czynności,
//! **pełna** ścieżka i **pełne** wyjście [T1 §8.2].
//!
//! # Skąd ten plik bierze prawdę
//!
//! `docs/research/fixtures/codex-stream-live.jsonl` — prawdziwy `codex exec --json`, 11 linii,
//! surowe bajty. Nie `codex-stream.jsonl`: tamten jest kopertą biegu, który padł na wyczerpanych
//! kredytach (cztery linie, ani jednego `item.*`), i **ma taki zostać**, bo kryterium S-3
//! asertuje dokładnie ten wariant „zablokowany".
//!
//! # `reasoning` dowodzimy linią podaną wprost, i to jest fakt o vendorze
//!
//! Zmierzone 2026-08-24 na `codex-cli 0.148.0` trzema drogami — sześć prawdziwych biegów, sonda
//! z siecią i sonda z `model_reasoning_effort=high` plus `model_reasoning_summary=detailed` —
//! **`reasoning` nie pada w trybie `exec` ani razu**. Tabela w `ARCHITECTURE.md` §6 wymienia go
//! za raportem T2 i ta pozycja się zestarzała. Odwzorowanie ma więc istnieć i być sprawdzone,
//! żeby zadziałało, gdyby vendor kiedyś zaczął je wysyłać — ale **dopisanie go do fikstury
//! byłoby wymyśleniem biegu, który się nie zdarzył**, czyli dokładnie tą cichą porażką, przed
//! którą ostrzega S-3. Stoi więc niżej jako zwykły test jednostkowy nad jedną linią.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(lines.len() > 1)`. Przechodzi dla implementacji, która oddaje samą prozę — czyli dla
//! dzisiejszej. Rozróżnia to **zbiór rodzajów** wierszy plus treść każdego z nich: `ran` musi
//! wiedzieć, czy komenda wyszła (`exit_code`), `edit` musi nieść ścieżkę, `search` zapytanie.
//!
//! Druga słaba wersja: pętla po liniach fikstury z asercjami w środku. Plik krótszy, niż ktokolwiek
//! zakładał, wykonuje ją zero razy i jest zielony (niezmiennik 19). Rozróżniają to dwa strażniki
//! wpisane wprost — `FIXTURE_LINES` i kontrola, że fikstura naprawdę niesie cztery rodzaje
//! czynności, zanim cokolwiek zostanie o nich powiedziane.

// `expect()`/`unwrap()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam
// powód, co w `driver_codex_stream` i w pozostałych plikach tego celu.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::error::Error;

use loadout_lib::engine::drivers::codex::CodexDecoder;
use loadout_lib::engine::line::{Curator, Line, LineKind, Seen, Status};
use loadout_lib::engine::stream::{Decoded, DecodedEvent, decode_codex};

/// Żywy strumień Codeksa, nagrany prawdziwym `codex exec --json`.
const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docs/research/fixtures/codex-stream-live.jsonl"
));

/// Ile linii ma mieć złoty plik **co najmniej**. Strażnik, nie komentarz: przycięty plik
/// przepuściłby każdą pętlę niżej na krótszej sekwencji i nikt by tego nie zauważył.
const FIXTURE_LINES: usize = 11;

/// Czyj to strumień.
const AGENT: &str = "builder";

/// Ile milisekund dzieli kolejne linie w tym teście.
///
/// **Więcej niż okno sklejania (2 s)**, i to jest wymaganie, nie ostrożność: czynności tej samej
/// rodziny podane w jednym oknie zwijają się w jeden wiersz z licznikiem — co jest poprawne
/// i czego ten plik nie sądzi. Rozstrzelone w czasie dają po wierszu na czynność, więc każda
/// asercja niżej mówi o dokładnie jednej rzeczy.
const APART_MS: u64 = 3_000;

/// Plik, który agent założył w tym biegu — **pełna** ścieżka, tak jak stoi na drucie.
const FILE: &str = "/private/tmp/cxp/notes.md";

/// Komenda, którą agent uruchomił.
const COMMAND: &str = "wc -l seed.txt";

/// Czego agent szukał w sieci.
const QUERY: &str = "site:blog.rust-lang.org Rust stable release current August 2026";

/// Cały złoty plik przepuszczony przez PRAWDZIWĄ drogę: dekoder vendora, fakty z tej samej linii,
/// jeden kurator.
///
/// Nie `Curator` wołany na ręcznie złożonych zdarzeniach: kryterium mówi o tym, co zobaczy
/// człowiek, a między zdarzeniem a wierszem stoi dokładnie ten szew, którego brak jest tu wadą.
///
/// Oddaje **oba** ujścia kuratora, bo ekran ma dwa i tylko jedno z nich jest historią: wiersze
/// i slot na dole (`Curator::status`). Test patrzący wyłącznie na wiersze nie odróżnia myślenia,
/// które doszło do slotu, od myślenia, które przepadło.
fn curated(text: &str) -> (Vec<Line>, Option<Status>) {
    let mut decoder = CodexDecoder::new();
    let mut curator = Curator::new();
    let mut lines = Vec::new();
    let mut slot = None;
    let mut at_ms = 0;

    for line in text.lines() {
        at_ms += APART_MS;
        let Decoded::Events(events) = decode_codex(&mut decoder, line) else {
            continue;
        };
        for DecodedEvent { event, tool } in events {
            lines.extend(curator.observe(Seen {
                agent: AGENT,
                at_ms,
                event: &event,
                tool: tool.as_ref(),
            }));
            // Zapamiętany PO KAŻDYM zdarzeniu, nie odczytany na końcu: slot jest nadpisywany,
            // a kolejna czynność go gasi (`self.status = None`), więc odczyt po zakończeniu
            // strumienia zawsze mówiłby „nic się nie dzieje".
            slot = slot.or(curator.status());
        }
    }
    lines.extend(curator.flush());
    (lines, slot)
}

/// Rodzaje wierszy, które z tego wyszły.
fn kinds(lines: &[Line]) -> BTreeSet<String> {
    lines
        .iter()
        .map(|line| format!("{:?}", line.kind()))
        .collect()
}

/// Pierwszy wiersz danego rodzaju.
fn first(lines: &[Line], kind: LineKind) -> Option<&Line> {
    lines.iter().find(|line| line.kind() == kind)
}

#[test]
fn the_fixture_really_carries_the_four_kinds_of_doing() {
    // KONTROLA FIKSTURY, i stoi pierwsza. Każda asercja w pozostałych testach jest prawdziwa
    // o pustym pliku, więc bez tej jednej cała reszta może certyfikować nic.
    let lines: Vec<&str> = FIXTURE
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert!(
        lines.len() >= FIXTURE_LINES,
        "the live golden file has to hold at least {FIXTURE_LINES} lines. It holds {}, and every \
         loop over a truncated file makes each assertion inside it true about nothing",
        lines.len()
    );

    for kind in [
        "agent_message",
        "file_change",
        "command_execution",
        "web_search",
    ] {
        assert!(
            FIXTURE.contains(kind),
            "this file is the whole evidence that a Codex step does more than talk, and it does \
             not carry a single {kind}"
        );
    }

    // I ANI JEDNEGO `reasoning`, bo w trybie exec ten typ nie pada — patrz nagłówek pliku.
    // Asercja stoi tu po to, żeby DOPISANIE go do fikstury było czerwone: wymyślony bieg
    // przechodziłby wtedy jako nagrany.
    assert!(
        !FIXTURE.contains("reasoning\""),
        "measured on codex-cli 0.148.0 three separate ways: this vendor never emits that item \
         type in exec mode. A golden file that carries one was not recorded - it was written, \
         and then it certifies the decoder against our own beliefs instead of the vendor"
    );
}

#[test]
fn a_codex_step_shows_the_same_kinds_of_row_a_claude_step_does() {
    let (lines, _slot) = curated(FIXTURE);
    let seen = kinds(&lines);

    assert!(
        seen.contains("Ran") && seen.contains("Edit") && seen.contains("Search"),
        "a step that ran a command, changed a file and searched the web has to leave a row for \
         each of them - the same rows a Claude step leaves for the same doing. It left {seen:?}, \
         and a transcript of pure prose is indistinguishable from an agent that only talked"
    );
    assert!(
        seen.contains("Note"),
        "and the prose stays: it is the one thing in this stream a person actually asked for. \
         It left {seen:?}"
    );
}

#[test]
fn the_row_for_a_command_knows_whether_it_worked() -> Result<(), Box<dyn Error>> {
    let (lines, _slot) = curated(FIXTURE);
    let row = first(&lines, LineKind::Ran).ok_or_else(|| {
        format!(
            "the stream ran a command and left no row for it: {:?}",
            kinds(&lines)
        )
    })?;

    let Line::Ran { text, ok, .. } = row else {
        return Err(format!("the row for a command is not a command row: {row:?}").into());
    };
    assert!(
        *ok,
        "this command came back with exit_code 0, so it worked. A row that reads as failed here \
         tells somebody their build broke when it did not - and `ok` has to come from exit_code \
         and nowhere else. It came out as {row:?}"
    );
    assert!(
        text.contains(COMMAND),
        "the row has to name the command that ran, or it says only that SOMETHING ran. It reads \
         as {text:?}"
    );

    Ok(())
}

#[test]
fn the_row_for_a_changed_file_carries_the_whole_path() -> Result<(), Box<dyn Error>> {
    let (lines, _slot) = curated(FIXTURE);
    let row = first(&lines, LineKind::Edit).ok_or_else(|| {
        format!(
            "the stream changed a file and left no row for it: {:?}",
            kinds(&lines)
        )
    })?;

    let Line::Edit { paths, .. } = row else {
        return Err(format!("the row for a changed file is not an edit row: {row:?}").into());
    };
    assert!(
        paths.iter().any(|path| path == FILE),
        "the row has to carry the FULL path from the stream, because expanding it is what shows \
         which files moved. It carried {paths:?}"
    );

    Ok(())
}

#[test]
fn the_row_for_a_search_carries_what_was_asked() -> Result<(), Box<dyn Error>> {
    let (lines, _slot) = curated(FIXTURE);
    let row = first(&lines, LineKind::Search).ok_or_else(|| {
        format!(
            "the stream searched the web and left no row for it: {:?}",
            kinds(&lines)
        )
    })?;

    assert!(
        row.text().contains(QUERY),
        "the row has to say what was searched for, or it is a row saying only that something was \
         looked up somewhere. It reads as {:?}",
        row.text()
    );

    Ok(())
}

#[test]
fn an_item_type_nobody_knows_is_let_go_without_taking_the_stream_down() {
    // Niezmiennik 5: vendorzy dokładają typy co tydzień, po cichu. Linia, której nikt nie rozumie,
    // ma zniknąć z widoku i **nie** zabrać ze sobą reszty tury.
    let invented = "{\"type\":\"item.completed\",\"item\":{\"id\":\"item_9\",\
                    \"type\":\"quantum_flux\",\"amplitude\":3}}\n";
    let with_a_stranger = format!("{FIXTURE}{invented}");

    let before = kinds(&curated(FIXTURE).0);
    let after = kinds(&curated(&with_a_stranger).0);

    assert_eq!(
        after, before,
        "an item type from next week's release has to be let go quietly. It changed what the \
         person sees from {before:?} to {after:?} - and a stream that falls over on the first \
         unknown type looks exactly like an agent that crashed"
    );
}

#[test]
fn thinking_has_a_mapping_ready_for_the_day_the_vendor_starts_sending_it() {
    /* LINIA PODANA DEKODEROWI WPROST, NIE FIKSTURA, i to jest zmierzony fakt o vendorze, nie
     * luka w pliku: `reasoning` nie pada w trybie `exec` ani razu (trzy drogi sprawdzenia,
     * 2026-08-24, codex-cli 0.148.0 - w całości w nagłówku). Dopisanie go do złotego pliku
     * byłoby wymyśleniem biegu, który się nie zdarzył; odwzorowanie ma jednak istnieć, żeby
     * zadziałało w dniu, w którym vendor je włączy. */
    let reasoning = "{\"type\":\"item.completed\",\"item\":{\"type\":\"reasoning\"}}";
    let (lines, slot) = curated(reasoning);

    assert_eq!(
        slot,
        Some(Status::Thinking),
        "this item type has to light the slot at the bottom of the screen - that slot IS the \
         mapping, and while it stays dark the bottom of the screen is dead whenever the agent \
         is working (ARCHITECTURE section 6, rule 5)"
    );
    assert!(
        lines.is_empty(),
        "and it has to leave the history alone: the virtualised list measures every row, \
         including an empty one. It left {lines:?}"
    );
}

#[test]
fn thinking_never_becomes_a_line_of_the_transcript() {
    // Druga połowa tamtego odwzorowania i osobne zdanie: `ARCHITECTURE.md` §6 reguła 5 mówi, że
    // myślenie rysuje stały slot na dole ekranu i **nie wchodzi do historii**. Bez tej asercji
    // przechodzi implementacja, która pokazuje treść myślenia jako prozę.
    let reasoning = "{\"type\":\"item.completed\",\"item\":{\"type\":\"reasoning\",\
                     \"text\":\"first I will read the file\"}}";
    let (lines, _slot) = curated(reasoning);

    for line in &lines {
        assert_ne!(
            line.kind(),
            LineKind::Note,
            "thinking draws the slot at the bottom of the screen and never enters the history. \
             It came out as {line:?}"
        );
        assert!(
            !line.text().contains("first I will read the file"),
            "and its content never reaches the screen at all. It came out as {line:?}"
        );
    }
}
