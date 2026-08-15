# 01 — Sprawdzaj skutek, nie kształt

**Kryterium, które spełnia tożsamość, nie certyfikuje niczego.**

Repo źródłowe (`~/Projects/spreadsheet`) wypuściło dwa zielone kryteria, które nie sprawdzały nic:
oba były algebraiczne. `f(f(x)) === x` przechodzi, kiedy `f` jest funkcją tożsamościową —
czyli kiedy parser nic nie robi.

## Zakazane kształty kryteriów

| Kształt | Dlaczego nie działa |
|---|---|
| `f(f(x)) === x` | tożsamość spełnia |
| `parse(render(x)) === x` | pusty render + pusty parse spełnia |
| `result.length > 0` | jeden śmieciowy element spełnia |
| `expect(fn).not.toThrow()` | funkcja z pustym ciałem spełnia |
| `expect(spy).toHaveBeenCalled()` | wywołanie z błędnymi argumentami spełnia |
| `expect(html).toContain('button')` | dowolny przycisk gdziekolwiek spełnia |

## Zamiast tego

Nazwij **konkretne wejście i konkretny oczekiwany skutek**, taki, którego zła implementacja nie da.

```rust
// źle: przechodzi, gdy splitter zwraca całą linię jako jedno pole
assert_eq!(parse(join(fields)), fields);

// dobrze: wartość, którą daje tylko poprawna obsługa cudzysłowów
assert_eq!(
    parse(r#"a,"b,c",d"#),
    vec!["a", "b,c", "d"],
    "przecinek w cudzysłowie nie może dzielić pola"
);
```

## Słaba asercja

Każde kryterium akceptacji w `tasks/` kończy się linią `*Słaba asercja:*`, która nazywa
**implementację przechodzącą sprawdzenie i łamiącą kryterium** — oraz dodatkową asercję,
która je rozróżnia.

To jest dokładnie to pytanie, które dostaje recenzent w drugiej opinii:
*czy implementacja spełnia KRYTERIUM, czy tylko ASERCJĘ napisaną pod nie?*

Jeśli nie umiesz napisać słabej asercji, kryterium jest albo trywialne, albo źle sformułowane.
