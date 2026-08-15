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
        let mut deps: Vec<Vec<StepId>> = vec![Vec::new(); n];
        let mut children: Vec<Vec<StepId>> = vec![Vec::new(); n];

        // Zakres PRZED wszystkim innym. Krawędź w próżnię zaindeksowałaby wektor poza końcem,
        // a to jest panika — czyli koniec całego biegu agentów zamiast odmowy zapisu jednego
        // workflow (AGENTS.md §4: żadnej paniki w silniku).
        for &(parent, child) in edges {
            if parent >= n {
                return Err(DagError::UnknownNode {
                    edge: (parent, child),
                    node: parent,
                });
            }
            if child >= n {
                return Err(DagError::UnknownNode {
                    edge: (parent, child),
                    node: child,
                });
            }
            deps[child].push(parent);
            children[parent].push(child);
        }

        // Kahn na KOPII stopni wejściowych. Wybrany nie dla szybkości, tylko dlatego, że
        // `[(0,1),(1,2),(2,1)]` — cykl pod istniejącym korzeniem — przechodzi każde tańsze
        // sprawdzenie w rodzaju „czy istnieje węzeł o stopniu 0": węzeł 0 taki jest. Przewraca
        // to dopiero liczenie węzłów, które udało się zdjąć.
        let mut remaining: Vec<usize> = deps.iter().map(Vec::len).collect();
        let mut queue: Vec<StepId> = (0..n).filter(|&id| remaining[id] == 0).collect();
        let mut settled = vec![false; n];
        while let Some(id) = queue.pop() {
            settled[id] = true;
            for &child in &children[id] {
                remaining[child] -= 1;
                if remaining[child] == 0 {
                    queue.push(child);
                }
            }
        }

        // Co zostało, leży na cyklu albo pod nim. Pętla własna `(i, i)` wychodzi tędy sama:
        // jest cyklem długości jeden i nigdy nie schodzi do stopnia 0.
        let stuck: Vec<StepId> = (0..n).filter(|&id| !settled[id]).collect();
        if !stuck.is_empty() {
            return Err(DagError::Cycle { nodes: stuck });
        }

        Ok(Self { deps, children })
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
