//! AC-2 dla T-30: trzy komendy biegu są zarejestrowane, a ich skorupy nie niosą logiki.
//!
//! T-27 wpiął w okno cztery powierzchnie biblioteki i zostawił bieg poza listą — bo bez mostu
//! do pompy nie było czego rejestrować. Ten plik trzyma drugą połowę tej samej granicy:
//! `run_workflow`, `stop_run` i `continue_run` mają stać w `commands.golden.txt` **i**
//! w `generate_handler!`, a ich skorupy mają zostać skorupami.
//!
//! # Dlaczego dwie asercje, a nie jedna
//!
//! Sama obecność nazw przechodzi na skorupie, która niesie połowę planisty — a logika napisana
//! w `#[tauri::command]` jest logiką, której **nie da się przetestować bez Tauri**:
//! `State<'_, AppState>` nie zbudujesz w teście jednostkowym, a `&RunDeps` zbudujesz w sześciu
//! wierszach (niezmiennik 23). Dlatego drugie pytanie brzmi: ile instrukcji stoi w ciele.
//! Trzy — rozpakowanie stanu, wywołanie `*_inner`, zwrot — i ani jednej więcej.
//!
//! **Słaba wersja: `assert!(source.contains("run_workflow"))`.** Przechodzi na nazwie
//! wypisanej w komentarzu i na skorupie o trzydziestu wierszach. Rozróżniają je: zdjęcie
//! komentarzy przed czytaniem (ten sam incydent, który `AGENTS.md` niezmiennik 20 nazywa po
//! imieniu — selftest przechodził **na komentarzu**, a żywa flaga brzmiała inaczej) i limit
//! instrukcji.
//!
//! # Czego ten test świadomie NIE robi
//!
//! Nie uruchamia Tauri i nie może: `Failed to launch` i `Executable doesn't exist` są na liście
//! `NOT_A_REAL_RED`, więc kryterium wymagające żywego okna nie umie być czerwone z właściwego
//! powodu. Czyta więc **źródło** — dokładnie tak, jak `ipc_commands_registered.rs` z T-27,
//! i z tego samego powodu.
//!
//! Nie liczy też instrukcji **zagnieżdżonych**. Liczy je na najwyższym poziomie ciała, bo tam
//! mieszka zdanie z kryterium („rozpakuj, zawołaj, zwróć"). Skutek uboczny jest tu wypisany,
//! żeby nikt nie wziął zieleni za dowód: skorupa z jednym wielkim `match` w środku ma jedną
//! instrukcję i przejdzie. Ten przypadek zostaje ludzkim osądem, tak samo jak trzecia reguła
//! `checks/quick-boundary.sh` zostawia ludziom zapytanie sklejane w czasie biegu.

use std::collections::BTreeSet;
use std::error::Error;

/// Jedyna lista nazw komend. Ten sam plik czyta lustro po stronie okna
/// (`src/sections/commands-wired.test.ts`) i drugi test rejestracji z T-27.
const GOLDEN: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/commands.golden.txt"));

/// Plik, w którym wolno napisać słowo „tauri" (`docs/ARCHITECTURE.md` §3) — i w którym stoi
/// jedyna lista `generate_handler!` oraz wszystkie skorupy.
const IPC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ipc.rs"));

/// Trzy komendy biegu. Nazwa komendy to nazwa funkcji pod `#[tauri::command]`, znak w znak.
const RUN_COMMANDS: [&str; 3] = ["continue_run", "run_workflow", "stop_run"];

/// Sufit instrukcji w ciele skorupy: rozpakuj stan, zawołaj `*_inner`, zwróć.
const STATEMENT_LIMIT: usize = 3;

/// Źródło bez komentarzy liniowych.
///
/// Bez tego zdanie o `run_workflow` napisane w komentarzu liczyłoby się jak rejestracja.
/// Cięcie jest naiwne i takie ma zostać: `//` wewnątrz literału tekstowego obcięłoby za dużo,
/// a w tym pliku literały z `//` w środku się nie zdarzają. Ta sama technika, co
/// w `checks/quick-boundary.sh` (`sed 's://.*::'`).
fn without_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        out.push_str(line.split_once("//").map_or(line, |(before, _)| before));
        out.push('\n');
    }
    out
}

/// Nazwy z `commands.golden.txt`. Wiersz pusty i wiersz zaczynający się od `#` są pomijane.
fn on_the_list() -> BTreeSet<&'static str> {
    GOLDEN
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Treść pierwszej pary nawiasów po `generate_handler!`.
///
/// Trzy rodzaje nawiasu, bo makro wywołuje się legalnie na każdy z nich; zliczanie zagnieżdżenia
/// jest po to, żeby wywołanie w środku listy nie ucinało jej w połowie.
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
/// `commands::run::run_workflow` i `run_workflow` to ta sama komenda: Tauri nadaje jej nazwę
/// funkcji, nie ścieżki, którą do niej dojechano.
fn registered(code: &str) -> BTreeSet<String> {
    let Some(list) = handler_list(code) else {
        return BTreeSet::new();
    };
    list.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.rsplit("::").next().unwrap_or(item).trim().to_owned())
        .collect()
}

/// Wszystko między klamrami ciała funkcji `name`.
///
/// Szukamy `fn <name>(` z nawiasem tuż za nazwą, więc `run_workflow_inner` nie udaje
/// `run_workflow`. Zwraca `None`, kiedy takiej funkcji nie ma — i wołający ma to zgłosić jako
/// brak skorupy, nigdy przemilczeć: pusty zbiór instrukcji przechodzi każdy limit.
fn body_of<'a>(code: &'a str, name: &str) -> Option<&'a str> {
    let at = code.find(&format!("fn {name}("))?;
    let after = code.get(at..)?;
    let open_at = after.find('{')?;

    let mut depth = 0_usize;
    for (index, byte) in after.bytes().enumerate().skip(open_at) {
        if byte == b'{' {
            depth += 1;
        } else if byte == b'}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return after.get(open_at + 1..index);
            }
        }
    }
    None
}

/// Ile instrukcji stoi na najwyższym poziomie ciała.
///
/// Średnik na głębokości zero kończy instrukcję; to, co zostało po ostatnim średniku i nie jest
/// samą pustką, jest wyrażeniem ogonowym, czyli czwartą instrukcją, gdyby ktoś ją dopisał.
fn statements(body: &str) -> usize {
    let mut depth = 0_usize;
    let mut counted = 0_usize;
    let mut open = false;

    for character in body.chars() {
        match character {
            '{' | '(' | '[' => {
                depth += 1;
                open = true;
            }
            '}' | ')' | ']' => {
                depth = depth.saturating_sub(1);
                open = true;
            }
            ';' if depth == 0 => {
                counted += 1;
                open = false;
            }
            other if !other.is_whitespace() => open = true,
            _ => {}
        }
    }

    counted + usize::from(open)
}

#[test]
fn the_three_run_commands_are_on_the_list_and_in_the_handler() {
    let listed = on_the_list();
    let code = without_comments(IPC);
    let live = registered(&code);

    let missing_from_list: Vec<&str> = RUN_COMMANDS
        .into_iter()
        .filter(|name| !listed.contains(name))
        .collect();
    assert!(
        missing_from_list.is_empty(),
        "these run commands are absent from src-tauri/commands.golden.txt: \
         {missing_from_list:?}. That list is the one place where both sides of the seam agree \
         on a name — a command missing from it is a command the window is never told about, \
         and `invoke` on it fails with a name nobody is keeping alive"
    );

    let missing_from_handler: Vec<&str> = RUN_COMMANDS
        .into_iter()
        .filter(|name| !live.contains(*name))
        .collect();
    assert!(
        missing_from_handler.is_empty(),
        "these run commands are never registered in generate_handler!: \
         {missing_from_handler:?}. This is today's defect spelled out: the *_inner function is \
         there, it is tested, `invoke` never reaches it, and nothing in between says a word. \
         The handler holds {live:?}"
    );
}

#[test]
fn no_run_shell_carries_logic() -> Result<(), Box<dyn Error>> {
    let code = without_comments(IPC);

    for name in RUN_COMMANDS {
        // Brak skorupy jest ZGŁASZANY, nie przemilczany: puste ciało ma zero instrukcji, więc
        // przechodzi każdy limit, jaki da się wymyślić.
        let body = body_of(&code, name).ok_or_else(|| {
            format!(
                "src/ipc.rs has no `fn {name}(` at all, so there is no shell to measure. A name \
                 on the golden list with no function behind it is a command that goes nowhere"
            )
        })?;

        let counted = statements(body);
        assert!(
            counted <= STATEMENT_LIMIT,
            "the shell of `{name}` holds {counted} statements and the ceiling is \
             {STATEMENT_LIMIT}: unpack the state, call the *_inner function, return. Anything \
             more is logic written where no unit test can reach it — `State<'_, AppState>` \
             cannot be built in a test, and `&RunDeps` can (invariant 23). The body was:\n{body}"
        );
    }
    Ok(())
}
