//! Graf kroków: listy sąsiedztwa, stopnie wejściowe i **odmowa cyklu przy konstrukcji**.
//!
//! `petgraph` jest tu świadomie nieobecny [T7 §9.1]: listy sąsiedztwa plus algorytm Kahna
//! wystarczą, a cykl jako *ścieżka do narysowania* będzie potrzebny dopiero, kiedy edytor
//! będzie musiał go pokazać [T7 §9.4].
//!
//! **Dlaczego odmowa mieszka w [`Dag::new`], a nie w osobnym `is_acyclic()`.** Osobna funkcja,
//! którą trzeba pamiętać, żeby zawołać, jest funkcją, której się nie woła. Planista dostaje
//! wtedy graf bez ani jednego korzenia, jego pętla kończy się przy `inflight == 0` w pierwszym
//! obrocie i **melduje sukces biegu, w którym nic nie biegło**. Typ zwrotny `Result` sprawia,
//! że pominięcie sprawdzenia przestaje być możliwe.

use std::fmt;

use super::StepId;

/// Powód, dla którego graf nie powstał.
///
/// Dwa warianty, bo to są dwie różne pomyłki człowieka i UI kiedyś powie o nich dwa różne
/// zdania. Komunikaty dla użytkownika buduje jednak T-12 przy zapisie workflow — odmowa tutaj
/// jest **ostatnią linią obrony, nie pierwszą**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DagError {
    /// Zbiór węzłów, których algorytm Kahna nie zdołał przetworzyć. Każdy z nich leży na cyklu
    /// albo poniżej niego; pusty być nie może, bo inaczej cyklu nie było.
    Cycle {
        /// Węzły, które zostały. Komunikat nazywa je po numerze — bez tego „graf ma cykl"
        /// jest zdaniem, z którym nie da się nic zrobić.
        nodes: Vec<StepId>,
    },
    /// Krawędź celująca w węzeł, którego w grafie nie ma. Osobny wariant, bo to jest literówka
    /// w danych wejściowych, a nie kolejność kroków.
    UnknownNode {
        /// Krawędź, w której to znaleziono.
        edge: (StepId, StepId),
        /// Koniec tej krawędzi, który nie istnieje.
        node: StepId,
    },
}

impl fmt::Display for DagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cycle { nodes } => {
                write!(f, "these steps depend on one another in a circle: ")?;
                for (position, node) in nodes.iter().enumerate() {
                    if position > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{node}")?;
                }
                Ok(())
            }
            Self::UnknownNode { edge, node } => write!(
                f,
                "the link {} -> {} names step {node}, which this graph does not have",
                edge.0, edge.1
            ),
        }
    }
}

impl std::error::Error for DagError {}

/// Graf kroków, którego nie da się zbudować z cyklem.
#[derive(Debug, Clone)]
pub struct Dag {
    /// Rodzice każdego węzła. Stopień wejściowy to długość tej listy — planista dekrementuje
    /// jego kopię, nigdy sam graf, żeby ten sam `Dag` dało się uruchomić drugi raz.
    deps: Vec<Vec<StepId>>,
    /// Dzieci każdego węzła, czyli te same krawędzie odwrócone. Trzymane obok `deps`, bo
    /// planista potrzebuje obu kierunków w każdym obrocie pętli i odwracanie w locie byłoby
    /// jedynym kosztem, który w tych rozmiarach grafu w ogóle widać.
    children: Vec<Vec<StepId>>,
}

impl Dag {
    /// Buduje graf z `n` węzłów o numerach `0..n` i podanych krawędzi `(rodzic, dziecko)`.
    ///
    /// Odmawia cyklu i krawędzi do nieistniejącego węzła. Pętla własna `(i, i)` jest cyklem
    /// długości jeden i wychodzi tym samym wariantem.
    pub fn new(n: usize, edges: &[(StepId, StepId)]) -> Result<Self, DagError> {
        // SZKIELET (2026-08-15) — świadomie zła odpowiedź: przyjmuje KAŻDE wejście, także cykl
        // i krawędź w próżnię, i nie zapamiętuje ani jednej krawędzi. Dzięki temu AC-4 pada na
        // braku `Err` oraz na stopniach wejściowych, czyli na ZACHOWANIU, a nie na tym, że plik
        // się nie ładuje (AGENTS.md §2a p. 5).
        //
        // Implementacja: najpierw zakres każdego końca każdej krawędzi (`UnknownNode`), potem
        // Kahn po kopii stopni wejściowych i porównanie liczby przetworzonych węzłów z `n`.
        // Kahn jest tu wybrany nie dla szybkości, tylko dlatego, że przypadek
        // `[(0,1),(1,2),(2,1)]` — cykl przy istniejącym korzeniu — przechodzi każde tańsze
        // sprawdzenie w rodzaju „czy istnieje węzeł o stopniu 0".
        let _ = edges;
        Ok(Self {
            deps: vec![Vec::new(); n],
            children: vec![Vec::new(); n],
        })
    }

    /// Liczba węzłów.
    #[must_use]
    pub fn len(&self) -> usize {
        self.deps.len()
    }

    /// Czy graf jest pusty. Bieg pustego grafu jest legalny i kończy się od razu.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.deps.is_empty()
    }

    /// Dzieci każdego węzła, indeksowane numerem węzła.
    #[must_use]
    pub fn children(&self) -> &[Vec<StepId>] {
        &self.children
    }

    /// Stopnie wejściowe, indeksowane numerem węzła. Planista pracuje na **kopii**.
    #[must_use]
    pub fn in_degree(&self) -> Vec<usize> {
        self.deps.iter().map(Vec::len).collect()
    }
}
