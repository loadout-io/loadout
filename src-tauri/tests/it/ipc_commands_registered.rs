//! AC-1 dla T-27: obie strony szwu nazywają te same komendy, i okno dostaje tę listę raz.
//!
//! `src-tauri/commands.golden.txt` to jeden plik i dwie strony granicy: ten test trzyma stronę
//! rustową (lista `generate_handler!` w `ipc.rs`, wpięcie w `lib.rs`),
//! `src/sections/commands-wired.test.ts` trzyma stronę okna. Dryf jednej z nich jest czerwony
//! u niej. To ten sam wzorzec, co `ipc_line_wire_golden.rs` z T-07 — złoty plik czytany z obu
//! stron — i to jest jedyny powód, dla którego ta lista w ogóle wolno istnieć (niezmiennik 21).
//!
//! # Dlaczego przedmiotem asercji jest RÓWNOŚĆ ZBIORÓW
//!
//! Zawieranie w jedną stronę nie odróżnia dwóch defektów, a oba są ciche:
//!
//! - **Komenda na liście, ale niezarejestrowana** jest dokładnie dzisiejszym stanem repo:
//!   funkcja istnieje, jest przetestowana, `invoke` na nią nie trafia, i nic po drodze nie
//!   krzyczy — okno dostaje odmowę, której nikt nie umie powiązać z brakującym wierszem.
//! - **Komenda zarejestrowana, ale nieobecna na liście** jest komendą, o której front nie wie:
//!   powierzchnia, której nikt nie wywołuje i nikt nie pilnuje.
//!
//! **Słaba wersja tego kryterium: `assert!(source.contains("generate_handler!"))`.** Przechodzi
//! na liście z jedną komendą i dwunastoma brakującymi. Rozróżnia je równość zbiorów i to, że
//! komunikat **nazywa** brakujących po każdej ze stron — lista, która mówi „nie zgadza się",
//! zostawia szukanie po dwóch plikach.
//!
//! # Czego ten test świadomie NIE robi
//!
//! Nie uruchamia Tauri i nie może: `Failed to launch` i `Executable doesn't exist` są na liście
//! `NOT_A_REAL_RED` w `harness/gate.py`, więc kryterium wymagające żywego okna nie umie być
//! czerwone z właściwego powodu. Czyta więc **źródło** — i to jest jedyne miejsce w tym
//! zadaniu, gdzie asercja dotyczy tekstu, a nie zachowania (niezmiennik 20). Dwa pozostałe
//! kryteria rustowe przepuszczają dane przez prawdziwe funkcje na prawdziwym katalogu.

use std::collections::BTreeSet;

/// Jedyna lista nazw komend. Ten sam plik czyta lustro po stronie okna.
const GOLDEN: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/commands.golden.txt"));

/// Plik, w którym wolno napisać słowo „tauri" (`docs/ARCHITECTURE.md` §3) — i w którym stoi
/// jedyna lista `generate_handler!`.
const IPC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ipc.rs"));

/// Powłoka aplikacji: builder okna, a na nim jedno `.invoke_handler(...)`.
const SHELL: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));

/// Źródło bez komentarzy liniowych.
///
/// Bez tego zdanie o `generate_handler!` napisane w komentarzu liczyłoby się jak rejestracja —
/// czyli dokładnie ten incydent, który `AGENTS.md` (niezmiennik 20) nazywa po imieniu:
/// selftest asertował flagę, przechodził **na komentarzu**, a żywa flaga brzmiała inaczej.
///
/// Cięcie jest naiwne i takie ma zostać: `//` wewnątrz literału tekstowego obcięłoby za dużo.
/// To ta sama technika, którą stosuje `checks/quick-boundary.sh` (`sed 's://.*::'`), a w Ruście
/// literał z `//` w środku zdarza się w adresach, których żaden z tych dwóch plików nie niesie.
fn without_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        out.push_str(line.split_once("//").map_or(line, |(before, _)| before));
        out.push('\n');
    }
    out
}

/// Nazwy z `commands.golden.txt`, w kolejności z pliku i z powtórzeniami.
///
/// Powtórzenia zostają, bo są osobną asercją: zbiór je milcząco skleja, a sklejone dwie nazwy
/// to jedna komenda mniej po obu stronach porównania naraz.
fn on_the_list() -> Vec<&'static str> {
    GOLDEN
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Treść pierwszej pary nawiasów po `generate_handler!`.
///
/// Trzy rodzaje nawiasu, bo makro wywołuje się legalnie na każdy z nich, a zliczanie
/// zagnieżdżenia jest po to, żeby `Builder::default()` w środku listy nie ucinał jej w połowie.
fn handler_list(code: &str) -> Option<&str> {
    let after = code.split_once("generate_handler!")?.1;
    let open_at = after.find(['[', '(', '{'])?;
    let open = after.as_bytes().get(open_at).copied()?;
    let close = match open {
        b'[' => b']',
        b'(' => b')',
        _ => b'}',
    };

    let mut depth = 0_usize;
    for (index, byte) in after.bytes().enumerate().skip(open_at) {
        if byte == open {
            depth += 1;
        } else if byte == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return after.get(open_at + 1..index);
            }
        }
    }
    None
}

/// Nazwy komend naprawdę zarejestrowanych, ostatni człon ścieżki modułu.
///
/// `commands::agents::list_agents` i `list_agents` to ta sama komenda: Tauri nadaje jej nazwę
/// funkcji, nie ścieżki, którą do niej dojechano.
fn registered() -> BTreeSet<String> {
    let code = without_comments(IPC);
    let Some(list) = handler_list(&code) else {
        return BTreeSet::new();
    };

    list.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.rsplit("::").next().unwrap_or(item).trim().to_owned())
        .collect()
}

#[test]
fn the_list_and_the_handler_name_exactly_the_same_commands() {
    let listed = on_the_list();
    let wanted: BTreeSet<String> = listed.iter().map(|name| (*name).to_owned()).collect();
    assert_eq!(
        listed.len(),
        wanted.len(),
        "commands.golden.txt names one command twice. A repeat is invisible in this comparison \
         — the set swallows it — and it takes a real command down with it, because both sides \
         then agree about a list that is one shorter than the file. Rows: {listed:?}"
    );
    assert!(
        !wanted.is_empty(),
        "commands.golden.txt holds no command names at all. An empty list agrees with an empty \
         handler, so this criterion would pass over a window that can do nothing"
    );

    let live = registered();
    let missing: Vec<&String> = wanted.difference(&live).collect();
    let uninvited: Vec<&String> = live.difference(&wanted).collect();

    assert!(
        missing.is_empty() && uninvited.is_empty(),
        "the two sides of the seam do not name the same commands.\n  \
         on commands.golden.txt, never registered: {missing:?}\n  \
         registered, absent from commands.golden.txt: {uninvited:?}\n\
         The first list is today's defect, spelled out: the function is there, it is tested, \
         `invoke` never reaches it, and nothing in between says a word. The second is a command \
         the window was never told about."
    );
}

#[test]
fn there_is_exactly_one_handler_list() {
    let code = without_comments(IPC);
    let found = code.matches("generate_handler!").count();
    assert_eq!(
        found, 1,
        "src/ipc.rs holds {found} handler lists, and the number has to be one. Zero is a window \
         with no commands. Two means the comparison above reads the first one and says nothing \
         about the second, so half the commands could be missing behind a green criterion"
    );
}

#[test]
fn the_window_is_handed_that_list_exactly_once() {
    let code = without_comments(SHELL);
    let found = code.matches(".invoke_handler").count();
    assert_eq!(
        found, 1,
        "src/lib.rs hands the builder {found} handler lists, and the number has to be one. Zero \
         is the state this task exists to end: the window opens and not one command reaches \
         Rust. Two means the second one quietly replaced the first and half the commands \
         vanished — the builder keeps the last one it was given"
    );
}
