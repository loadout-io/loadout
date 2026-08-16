//! Mennica identyfikatorów. Jedna, dla wszystkich sekcji.
//!
//! Dlaczego to jest komenda, a nie `crypto.randomUUID()` w oknie: tamto daje uuid **v4**, czyli
//! liczbę losową, a Loadout wymaga **v7** — sortowalnego po czasie [T4 §5.1]. Lista agentów
//! i lista biegów układają się wtedy w kolejności powstania bez ani jednego pola z datą, a
//! `id` pozostaje stabilne przez zmianę nazwy.
//!
//! Jedna mennica, nie jedna na sekcję: dwie funkcje wybijające identyfikatory to dwie
//! odpowiedzi na pytanie „jak wygląda nowy identyfikator", i pierwsza z nich, która zostanie
//! przepisana, rozjedzie się po cichu (niezmiennik 23).

use uuid::Uuid;

/// Świeży identyfikator uuid v7.
///
/// Zegar jest tu i tylko tu: `now_v7` czyta czas systemowy, więc wołający dostaje wartość,
/// której nie da się podać z zewnątrz. To jest właściwa strona granicy — front, który wybija
/// identyfikatory sam, wybija je v4 i traci porządek, a `at` podawany argumentem (jak
/// w `memory::notes::Actor`) opisuje **czynność człowieka**, nie moment powstania nazwy.
#[must_use]
pub fn new_id_inner() -> Uuid {
    Uuid::now_v7()
}
