//! Stawki modeli: tabela wbudowana plus to, co człowiek dopisał w `~/.loadout/prices.json`.
//!
//! # Po co plik istnieje obok tabeli (2026-09, Z-44)
//!
//! Bo vendor dokłada modele częściej, niż Loadout ma wydania. Do tego dnia tabela była stałą
//! w `codex.rs`, więc model, którego w niej nie było, nie miał ceny **do następnej wersji
//! aplikacji** — a bez ceny jego tury nie wchodziły ani do sumy wydatku, ani do udziału
//! w sufcie biegu (Z-13b). Sufit 275 USD z Settings był dla takiego kroku napisem.
//!
//! Plik jest jedną drogą wyjścia, którą człowiek ma pod ręką: dopisuje w nim prefiks modelu
//! i trzy stawki, i ten sam krok rusza pod tym samym sufitem, jeszcze tego samego dnia.
//!
//! # Trzy rzeczy, które ten plik robi świadomie
//!
//! * **Plik wygrywa z tabelą wbudowaną.** Odwrotna kolejność znaczyłaby, że stawka wpisana ręką
//!   po podwyżce u vendora jest cicho ignorowana na rzecz liczby sprzed wydania — czyli że
//!   jedyna droga naprawy nie działa dokładnie wtedy, kiedy jest potrzebna.
//! * **Pliku, którego nie ma, nie ma i tyle.** Tak wygląda każda maszyna, na której nikt go nie
//!   założył; ten sam wybór, co przy [`crate::connections::secrets::Carrier`].
//! * **Plik, którego nie da się przeczytać, jest ODMOWĄ, nie ciszą.** Cichy powrót do tabeli
//!   wbudowanej znaczyłby, że literówka w przecinku zmienia cenę biegu i nikt się o tym nie
//!   dowie. Zdanie odmowy nazywa plik, bo to jedyna rzecz, którą człowiek ma poprawić.
//!
//! Kolumny są trzy i zostają rozdzielone: zsumowana stawka gubi informację, bez której nie da
//! się policzyć prawdziwego użycia — wejście z cache'u kosztuje u każdego vendora ułamek
//! świeżego.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use anyhow::anyhow;
use serde::Deserialize;

use super::Tokens;

/// Nazwa pliku w bibliotece. `~/.loadout/prices.json`, czyli obok `agents/` i `env`.
const FILE: &str = "prices.json";

/// Jak ten plik nazywa się w zdaniu dla człowieka. Jedno miejsce, bo tę ścieżkę wypisuje także
/// odmowa startu w `commands::run` — a jedno zdanie w dwóch kopiach rozjeżdża się przy pierwszej
/// zmianie jednej z nich (niezmiennik 13).
pub const WHERE_PRICES_LIVE: &str = "~/.loadout/prices.json";

/// Stawki jednego modelu, w dolarach za milion tokenów.
#[derive(Debug, Clone, Copy, Deserialize)]
struct Rate {
    /// Świeże wejście, czyli to, za które płaci się pełną stawkę.
    input: f64,
    /// Wejście przeczytane z cache'u.
    cached: f64,
    /// Wyjście modelu.
    output: f64,
}

/// Jedyna tabela cen, którą Loadout zna z siebie. Klucz jest **prefiksem** nazwy modelu, bo
/// vendor dokleja do niej datę wydania (`gpt-5.6-terra-2026-08-25`).
const BUILT_IN: &[(&str, Rate)] = &[
    (
        "gpt-5.6-sol",
        Rate {
            input: 2.0,
            cached: 0.4,
            output: 21.0,
        },
    ),
    (
        "gpt-5.6-terra",
        Rate {
            input: 1.0,
            cached: 0.2,
            output: 12.5,
        },
    ),
    (
        "gpt-5.6-luna",
        Rate {
            input: 0.1,
            cached: 0.02,
            output: 1.25,
        },
    ),
];

/// Co ten bieg wie o cenach, kiedy rusza.
///
/// Wczytywane RAZ, przy planowaniu (`commands::run::Plan::prices`), z tego samego powodu, co
/// nośnik sekretów obok: biblioteka jest znana przed pierwszym procesem, a dwa kroki tego samego
/// biegu mają pytać ten sam plik. Plik poprawiony w połowie biegu nie ma prawa zmienić ceny
/// kroku, który już rusza.
#[derive(Debug, Clone, Default)]
pub struct Prices {
    /// Stawki dopisane ręką człowieka, po jednej na prefiks. Puste na maszynie, na której nikt
    /// pliku nie założył — i wtedy zostaje sama tabela wbudowana.
    written_down: Vec<(String, Rate)>,
}

impl Prices {
    /// Tabela wbudowana, nadpisana tym, co człowiek dopisał w bibliotece.
    ///
    /// `Err` niesie GOTOWE zdanie dla człowieka, nie kod błędu: jedynym wołającym jest odmowa
    /// startu biegu, a ona nie ma z czego zbudować lepszego zdania niż to, które wie, o który
    /// plik chodziło i co się w nim nie zgadzało.
    pub fn from_library(library: &Path) -> anyhow::Result<Self> {
        let path = library.join(FILE);
        // Plik, którego nie ma, po prostu nic nie mówi — powód w całości w nagłówku modułu.
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path).map_err(|error| cannot_be_read(&path, &error))?;
        let written: BTreeMap<String, Rate> =
            serde_json::from_str(&text).map_err(|error| cannot_be_read(&path, &error))?;
        Ok(Self {
            written_down: written.into_iter().collect(),
        })
    }

    /// Czy da się powiedzieć, ile kosztuje tura tego modelu.
    ///
    /// `None` na modelu jest odpowiedzią „nie", a nie brakiem pytania: krok, który nie powiedział,
    /// czym jedzie, dostaje cenę od vendora dopiero z drutu — czyli już po zapłaceniu za turę.
    #[must_use]
    pub fn knows(&self, model: Option<&str>) -> bool {
        model.is_some_and(|model| self.rate_for(model).is_some())
    }

    /// Ile kosztowała tura o takim zużyciu — albo `None`, kiedy stawki nie zna nikt.
    ///
    /// Nieznana cena zostaje `None` i nigdy nie staje się zerem: zero wygląda jak tura darmowa,
    /// czyli jak zgoda na dalsze wydawanie.
    #[must_use]
    pub fn estimate(&self, model: &str, tokens: Tokens) -> Option<f64> {
        let rate = self.rate_for(model)?;
        // 2026-09 (Z-12): `input_tokens` Codeksa już zawiera `cached_input_tokens`. Liczenie obu
        // kolumn jako osobnych wejść płaciło za cache dwa razy — w pamięci projektu pokazało
        // 23,68 USD zamiast 5,53 USD — więc pełną stawkę dostają wyłącznie świeże tokeny.
        let fresh_input = tokens.input.saturating_sub(tokens.cached);
        // `From<u64> for f64` nie istnieje. Parsowanie dziesiętnego zapisu zachowuje pełny zakres
        // licznika bez ryzykownego, wyciszanego rzutowania; dla każdego `u64` wynik jest skończony.
        let input = fresh_input.to_string().parse::<f64>().ok()?;
        let cached = tokens.cached.to_string().parse::<f64>().ok()?;
        let output = tokens.output.to_string().parse::<f64>().ok()?;
        Some((input * rate.input + cached * rate.cached + output * rate.output) / 1_000_000.0)
    }

    fn rate_for(&self, model: &str) -> Option<Rate> {
        /* NAJDŁUŻSZY PASUJĄCY PREFIKS, nie pierwszy z brzegu (2026-09, Z-44). Plik człowieka jest
         * mapą, a mapa nie ma kolejności, którą dałoby się obiecać: przy dwóch wpisach `gpt-5.6`
         * i `gpt-5.6-sol` o cenie decydowałoby to, jak serde ułoży klucze. Wpis bardziej
         * szczegółowy wygrywa i to jest jedyna odpowiedź, która nie zależy od kolejności. */
        self.written_down
            .iter()
            .filter(|(prefix, _)| model.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(_, rate)| *rate)
            .or_else(|| {
                // Prefiksy wbudowane są rozłączne, więc pierwszy pasujący jest jedynym pasującym.
                BUILT_IN
                    .iter()
                    .find(|(prefix, _)| model.starts_with(prefix))
                    .map(|(_, rate)| *rate)
            })
    }
}

/// Jedno zdanie na obie drogi, którymi ten plik potrafi się nie udać: nieczytelny bajt i JSON,
/// którego nie da się rozebrać. Dla człowieka to jest ten sam problem i ta sama naprawa.
fn cannot_be_read(path: &Path, why: &impl fmt::Display) -> anyhow::Error {
    anyhow!(
        "Loadout could not read the prices in {}: {why}. Fix that file or move it out of the \
         way, and start the run again.",
        path.display()
    )
}
