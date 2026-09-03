//! Z-10: żadna komenda okna nie robi ciężkiej pracy tam, gdzie stoi webview.
//!
//! # Co to mierzy
//!
//! Tauri wykonuje komendę **bez `async` na wątku głównym** — czyli na tym samym, na którym
//! rysuje się okno. `review_skill` pobiera plik z sieci (do 20 s, `ingest::FETCH_TIMEOUT_SECONDS`),
//! `fold_run_into_branch` robi pełny checkout i merge'e, `scan_setup` obchodzi cudze repozytorium,
//! a `suggest_branch_name` woła `git for-each-ref` przy KAŻDYM znaku wpisanym w pole Startu.
//! Każda z nich zamraża okno na cały ten czas. `ipc.rs` nazywał ten dług wprost od 2026-08-16
//! i odsyłał go do człowieka (AGENTS.md §7); to jest ta odpowiedź.
//!
//! # Dlaczego ten plik CZYTA ŹRÓDŁO, choć niezmiennik 20 mówi inaczej
//!
//! Bo pytanie brzmi „na czym ta praca biegnie", a nie „co ta funkcja zwraca". Zamrożonego okna
//! nie da się zmierzyć bez Tauri, a kryterium wymagające żywego okna nie umie być czerwone
//! z właściwego powodu: `Failed to launch` i `Executable doesn't exist` stoją na liście
//! `NOT_A_REAL_RED` w `harness/gate.py`. To samo miejsce i ten sam powód, co
//! `ipc_commands_registered.rs`. Że oddana praca naprawdę zwalnia wątek, sądzi obok
//! `stop_answers_while_the_trees_close.rs` — na prawdziwym zegarze i na prawdziwym repozytorium.
//!
//! Komentarze zdejmujemy przed każdą asercją. Bez tego zdanie o `spawn_blocking` napisane
//! w prozie liczyłoby się jak oddana praca — czyli dokładnie ten incydent, który niezmiennik 20
//! nazywa po imieniu: selftest asertował flagę, przechodził **na komentarzu**, a żywa flaga
//! brzmiała inaczej.
//!
//! # Dlaczego lista nazw pochodzi z `commands.golden.txt`, a nie stąd
//!
//! Bo to kryterium ma palić DOMYŚLNIE. Lista wpisana tutaj sądziłaby dokładnie te komendy,
//! które ktoś pamiętał w dniu, w którym ją pisał — a komenda dopisana jutro byłaby niewidzialna
//! i czytałaby się jak zdana. Ten sam plik czytają oba testy rejestracji, więc nowa komenda
//! wchodzi w zakres tego pliku bez ani jednej linii.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(IPC.contains("spawn_blocking"))`. Przechodzi na jednym wywołaniu w jednej komendzie
//! z siedemdziesięciu sześciu — i przechodzi na komentarzu.

/// Jedyna lista nazw komend. Ten sam plik czytają oba testy rejestracji.
const GOLDEN: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/commands.golden.txt"));

/// Plik, w którym stoją WSZYSTKIE skorupy komend (`docs/ARCHITECTURE.md` §3).
const IPC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ipc.rs"));

/// Komendy, którym wolno być synchronicznymi, każda z zapisanym powodem.
///
/// Warunek wstępu jest jeden i jest wąski: ta komenda nie dotyka ani dysku, ani gita, ani sieci,
/// ani żadnego zamka — więc jej czas wykonania nie zależy od niczego poza procesorem.
/// `new_id` bije uuid v7 z zegara i oddaje napis (`commands::mint`).
///
/// Lista jest tu jawnie, a nie jako reguła „krótkie ciało wolno": reguła zgadująca po kształcie
/// wpuściłaby pierwszą komendę, która wygląda skromnie i woła `git`.
const PURE: [&str; 1] = ["new_id"];

/// Źródło bez komentarzy — liniowych I blokowych.
///
/// Blokowe też, i to nie jest ostrożność na zapas: `stop_run` ma w ciele trzydziestowierszowy
/// `/* … */`, w którym stoi `commands::run::stop_if_anything_is_going`. Sprawdzenie liczące
/// komentarz jak kod czyta tę prozę jako wywołanie — a sprawdzenie, które da się przekonać
/// komentarzem, jest dokładnie tym selftestem, który niezmiennik 20 nazywa po imieniu.
///
/// Cięcie jest naiwne i takie ma zostać: `//` wewnątrz literału tekstowego obcięłoby za dużo.
/// To ta sama technika, którą stosują `checks/boundary.sh` i `ipc_commands_registered`.
fn without_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find("/*") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let end = after.find("*/").map_or(after.len(), |close| close + 2);
        rest = &after[end..];
    }
    out.push_str(rest);

    let mut lines = String::with_capacity(out.len());
    for line in out.lines() {
        lines.push_str(line.split_once("//").map_or(line, |(before, _)| before));
        lines.push('\n');
    }
    lines
}

/// Nazwy z `commands.golden.txt`, po jednej w wierszu.
fn on_the_list() -> Vec<&'static str> {
    GOLDEN
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Skorupa jednej komendy, wyjęta ze źródła bez komentarzy.
struct Shell {
    /// Czy Tauri wykona ją poza wątkiem okna.
    is_async: bool,
    /// Podpis razem z ciałem, aż do klamry domykającej.
    code: String,
}

/// Skorupa komendy o tej nazwie — albo nic, kiedy w `ipc.rs` jej nie ma.
///
/// Ciało kończy się pierwszą klamrą stojącą w KOLUMNIE ZERO. To nie jest zliczanie zagnieżdżeń
/// i nie musi nim być: `cargo fmt --check` stoi w bramce, a w sformatowanym pliku wszystko
/// wewnątrz funkcji jest wcięte. Naliczanie klamer z literałów `format!("{error}")` byłoby tu
/// dokładnie tym rodzajem sprytu, który psuje się w ciszy.
fn shell_of(code: &str, name: &str) -> Option<Shell> {
    let at = code.find(&format!("fn {name}("))?;
    let (before, rest) = code.split_at(at);
    let end = rest.find("\n}\n")?;
    // Samo CIAŁO, bez podpisu: typy argumentów i typ wyniku są ścieżkami do `commands::`, ale
    // nie robią niczego. Ciało zaczyna się pierwszą klamrą po liście parametrów.
    let signed = &rest[..end];
    let body = signed.find('{').map_or("", |opens| &signed[opens..]);
    Some(Shell {
        is_async: before.ends_with("async "),
        code: body.to_owned(),
    })
}

/// Napisy, po których poznajemy WEJŚCIE W WARSTWĘ, która dotyka dysku, gita albo sieci.
///
/// Skorupa sama z siebie nie robi nic — cała praca mieszka piętro niżej i wywołuje się ją po
/// ścieżce modułu. To jest więc pytanie „czy ta skorupa woła robotę", a nie „czy ta skorupa
/// wygląda na ciężką": nazwa nowej funkcji nie musi być tu znana, bo ścieżka do niej i tak
/// zaczyna się od jednego z tych przedrostków.
const REACHES_THE_WORK: [&str; 5] = ["commands::", "crate::", "fs::", "Command::", "isolate::"];

/// Czy w tym fragmencie stoi WYWOŁANIE po jednej z tych ścieżek.
///
/// Wywołanie, nie wzmianka: `draft: commands::triggers::TriggerDraft` w podpisie i
/// `commands::memory::NoteAddress { … }` w ciele nie dotykają niczego — pierwsze jest typem
/// argumentu, drugie złożeniem struktury z tego, co już przyjechało. Sprawdzenie liczące każdy
/// napis `commands::` byłoby czerwone dla komendy, która nie robi nic (2026-09, Z-10).
fn calls_the_work(fragment: &str) -> bool {
    REACHES_THE_WORK.iter().any(|path| {
        let mut rest = fragment;
        while let Some(at) = rest.find(path) {
            let after = &rest[at + path.len()..];
            // Ścieżka bywa dowolnie długa: `crate::inherit::scan::what_this_project_can_lend(`.
            let mut tail = after;
            loop {
                tail = tail.trim_start_matches(|one: char| one.is_alphanumeric() || one == '_');
                match tail.strip_prefix("::") {
                    Some(more) => tail = more,
                    None => break,
                }
            }
            if tail.starts_with('(') {
                return true;
            }
            rest = after;
        }
        false
    })
}

/// Ciało skorupy z WYCIĘTYMI domknięciami `spawn_blocking`.
///
/// Praca oddana puli blokującej jest tym, o co to kryterium prosi, więc znika z tekstu, który
/// sądzimy — a to, co zostaje, jest dokładnie tym, co skorupa robi na wątku wołającego.
///
/// Wycinanie liczy nawiasy od `spawn_blocking(` do jego domknięcia. Zagnieżdżone wywołania w
/// środku (a są: `commands::x::y(...)`) są tym, po co ten licznik istnieje.
fn outside_the_blocking_pool(body: &str) -> String {
    let mut left = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find("spawn_blocking(") {
        left.push_str(&rest[..at]);
        let after = &rest[at + "spawn_blocking".len()..];
        let mut depth = 0_usize;
        let mut end = after.len();
        for (index, byte) in after.bytes().enumerate() {
            if byte == b'(' {
                depth += 1;
            } else if byte == b')' {
                depth -= 1;
                if depth == 0 {
                    end = index + 1;
                    break;
                }
            }
        }
        rest = &after[end..];
    }
    left.push_str(rest);
    left
}

// ── (a) ŻADNA KOMENDA NIE JEST SYNCHRONICZNA ───────────────────────────────────────────────
//
// Pierwsza połowa kryterium, i ta, która pali domyślnie: komenda bez `async` biegnie na wątku
// głównym, więc czas jej pracy jest czasem, w którym okno nie odpowiada na nic.

#[test]
fn no_command_runs_on_the_thread_the_window_is_drawn_on() {
    let listed = on_the_list();
    let code = without_comments(IPC);

    let mut missing: Vec<&str> = Vec::new();
    let mut blocking: Vec<&str> = Vec::new();
    for name in &listed {
        match shell_of(&code, name) {
            None => missing.push(name),
            Some(shell) if !shell.is_async && !PURE.contains(name) => blocking.push(name),
            Some(_) => {}
        }
    }

    assert!(
        listed.len() > 40,
        "the list of commands came out {} long, so both assertions below would be talking about \
         almost nothing. commands.golden.txt is the only list of command names this repository \
         has, and it is read here, by both registration tests, and by the window",
        listed.len()
    );
    assert!(
        missing.is_empty(),
        "these commands are on the list and have no shell in ipc.rs, so nothing below judged \
         them: {missing:?}"
    );
    assert!(
        blocking.is_empty(),
        "these commands are called on the thread the window is drawn on, so the window is frozen \
         for as long as each of them takes — and one of them fetches a file over the network, \
         another one runs a full checkout and merges, and another one asks git for every branch \
         name each time a person types a character: {blocking:?}"
    );
}

// ── (b) I ŻADNA NIE ROBI SWOJEJ PRACY NA TYM WĄTKU, NA KTÓRYM JĄ ZAWOŁANO ─────────────────
//
// Druga połowa, bez której pierwsza ma trywialne przejście: dopisanie `async` do skorupy, która
// dalej robi checkout w linii. `async fn` nie biegnie wtedy na wątku okna — ale zajmuje worker
// tokio, czyli ten sam wątek, na którym żyje Stop, pompa wierszy i pisarz indeksu.
//
// # Czego to kryterium NIE przyjmuje jako wymówki
//
// `.await` w ciele. Skorupa umie czekać na model przez pół minuty i **po drodze** przeczytać
// katalog w linii; jedno nie jest przeprosinami za drugie. Sądzimy więc każde wejście w warstwę
// roboczą OSOBNO: wywołanie po ścieżce `commands::`, `crate::`, `fs::`, `Command::` albo
// `isolate::` musi albo stać wewnątrz domknięcia `spawn_blocking` (praca oddana puli), albo być
// zakończone `.await` w tym samym wyrażeniu (praca, której nie ma co oddawać, bo jest
// asynchroniczna). Trzeciej możliwości nie ma i nie jest to formalność: `spawn_blocking` przyjmuje
// domknięcie SYNCHRONICZNE, więc pracy, na którą się czeka, nie da się w nie włożyć.
//
// Instrukcje rozdziela średnik, bo tak kończy się wyrażenie w Ruście. `let x = f(); g().await;`
// to dwie instrukcje i pierwsza z nich jest tu czerwona — dokładnie ta postać, w której czekanie
// na jedno przykrywało robotę drugiego.
//
// # Zero, nie zapadka
//
// Zmierzone 2026-09 (Z-10) przed tą poprawką: pracę w linii robiły **54** skorupy z siedemdziesięciu
// sześciu. Dziś nie robi jej ani jedna, więc nie ma tu żadnej listy wyjątków poza [`PURE`] —
// a nowa komenda wchodząca w warstwę roboczą bez `spawn_blocking` pali ten test **domyślnie**.

#[test]
fn no_command_does_its_work_on_the_thread_it_was_called_on() {
    let code = without_comments(IPC);

    let mut inline: Vec<String> = Vec::new();
    for name in on_the_list() {
        if PURE.contains(&name) {
            continue;
        }
        let Some(shell) = shell_of(&code, name) else {
            continue;
        };
        for statement in outside_the_blocking_pool(&shell.code).split(';') {
            if calls_the_work(statement) && !statement.contains(".await") {
                inline.push(format!(
                    "{name}: {}",
                    statement.split_whitespace().collect::<Vec<_>>().join(" ")
                ));
            }
        }
    }

    assert!(
        inline.is_empty(),
        "these commands reach the disk, git or the network on whichever thread Tauri called them \
         on, instead of handing that work to spawn_blocking. Each one holds that thread for as \
         long as the work takes — and that thread also carries Stop, the run's lines on their way \
         to the screen, and every write to the index:\n  {}",
        inline.join("\n  ")
    );
}
