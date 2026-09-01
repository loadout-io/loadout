# h — mały harness Loadouta

Jedno zadanie, jedna komenda, koniec.

```bash
scripts/h run <id> --prompt "co ma powstać"
```

```
worktree → plan → implementacja → checki + weryfikacja
                       ↑                    │
                       └──── max 2 poprawki ┘
```

Weryfikator odpowiada na jedno pytanie: **czy to zadanie zostało zrobione i czy ta
funkcjonalność działa?** Nie recenzuje kodu, nie żąda dowodów, nie proponuje ulepszeń.
Trzy wyjścia: `DZIALA`, `NIE_DZIALA` + co konkretnie, `NIE_WIEM`. Przy `NIE_DZIALA` ten sam
agent implementacyjny dostaje werdykt i poprawia — przez `claude --continue`, więc pamięta,
co już próbował. Po dwóch nieudanych poprawkach STOP i pytanie do człowieka; worktree zostaje.

## Komendy

```
run <id> --prompt "…"   całość
check [<nazwa>]         checki dla zmienionych ścieżek, albo jeden po nazwie
list                    otwarte zadania
status <id>             stan
land <id>               merge gałęzi + PEŁNE CI na trunku
clean <id>              usuń worktree + gałąź
```

## Vendorzy

Wysiłek jest **per faza**, nie na cały bieg. `LOADOUT_CLAUDE_EFFORT` rządzi planem
(domyślnie `max`), `LOADOUT_CLAUDE_EFFORT_DEV` implementacją (domyślnie to samo, co plan).
Zmierzone 2026-08-28 na dwóch biegach: plan 10 i 12 min, implementacja 30 i 49 min, checki
25 s, weryfikacja 4,5 min. Implementacja to 3–5× plan, a plan jest tą fazą, która w obu
biegach poprawiła przesłankę zlecenia — więc tanieje się na implementacji, nie na planie.

Model też jest **per rola**: `LOADOUT_CLAUDE_MODEL` dla planu i implementacji,
`LOADOUT_CLAUDE_MODEL_VERIFIER` dla weryfikacji (domyślnie to samo). Ta druga istnieje po to,
żeby para **same-vendor** była uczciwa: D3 wymaga wtedy innego modelu, bo ten sam model dwa
razy nie jest drugą opinią. Przykład, gdy jeden vendor jest niedostępny:
`scripts/h run <id> --verifier claude` z `LOADOUT_CLAUDE_MODEL_VERIFIER=claude-sonnet-5`.

Domyślnie **plan: claude, kod: claude, weryfikacja: codex** — weryfikuje inny vendor niż ten,
który pisał (decyzja D3). Zmiana: `--planner/--dev/--verifier` albo `H_PLANNER`/`H_DEV`/`H_VERIFIER`.

## Checki

`checks.json` mapuje zmienione ścieżki na komendy. Dwie rzeczy, na których stoi cała wydajność:

1. **Check biegnie tylko wtedy, gdy jego ścieżki się zmieniły.** Zmiana w `src/` nie odpala
   `cargo test`.
2. **Check jest zawężany do tego, co zmienione** (`"scoped"`). Rustowy test leci jako
   `cargo test --test it <moduł>::` zamiast całego `--tests`; vitest bierze wskazane pliki.
   Powyżej `scope_limit` dotkniętych modułów leci pełna forma.

Jednolinijkowce (`cargo clippy`, `tsc`, `prettier`) nie mają własnych plików — stoją wprost
w `checks.json`. Pliki w `checks/` zostają tylko tam, gdzie sprawdzenie jest **własne**:
niezmienniki 1–3 (`boundary`), tabela żargonu D5 (`vocabulary`), tokeny D1 (`tokens`), klucze
`invoke()` kontra `ipc.rs` (`invoke-args`), tłumione bramki (`suppressions`), martwe szwy
(`wired`), deklaracje modułów testowych (`tests-listed`).

`density` i `worktree-trust` są w `manual_only` — odpalasz je sam (`scripts/h check density`),
a `.loadout/h/guards.sh` wypisuje je z nazwy jako pominięte, żeby cichy skip był niemożliwy.

## `target/` NIE jest dzielony między worktree

To decyzja o **poprawności**, nie o wydajności, i jest odwrotna niż w `../meetnotes`.
Odtworzone tam przy zerowej równoległości: dwa checkouty o tej samej nazwie pakietu, wersji
i układzie względnym, budowane przez jeden `CARGO_TARGET_DIR`, dają jeden odcisk metadanych.
Sekwencja `build A; build B; build A` melduje A jako `Fresh`, choć rlib na dysku zbudowano ze
źródeł B — czyli check potrafi osądzić **cudzy** kod i zaświecić zielono. Do tego zmierzone
tutaj 2026-08-17: 24 worktree na jeden `target/` = 66 GB i 886 645 plików.

Wydajność bierzemy więc z drugiego lewara, tego bezpiecznego: zawężania checków.

## Granice

Ten harness ma ~590 linii w `h.py` plus 90 linii `checks.json` i trzy prompty po ~30 linii.
Poprzedni miał **9323 linie w czternastu plikach** i to jest dokładny powód, dla którego go
nie ma. Zanim cokolwiek tu dopiszesz, sprawdź w `runs/`, czy to kiedykolwiek złapało realny błąd.

Czego tu celowo NIE ma, i co każde z tego kosztowało (zmierzone na 121 biegach):

| Usunięte | Dlaczego |
|---|---|
| poziomy `before`/`quick`/`task`/`full` | `full` to 319 s, z czego 280 s (88%) suita całego repo, wołana DWA razy na bieg |
| `tasks/*.md` i blok `OWNS` | 26 617 linii kontraktów pisanych ręcznie **przed** biegiem |
| odwracanie kryteriów w `before` + `NOT_A_REAL_RED` | ~30 podpisów fałszywej czerwieni; zastąpione jednym pytaniem do weryfikatora |
| obowiązkowa recenzja ze schematem findingów | 97 uwag na 105 recenzji → runda naprawcza w 81% biegów, dłuższa niż implementacja |
| paragony, zamrożenie kontraktu, odcisk asercji | obsługiwały kontrakt, którego nie ma |
| muteks cargo i zamek poziomu `full` | powstały, bo bramka odkrywała dwa clippy naraz; teraz jest jedno |
| `prompt_backticks`, `prompt_dollars` | prompty są plikami `.md`, więc bash ich nie interpoluje — klasa niemożliwa, nie pilnowana |

Co zostało z całej starej maszynerii dowodowej: **licznik przejść** (niezmiennik 19, 15 linii
w `h.py`). Kod testowany biegnie w tym samym procesie, którego kod wyjścia czytasz, więc
`exit 0` bez ani jednego zameldowanego przejścia jest czerwony. To także jedyna rzecz, która
pilnuje, żeby zawężenie checka nie zazieleniło go przez filtr, który nic nie dopasował.
