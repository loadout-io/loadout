//! Reguły, które repo gospodarza zapisało sobie samo — przepisane do nas jako **tekst**,
//! nigdy jako maszyneria.
//!
//! Harness jest nasz. Z projektu, w którym akurat pracuje krok, dziedziczymy jedno pole
//! i dziedziczymy je przez **przepisanie do własnego pliku**, nie przez wczytanie cudzego.
//! Cała reszta tamtego dokumentu — `hooks`, `env`, `sandbox`, `permissions.allow` — nie wsiada
//! do naszego biegu w ogóle, a jedynym sposobem, żeby tego dopilnować, jest **nie wczytać
//! tamtego pliku** (`--setting-sources ""` w [`super::claude`]).
//!
//! # Dlaczego akurat te cztery pola odrzucamy [zmierzone 2026-08-19]
//!
//! **`hooks`.** Hak `PreToolUse` gospodarza startuje proces we **własnej grupie procesów**;
//! jego dziecko dostaje `ppid=1` i **przeżywa wyjście `claude`**. Jeden bieg zostawił
//! 14 sierot, eksperymenty łącznie 30. Krok się kończy, zdarzenie końca przychodzi, dowód
//! śmierci grupy z niezmiennika 6 jest prawdziwy — i nie dotyczy procesu, który nigdy nie był
//! w naszej grupie. Nic nie pęka, bramka jest zielona, a sierota pali limit w tle i dowiadujesz
//! się o niej z rachunku. Przy załadowanych ustawieniach gospodarza **niezmiennik 6 jest nie do
//! utrzymania**, i to nie dlatego, że supervisor jest słaby.
//!
//! **`env`.** Blok `env` gospodarza **nadpisuje** środowisko podane przez Loadouta, czyli
//! przewraca `env_clear()` z niezmiennika 9 **od zewnątrz**. Nie da się tego naprawić po naszej
//! stronie; da się tylko tego nie wczytać.
//!
//! **`sandbox`.** `autoAllowBashIfSandboxed: true` przepuszcza **dowolną** komendę mimo naszej
//! białej listy narzędzi. To jest pole, które nas **rozszerza**, a nie ogranicza — i dlatego
//! „wczytajmy jego ustawienia, przecież on wie lepiej, czego u siebie zabrania" nie jest
//! ostrożnością, tylko oddaniem kierownicy.
//!
//! **`permissions.allow`.** Cudza lista auto-zatwierdzania nie jest naszą polityką. Nasza
//! mieszka w jednej tabeli w [`super::claude`] (niezmiennik 23).
//!
//! # Czego ten plik nie robi
//!
//! **Nie zgaduje ścieżki projektu.** Bierze ją argumentem i nie pyta o nią warstwy okna —
//! `engine/` nie zna tego słowa (niezmiennik 1). Sąsiad `claude.rs`, nie część rdzenia:
//! `.claude/settings.json` to kształt jednego vendora, a `drivers/mod.rs` nie zna ani jednego.
//!
//! **Nie zatrzymuje biegu na cudzym pliku.** Repo, którego nie kontrolujemy, nie ma prawa
//! zabić naszego kroku jednym zepsutym przecinkiem — ani brakiem pliku, bo projekt, który nigdy
//! nie widział Claude, ma prawo wystartować. Obie te sytuacje to **pusta lista, nie błąd**.

use std::path::Path;

/// Katalog, w którym repo gospodarza trzyma swoje ustawienia projektowe.
const HOST_SETTINGS_DIR: &str = ".claude";

/// Plik ustawień projektowych wewnątrz tamtego katalogu.
///
/// Nazwa jest **cudza** i dlatego stoi tu jako stała, a nie w literale przy wywołaniu: to jest
/// jedyne miejsce w silniku, które w ogóle wie, jak ten plik się nazywa.
const HOST_SETTINGS_FILE: &str = "settings.json";

/// Reguły `deny` przepisane z `<projekt>/.claude/settings.json` — i **nic poza nimi**.
///
/// Kolejność zostaje taka, jak w cudzym pliku: lista odmów przetasowana po drodze jest listą,
/// której człowiek nie potrafi zweryfikować spojrzeniem.
///
/// Pusta lista jest **normalną odpowiedzią**, nie sygnałem błędu — tak wygląda projekt bez
/// tego pliku i projekt z plikiem, którego nie da się wczytać.
///
/// **SZKIELET KONTRAKTU (2026-08-19): jeszcze nic nie czyta i oddaje pustkę.** `todo!()` jest
/// w tym repo zakazane (`todo = "deny"`), więc zaślepka kompiluje się i oddaje pustą wartość —
/// dzięki temu AC-4 pada na **asercji**, a nie na kompilatorze. Uwaga przy wypełnianiu:
/// przepisanie ma iść **polem po polu**, nigdy przez skopiowanie obiektu `permissions` ani
/// całego dokumentu z dołożonym `deny` na wierzch. Tamta droga przenosi `env`, `sandbox`
/// i `hooks` **drugą drogą**, a test na samej zawartości `deny` świeci przy niej na zielono.
#[must_use]
pub fn deny_rules(project: &Path) -> Vec<String> {
    let settings = project.join(HOST_SETTINGS_DIR).join(HOST_SETTINGS_FILE);
    tracing::debug!(
        path = %settings.display(),
        "the host deny rules are not rewritten yet; this run starts with none"
    );
    Vec::new()
}
