//! Wejście binarki. Jedno rozgałęzienie i ani jednej decyzji poza nim.
//!
//! Wszystko, co powłoka robi, mieszka w `lib.rs` — dzięki temu `cargo test --lib` obejmuje
//! CAŁĄ powierzchnię testową tego crate'a, a nie wszystko minus to, co da się wywołać tylko
//! przez `main`.
//!
//! # Dlaczego most jest TĄ SAMĄ binarką, a nie osobnym programem
//!
//! Trzy powody, wszystkie zmierzone:
//!
//! * **Zero nowych skrzyń.** Serwer HTTP (`axum`, `hyper`) kosztowałby kilkadziesiąt skrzyń
//!   w drzewie, które ma ich 527 i mierzy czas kompilacji w minutach. Gniazdo uniksowe jest
//!   CECHĄ `tokio`, który już tu stoi.
//! * **Nic nie dochodzi do bundla.** Program już tam jest; most to ten sam plik z inną flagą.
//! * **Niezmiennik 6 spełniony bez ani jednej linii kodu.** Most startuje `claude`, a `claude`
//!   stoi w naszej grupie procesów — więc most też w niej stoi, ginie razem z nią i wchodzi
//!   do dowodu śmierci. Serwer nasłuchujący po stronie aplikacji stałby poza tym dowodem.

use std::io::{self, Write as _};

fn main() {
    /* ROZGAŁĘZIENIE PRZED TAURI, i to jest cała jego treść: ani most, ani rozrusznik nie
     * otwierają okna, nie zakładają bazy i nie czytają biblioteki. Jeden jest rurą, która umie
     * ramkować MCP, drugi znika przy `exec`.
     *
     * Pierwszy argument, nie parser: to są DWIE flagi, które ta binarka rozumie sama z siebie,
     * a parser dla dwóch napisów byłby zależnością za nic. */
    let mut argv = std::env::args_os().skip(1);
    if argv
        .next()
        .is_some_and(|first| first == loadout_lib::bridge::host::FLAG)
    {
        let Some(socket) = argv.next() else {
            // 2026-09 (Z-6) — `writeln!` z porzuconym wynikiem, nigdy `eprintln!`. Ten makro
            // PANIKUJE, kiedy zapis się nie uda („failed printing to stderr: Broken pipe" stoi
            // w dzienniku z 31.08), a most działa dokładnie tam, gdzie to się zdarza: wychodzi
            // razem z vendorem, więc drugi koniec bywa już zamknięty. W release stoi
            // `panic = "abort"`, czyli panika w tym miejscu zamienia uczciwy kod wyjścia
            // w przerwany proces — i nikt nie dowie się, czego brakowało.
            let _ = writeln!(
                io::stderr(),
                "Loadout needs the socket path after {}.",
                loadout_lib::bridge::host::FLAG
            );
            std::process::exit(2);
        };
        loadout_lib::bridge::run_bridge(std::path::Path::new(&socket));
        return;
    }
    loadout_lib::run();
}
