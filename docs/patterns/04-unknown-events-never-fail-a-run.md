# 04 — Nieznane zdarzenie nigdy nie wywala biegu

Vendorzy dokładają typy zdarzeń co tydzień, po cichu, bez wersjonowania schematu.
`system/init` Claude'a niesie dziś 24 klucze. Jutro 25.

Bieg, który padł, bo dostał zdarzenie, którego nie znamy, to **nasz** błąd, nie ich.

## Trzy linie obrony

```rust
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClaudeLine {
    System    { subtype: String, #[serde(flatten)] rest: serde_json::Value },
    Assistant { message: AssistantMsg, parent_tool_use_id: Option<String> },
    Result    { #[serde(flatten)] r: ResultMsg },

    #[serde(other)]
    Unknown,          // ← 1. nieznany wariant nie jest błędem
}

struct ResultMsg {
    is_error: bool,
    terminal_reason: Option<String>,   // ← 2. Option na wszystkim nieistotnym
    total_cost_usd: Option<f64>,
    #[serde(flatten)]
    rest: serde_json::Value,           // ← 3. nic nie ginie po cichu
}
```

Nieznaną linię **logujemy do pliku debug i porzucamy z UI**. Nigdy nie przerywamy biegu.

## Nie czytaj `subtype`

Zweryfikowane: `subtype` zaraportował `"success"` na biegu, który padł.
Prawdę o zakończeniu niosą `is_error` i `terminal_reason`:

```rust
let reason = if !r.is_error                            { Completed }
             else if r.terminal_reason == Some("cancelled") { Cancelled }
             else if r.subtype.starts_with("error_max")     { LimitReached }
             else { Failed(r.result.clone()) };
```

Wyjście procesu to sygnał **drugorzędny**: „zakończył się bez zdarzenia `result`" = `Failed`.

## Test regresyjny

Wstrzyknij do złotego strumienia linię z wymyślonym `"type": "quantum_flux"` i udowodnij,
że bieg kończy się normalnie, a linia ląduje w pliku debug.

Ten jeden test jest wart więcej niż cała reszta obsługi błędów parsera.
