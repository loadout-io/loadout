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

fn main() {
    /* ROZGAŁĘZIENIE PRZED TAURI, i to jest cała jego treść: proces mostu nie otwiera okna,
     * nie zakłada bazy i nie czyta biblioteki. Jest rurą, która umie ramkować MCP.
     *
     * `nth(1)`, nie parser argumentów: to jedyna flaga, którą ta binarka rozumie sama z siebie,
     * a parser dla jednego napisu byłby zależnością za nic. */
    let mut argv = std::env::args_os().skip(1);
    if argv
        .next()
        .is_some_and(|first| first == loadout_lib::bridge::host::FLAG)
    {
        let Some(socket) = argv.next() else {
            eprintln!(
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
