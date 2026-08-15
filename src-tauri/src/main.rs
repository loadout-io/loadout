//! Wejście binarki. Cztery linie i ani jednej decyzji.
//!
//! Wszystko, co powłoka robi, mieszka w `lib.rs` — dzięki temu `cargo test --lib` obejmuje
//! CAŁĄ powierzchnię testową tego crate'a, a nie wszystko minus to, co da się wywołać tylko
//! przez `main`.

fn main() {
    loadout_lib::run();
}
