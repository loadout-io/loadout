# T-82 — Rekonstrukcja workflow przestaje być fiksturą

Import nie „nie umie jeszcze" odtworzyć workflow. Import umie odtworzyć **dokładnie jeden**,
i rozpoznaje go po trzech napisach:

```rust
pub(crate) fn knows_ship_ui(content: &str) -> bool {
    ["frontend-dev", "design-qa", "code-reviewer"].iter().all(|role| content.contains(role))
        && check_command(content).is_some()
}
```

`ship_ui()` dodatkowo wymaga zapisanych agentów o **dokładnie** tych nazwach. Repozytorium,
w którym role nazywają się inaczej, dostaje zero workflow — i to jest cała przyczyna, dla której
prawdziwy import nie utworzył żadnego. To jest niezmiennik 20 odwrócony: nie test stoi na obecności
napisu, tylko kod produkcyjny.

**Trzy rodzaje kroku i ani jednego więcej** (D6). Recenzja jest zwykłym krokiem z agentem,
zatwierdzenie człowieka jest punktem kontrolnym, a komenda z własnym wynikiem jest krokiem
„sprawdź" — **wyłącznie** wtedy, gdy stoi dosłownie w wskazanym pliku setupu i ma dowód licznika
przejść (niezmiennik 19). Vendorowe funkcje są konfiguracją agenta albo kroku, nigdy nowym
rodzajem kafelka.

**Nierozstrzygnięte zostaje nierozstrzygnięte.** Warunek albo gałąź, której nie da się wyrazić
w dzisiejszym modelu, zostaje jako pozycja wymagająca uwagi i jest nazwana. Pominięcie jej
i nazwanie wyniku zgodnym jest tą samą cichą zielenią, przed którą stoi całe to repo.

**Bez kryterium na scheduler.** „Silnik nie zna etapów" pilnuje już gerp granicy z niezmiennika 27,
uruchamiany w bramce razem z niezmiennikiem 1. Drugi egzekutor tej samej reguły to drugie źródło
prawdy, a nie dodatkowy dowód.

**Read first:** `src-tauri/src/import/translate.rs` (`imported_workflows`, `ship_ui`, `flatten`) ·
`src-tauri/src/import/adapters.rs` (`knows_ship_ui`, `check_command`) ·
`docs/DECISIONS-LOCKED.md` D6 (trzeci rodzaj „sprawdź" i czego ta zmiana NIE otwiera) ·
`AGENTS.md` niezmienniki 11, 19, 20, 27.

## Kto to robi

- **Agent:** `rust-core`
- **Druga opinia:** inny vendor niż pisarz (D3).

## Zanim napiszesz pierwszą specyfikację — dwie rzeczy o celu `it`

1. **Adresowanie.** Kryterium woła `cargo test --test it <modul>::`, nigdy
   `cargo test --test <modul>`. Cel `it` jest jeden i zbiorczy.
2. **Wpis `mod`.** Nowy plik wymaga linii `mod <nazwa>;` w `src-tauri/tests/it/main.rs`. Bez niej
   plik kompiluje się do niczego i nie uruchamia ani jednego testu — czyli **wygląda jak zestaw,
   który przeszedł**, a bramka melduje „exit 0 but no evidence of execution" i czyta to jako defekt
   KONTRAKTU, nie implementacji. Dlatego `main.rs` jest w OWNS tego zadania.

   Zmierzone 2026-08-22 na pierwszym biegu T-79: faza kontraktu napisała dwie specyfikacje po
   kilkanaście kilobajtów i nie dopisała ani jednego `mod`. Runda naprawcza powtórzyła ten sam
   błąd, bo widziała ten sam komunikat i nie wiedziała, co on znaczy. Bieg skończył się niczym.

## AC-1 Rekonstrukcja nie zna nazw ról z jednego repozytorium
check: cargo test --test it import_workflow_is_not_a_fixture::
expect: (\d+) passed

Repozytorium, w którym role nazywają się inaczej niż w fiksturze, dostaje odtworzony workflow:
każdy krok wskazuje istniejącego natywnego agenta, a kolejność i równoległość pochodzą ze źródła.
Kryterium jest odporne na powrót fikstury — sadzi repozytorium z zupełnie innymi nazwami ról
i wymaga workflow, a drugim przebiegiem wymaga, żeby repozytorium bez rozpoznawalnej sekwencji
dostało **nazwane** pozycje wymagające uwagi zamiast pustej listy.

## AC-2 Krok „sprawdź" istnieje tylko z dowodem ze źródła
check: cargo test --test it import_check_step_needs_its_evidence::
expect: (\d+) passed

Komenda, która stoi dosłownie we wskazanym pliku setupu i ma wzorzec licznika przejść, daje krok
„sprawdź" z zapisanym plikiem-świadkiem. Komenda wymyślona, sparafrazowana albo bez licznika
**nie daje kroku** — zostaje zachowaniem wymagającym uwagi, nazwanym po komendzie. Zatwierdzenie
człowieka jest punktem kontrolnym; rodzaj `review` jest odmową (wyrocznia z T-23).

## AC-3 Odtworzony workflow naprawdę biegnie w zadanej kolejności
check: cargo test --test it imported_workflow_runs_in_order::
expect: (\d+) passed

Zaimportowany graf — planista, wykonawca, dwóch recenzentów równolegle, składacz — biegnie na
atrapach sterownika: przekazanie planisty dochodzi do wykonawcy, wykonawca ma przypisany skill,
a okna czasowe obu recenzentów **nakładają się** (niezmiennik 11), co kryterium mierzy znacznikami
z zegara biegu, nie kolejnością wywołań.

<!-- OWNS
tasks/T-82.md
src-tauri/src/import/translate.rs
src-tauri/src/import/adapters.rs
src-tauri/src/import/mod.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/import_workflow_is_not_a_fixture.rs
src-tauri/tests/it/import_check_step_needs_its_evidence.rs
src-tauri/tests/it/imported_workflow_runs_in_order.rs
src-tauri/tests/it/import_workflow_is_runnable_only_when_complete.rs
src-tauri/tests/it/imported_subworkflow_is_flattened.rs
src/sections/import/setup.tsx
-->
