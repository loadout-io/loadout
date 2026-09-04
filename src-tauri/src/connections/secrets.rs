//! Nośnik wartości, których wymagają zatwierdzone Connections.
//!
//! # Po co to istnieje (audyt 2026-09-02, C-6)
//!
//! Do 2026-09 wartości miały jedno źródło: środowisko procesu okna (`std::env::var_os`
//! w `commands::run` i `commands::chat`). Aplikacja uruchomiona **z Docka** nie dziedziczy
//! powłoki człowieka — `launchd` daje jej kilkanaście zmiennych i ani jednego klucza — więc to
//! samo zatwierdzone Połączenie działało po starcie z terminala i odmawiało po kliknięciu
//! w ikonę. Nic na ekranie nie mówiło, na czym polega różnica.
//!
//! Nośnikiem jest `~/.loadout/env`, czyli `<biblioteka>/env`: jeden plik obok reszty rzeczy
//! Loadouta, pisany ręką człowieka, w kształcie `NAZWA=wartość` po jednej na wiersz.
//!
//! # Trzy rzeczy, które ten plik robi świadomie
//!
//! * **Środowisko wygrywa.** Nośnik odpowiada dopiero wtedy, gdy nazwy w środowisku okna nie
//!   ma. Odwrotna kolejność znaczyłaby, że plik sprzed miesiąca cicho przykrywa wartość, którą
//!   człowiek właśnie wyeksportował, żeby coś sprawdzić.
//! * **Plik czytelny dla innych kont jest pomijany w całości**, nie „po wierszu". Jeden plik
//!   trzyma klucze do wszystkich narzędzi tego człowieka, więc jest wart dokładnie tyle, ile
//!   jego prawa dostępu; pytanie o nie zadajemy tam, gdzie mieszka kod platformowy
//!   (niezmiennik 3), a nie tutaj.
//! * **Cudzysłowów nie zdejmujemy.** Znak `"` jest legalnym znakiem tokena, więc obcinanie go
//!   psułoby wartość, która naprawdę go ma — a wtedy odmowa przyszłaby od narzędzia, nie od
//!   Loadouta, i nikt nie skojarzyłby jej z tym plikiem. Białe znaki wokół wartości obcinamy,
//!   bo `NAZWA = wartość` ludzie piszą, a token kończący się spacją nie istnieje.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Nazwa nośnika w bibliotece. `~/.loadout/env`, czyli obok `connections/` i `agents/`.
const FILE: &str = "env";

/// Co Loadout czyta, kiedy nośnika nie ma gdzie szukać. Jedno miejsce, bo to zdanie stoi
/// w odmowie, którą czyta człowiek, a jedno zdanie w dwóch kopiach rozjeżdża się przy pierwszej
/// zmianie jednej z nich (niezmiennik 13).
pub const ENVIRONMENT_ONLY: &str = "the environment this app was started with";

/// Skąd Loadout bierze wartość wymaganą przez Połączenie — i co potrafi o tym powiedzieć.
///
/// Struktura, a nie samo domknięcie, dokładnie z jednego powodu: odmowa ma powiedzieć CZŁOWIEKOWI,
/// gdzie Loadout szukał (niezmiennik 29). Domknięcie oddające `Option` zna odpowiedź na „czy
/// jest", ale nie ma jak opowiedzieć „gdzie patrzyłem", a to jest jedyne pytanie, z którym ta
/// odmowa człowieka zostawia.
#[derive(Debug, Clone)]
pub struct Carrier {
    /// `None`, kiedy okno nie powiedziało jeszcze, gdzie leży biblioteka. Wtedy zostaje samo
    /// środowisko — i tyle mówi też zdanie odmowy, zamiast wskazywać plik, którego nie czytano.
    path: Option<PathBuf>,
}

impl Carrier {
    /// Nośnik leżący w bibliotece tego człowieka.
    #[must_use]
    pub fn in_library(library: Option<&Path>) -> Self {
        Self {
            path: library.map(|dir| dir.join(FILE)),
        }
    }

    /// Wartość dla tej nazwy: najpierw środowisko okna, potem nośnik.
    #[must_use]
    pub fn value_of(&self, name: &str) -> Option<OsString> {
        std::env::var_os(name).or_else(|| self.written_in_the_file(name))
    }

    /// Zdanie do odmowy: gdzie Loadout tej wartości szukał.
    #[must_use]
    pub fn where_loadout_reads_it_from(&self) -> String {
        self.path.as_deref().map_or_else(
            || ENVIRONMENT_ONLY.to_owned(),
            |path| format!("{ENVIRONMENT_ONLY} and from {}", path.display()),
        )
    }

    fn written_in_the_file(&self, name: &str) -> Option<OsString> {
        let path = self.path.as_deref()?;
        // Pytanie o prawa dostępu jest pojęciem systemu, więc odpowiada na nie plik, w którym
        // mieszka kod platformowy (niezmiennik 3). Plik, którego nie ma, po prostu nic nie mówi
        // — to jest stan każdego, kto nigdy go nie założył (2026-09, Z-23).
        if !path.is_file() {
            return None;
        }
        if crate::engine::supervisor::others_can_read_it(path) {
            tracing::warn!(
                carrier = %path.display(),
                "Loadout did not read this file of values because other accounts on this machine \
                 can read it too; give it back to yourself with chmod 600"
            );
            return None;
        }
        let text = fs::read_to_string(path)
            .inspect_err(|error| {
                tracing::warn!(carrier = %path.display(), %error, "this file of values could not be read");
            })
            .ok()?;
        text.lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .find(|(key, _)| key.trim() == name)
            .map(|(_, value)| OsString::from(value.trim()))
    }
}
