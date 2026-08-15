# 05 — Dziedzicz różnicę, nie kopiuj wartości

Krok w workflow bierze ustawienia z agenta. Kiedy zmienisz coś **w kroku**, domyślne
u agenta muszą zostać nietknięte. To jest wprost postawione wymaganie użytkownika.

Naiwne rozwiązanie — skopiować pola agenta do kroku przy tworzeniu — łamie je w drugą stronę:
poprawka w agencie nie dociera do kroków, które powstały wcześniej. Dostajesz dryf,
którego nikt nie widzi, dopóki dwa kroki nie zaczną zachowywać się różnie bez powodu.

## Model

Krok trzyma **wyłącznie różnicę** wobec szablonu agenta (JSON Merge Patch, RFC 7386).

```jsonc
// ~/.loadout/agents/forge.json  — szablon
{ "model": "opus", "thinking": "balanced", "fileAccess": "ask", "giveUpAfter": 20 }

// krok w workflow — TYLKO to, co nadpisane
{ "agent": "forge", "overrides": { "fileAccess": "free", "giveUpAfter": 40 } }
```

Efektywna konfiguracja liczona przy uruchomieniu:

```rust
let effective = agent_template.merge_patch(&step.overrides);
```

Konsekwencje, które są zaletami:
- zmiana modelu u agenta dociera do **wszystkich** kroków, które go nie nadpisały,
- nadpisanie w kroku nie dotyka agenta — bo krok nigdy nie pisze do jego pliku,
- `null` w łatce znaczy „przywróć domyślne" i jest jedyną drogą do usunięcia nadpisania.

## Pokaż to w UI

Użytkownik musi widzieć, że coś jest nadpisane, bez otwierania pliku.

```
WHO DOES THIS   [2 changed]
Forge — writes code · Claude Opus
Inherited from the agent. Changing it here does not change the agent.
```

Znacznik liczy klucze w `overrides`. Kliknięcie pokazuje które i jakie były domyślne.

## Czego nie robimy

**Nie hashujemy konfiguracji agenta do tożsamości planu.** poprzedni prototyp tak robił
(`AgentConfigurationRevision` wciągał digest każdej umiejętności do digestu planu) —
skutek był taki, że edycja umiejętności unieważniała zatwierdzony plan.
To jest aktywnie wrogie, kiedy ktoś iteruje nad umiejętnościami.

Zamiast tego: baner „ustawienia się zmieniły od zatwierdzenia" i decyzja należy do człowieka.
