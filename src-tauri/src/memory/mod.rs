//! Pamięć Loadouta: przekazania między krokami biegu (tu, w [`handoff`]) i notatki (T-17).
//!
//! Ten plik trzyma **wyłącznie to, co wspólne dla obu**: płaski czytnik/pisarz front-mattera,
//! [`est_tokens`] i [`slugify`]. T-17 z tego korzysta i nie pisze drugiej kopii — dwa czytniki
//! tego samego formatu rozjeżdżają się w tydzień, a rozjazd widać dopiero wtedy, gdy jeden
//! z nich czyta plik zapisany przez drugi. Jedna polityka, jeden rdzeń (niezmiennik 23).
//!
//! Czego tu nie ma i nie będzie: `Connection`. Pamięć zwraca struktury, wiersz do `SQLite`
//! wkłada `store::writer` i nikt inny (niezmiennik 2). Drugie połączenie zapisujące to
//! zakleszczenie, nie „czasem wolniej".

use std::path::PathBuf;

pub mod handoff;
pub mod notes;

/// Błędy pamięci — wspólne dla przekazań i notatek.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// Plik bez bloku `---` otwartego na bajcie 0. To nie jest przekazanie, tylko markdown.
    ///
    /// `path.display()`, nie `{path}`: `PathBuf` nie ma `Display` (bo ścieżka nie musi być
    /// poprawnym UTF-8), więc skrót thiserrora nie kompiluje się na tym polu.
    #[error("{} opens with no front-matter block", path.display())]
    NoFrontMatter { path: PathBuf },

    /// Korekta wskazuje na `id`, którego w tym katalogu biegu nie ma.
    #[error("this run has nothing with id {id}")]
    NoSuchHandoff { id: String },

    /// Druga korekta tego samego przekazania. Historia biegu ma zostać prawdziwa: plik
    /// oddał już swoje miejsce następnikowi i drugi raz nie ma czego oddać [T6 §9].
    #[error("{id} was already corrected once")]
    AlreadySuperseded { id: String },
}

/// Skrót używany przez cały moduł pamięci.
pub type Result<T> = std::result::Result<T, Error>;

/// Płaska mapa `klucz: wartość` z zachowaną kolejnością kluczy, z dwoma polami listowymi.
///
/// Ręcznie, a nie `gray_matter`: `src-tauri/Cargo.toml` nie należy do T-16, więc dołożenie
/// zależności jest pytaniem do człowieka (AGENTS.md §7), nie dopiskiem. Niezależnie od tego
/// ręczny czytnik jest tu **lepszy**: AC-1 wymaga, żeby dokładnie wiadomo było, co zostało
/// sparsowane, a co jest tylko tekstem w ciele. `serde_yaml` odpada osobno — ostatnie wydanie
/// to `0.9.34+deprecated` [T6 §7.3].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FrontMatter {
    pairs: Vec<(String, String)>,
}

impl FrontMatter {
    /// Rozbiera plik na front-matter i **offset bajtowy ciała**.
    ///
    /// Offset, a nie sam wycinek, bo AC-1 pyta wprost o to, czy sfałszowany blok agenta leży
    /// za zamknięciem front-mattera — a na to pytanie odpowiada tylko liczba.
    ///
    /// Zamknięciem jest **pierwszy** wiersz `---` po wierszu otwierającym. Dzięki temu blok,
    /// który agent wkleił do ciała, nie ma jak zostać wzięty za nagłówek: parser kończy pracę
    /// zanim do niego dojdzie, a wszystko dalej jest tekstem, którego nikt nie interpretuje.
    ///
    /// 2026-08-16: wiersz bez dwukropka jest **pomijany**, nie jest błędem (niezmiennik 5).
    /// Plik po ręcznej edycji albo od nowszego Loadouta ma się odczytać, a nie wywrócić skan
    /// całego katalogu.
    ///
    /// Ścieżka w [`Error::NoFrontMatter`] jest tu pusta, bo parser widzi sam tekst.
    /// Uzupełnia ją [`handoff::read_handoff`], które jako jedyne wie, skąd ten tekst przyszedł.
    pub fn split(file: &str) -> Result<(Self, usize)> {
        if !file.starts_with("---\n") {
            return Err(Error::NoFrontMatter {
                path: PathBuf::new(),
            });
        }

        let mut pairs: Vec<(String, String)> = Vec::new();
        let mut at = 4;
        while at < file.len() {
            let end = file[at..].find('\n').map_or(file.len(), |i| at + i + 1);
            let line = file[at..end].trim_end();

            if line == "---" {
                // Kontrakt [T6 §10.2]: dokładnie jeden pusty wiersz separatora między
                // zamknięciem a ciałem. Jest częścią nagłówka, nie ciała — gdyby wpadł do
                // ciała, `bytes` liczyłoby bajt, którego agent nie napisał.
                let body_at = usize::from(file[end..].starts_with('\n')) + end;
                return Ok((Self { pairs }, body_at));
            }

            if let Some((key, value)) = line.split_once(':') {
                pairs.push((key.trim().to_owned(), value.trim().to_owned()));
            }
            at = end;
        }

        Err(Error::NoFrontMatter {
            path: PathBuf::new(),
        })
    }

    /// Blok od `---` do `---` włącznie z zamykającą nową linią. Ciało dokleja wołający.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::from("---\n");
        for (key, value) in &self.pairs {
            out.push_str(key);
            out.push_str(": ");
            out.push_str(value);
            out.push('\n');
        }
        out.push_str("---\n");
        out
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// Pole listowe (`to`, `reads`) — jedyne dwa, które nie są płaskim stringiem [T6 §10.2].
    #[must_use]
    pub fn list(&self, key: &str) -> Option<Vec<String>> {
        let raw = self.get(key)?.trim();
        if raw.is_empty() || raw == "null" {
            return Some(Vec::new());
        }
        // Bez nawiasów wartość jest jednym elementem, nie błędem: tak wygląda plik pisany
        // ręcznie, a niezmiennik 5 mówi, że ma się odczytać.
        let inner = raw
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(raw);
        Some(
            inner
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        )
    }

    pub fn set(&mut self, key: &str, value: &str) {
        // Nadpisanie w miejscu, nie „skasuj i dopisz": kolejność kluczy jest kontraktem
        // [T6 §10.2], a przestawienie jednego wiersza w pliku, który ktoś już przeczytał,
        // wygląda w diffie jak zmiana treści.
        if let Some(slot) = self.pairs.iter_mut().find(|(name, _)| name == key) {
            value.clone_into(&mut slot.1);
        } else {
            self.pairs.push((key.to_owned(), value.to_owned()));
        }
    }

    pub fn set_list(&mut self, key: &str, values: &[String]) {
        self.set(key, &format!("[{}]", values.join(", ")));
    }

    /// Klucze w kolejności zapisu — po to, żeby czytający wiedział, co było w pliku,
    /// a nie tylko to, czego się spodziewał.
    #[must_use]
    pub fn keys(&self) -> Vec<&str> {
        self.pairs.iter().map(|(key, _)| key.as_str()).collect()
    }
}

/// Szacunek długości: ~4 bajty na jednostkę [T6 §10.2].
///
/// Szacunek, nie pomiar. Służy budżetowi promptu i paskowi w UI, nie rozliczeniu — prawdziwe
/// liczby przychodzą z `--output-format json` po zakończeniu kroku [T6 §8].
#[must_use]
pub fn est_tokens(bytes: usize) -> usize {
    bytes.div_ceil(4)
}

/// Nazwa pliku jest funkcją Loadouta, nie tekstem od agenta.
///
/// Zwraca slug pasujący do `^[a-z0-9]+(-[a-z0-9]+)*$`. Wejście, z którego nie zostaje ani
/// jeden dozwolony znak (same białe znaki, sama interpunkcja, `../..`), degraduje się do
/// `agent` — pusty człon nazwy pliku jest sposobem, w jaki `01____brief.md` przestaje dać się
/// odczytać z powrotem na trzy pola.
///
/// 2026-08-16, AC-6: to jest **lista dozwolonych**, nigdy lista zakazanych. `replace("../", "")`
/// przechodzi na `../../etc/passwd` i pada na `....//x`, bo po skasowaniu obu `../` z `....//`
/// zostaje `../`. Tu nie ma czego omijać: znak spoza `[a-z0-9]` nie przeżywa, więc ani `/`,
/// ani `.`, ani `\0` nie mają jak trafić do nazwy pliku.
#[must_use]
pub fn slugify(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut pending = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending && !out.is_empty() {
                out.push('-');
            }
            pending = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending = true;
        }
    }

    if out.is_empty() {
        // `01____brief.md` nie rozbija się z powrotem na trzy pola, więc pusty człon jest
        // gorszy niż stała.
        return "agent".to_owned();
    }
    out
}
