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
    pub fn split(file: &str) -> Result<(Self, usize)> {
        todo!("T-16: płaski parser front-mattera")
    }

    /// Blok od `---` do `---` włącznie z zamykającą nową linią. Ciało dokleja wołający.
    pub fn render(&self) -> String {
        todo!("T-16: płaski pisarz front-mattera")
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        todo!("T-16: odczyt pojedynczej wartości")
    }

    /// Pole listowe (`to`, `reads`) — jedyne dwa, które nie są płaskim stringiem [T6 §10.2].
    pub fn list(&self, key: &str) -> Option<Vec<String>> {
        todo!("T-16: odczyt pola listowego")
    }

    pub fn set(&mut self, key: &str, value: &str) {
        todo!("T-16: zapis pojedynczej wartości")
    }

    pub fn set_list(&mut self, key: &str, values: &[String]) {
        todo!("T-16: zapis pola listowego")
    }

    /// Klucze w kolejności zapisu — po to, żeby czytający wiedział, co było w pliku,
    /// a nie tylko to, czego się spodziewał.
    pub fn keys(&self) -> Vec<&str> {
        todo!("T-16: lista kluczy w kolejności z pliku")
    }
}

/// Szacunek długości: ~4 bajty na jednostkę [T6 §10.2].
///
/// Szacunek, nie pomiar. Służy budżetowi promptu i paskowi w UI, nie rozliczeniu — prawdziwe
/// liczby przychodzą z `--output-format json` po zakończeniu kroku [T6 §8].
pub fn est_tokens(bytes: usize) -> usize {
    todo!("T-16: bytes.div_ceil(4)")
}

/// Nazwa pliku jest funkcją Loadouta, nie tekstem od agenta.
///
/// Zwraca slug pasujący do `^[a-z0-9]+(-[a-z0-9]+)*$`. Wejście, z którego nie zostaje ani
/// jeden dozwolony znak (same białe znaki, sama interpunkcja, `../..`), degraduje się do
/// `agent` — pusty człon nazwy pliku jest sposobem, w jaki `01____brief.md` przestaje dać się
/// odczytać z powrotem na trzy pola.
pub fn slugify(raw: &str) -> String {
    todo!("T-16: slugifikacja nazw plików")
}
