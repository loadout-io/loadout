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
#[must_use]
pub fn new_id_inner() -> Uuid {
    todo!("hand out a fresh v7 id, the kind that sorts by the time it was made")
}
