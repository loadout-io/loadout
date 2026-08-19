# Pętla między krokami, z limitem tur

Data: 2026-08-19. Rozstrzygnięcia właściciela zapisane w tekście przy każdej decyzji.

## Po co

Workflow umie dziś wyłącznie iść do przodu. Kształt, którego brakuje, właściciel opisał tak:
implementer wysyła do testera, tester zdaje raport — jak **fail**, implementer poprawia i tester
sprawdza jeszcze raz; jak **pass**, bieg idzie dalej. Bez tego jedyną formą rundy poprawek jest
wypisanie każdej rundy jako osobnego kroku, co oznacza, że liczba prób jest zamrożona w pliku
i widać ją na płótnie jako łańcuch identycznych kafelków.

## Czego to NIE zmienia

Silnik biegu. `plan_run` rozwija pętlę **przy planowaniu**, więc `Dag` (`commands/run.rs:301`)
dalej dostaje graf bez cykli. Planista, pula miejsc (niezmiennik 11), świeże kopie plików,
indeks przekazań i zabijanie grupy zostają nietknięte. To był jedyny powód, dla którego wybrano
rozwinięcie zamiast prawdziwego cyklu w planiście: ryzyko siedzi w tej warstwie, a ta zmiana
w nią nie wchodzi.

## Decyzje właściciela

1. **Werdykt pisze tester, w swoim przekazaniu.** Front-matter dostaje `outcome: pass | fail`.
   Odrzucone: osobny rodzaj kroku uruchamiający komendę powłoki (deterministyczny, ale działa
   tylko tam, gdzie da się sprowadzić sprawdzenie do jednej komendy) i punkt kontrolny w każdej
   rundzie (kasuje bezobsługowość dokładnie wtedy, kiedy jest najbardziej potrzebna).
2. **Wyczerpany limit kończy bieg odmową.** Kroki za pętlą NIE ruszają. Odrzucone: przepuszczenie
   dalej z ostatnim raportem — kasowałoby jedyny powód, dla którego pętla istnieje.
3. **Rozwinięcia nie widać w oknie.** Na płótnie dwa kafelki i strzałka wstecz; w szynie jedna
   karta agenta na wszystkie rundy.

## 1. Plik i schemat

Strzałka dostaje pole opcjonalne:

```json
{ "from": "s_tester", "to": "s_implement", "max_turns": 3 }
```

Brak pola znaczy „zwykła strzałka", więc **każdy istniejący plik znaczy dokładnie to, co znaczył**.
Pętlę definiuje strzałka, nie nowy rodzaj kroku — dlatego na płótnie nie przybywa ani jeden
kafelek, a `Step` nie zmienia się wcale.

Zakres `max_turns`: **1–10**, tą samą drogą i z tym samym rodzajem uwagi, co `copies` (1–8).

## 2. Walidator (`workflow/check.rs`)

- **`a_circle` odmawia dalej każdemu cyklowi, którego nie zamyka oznaczona strzałka.** Reguła
  w jednym zdaniu: *po usunięciu strzałek z `max_turns` graf musi być bez cykli*. Nieoznaczony
  cykl to pomyłka i ma nią zostać.
- `max_turns` poza zakresem 1–10 to Problem.
- Krok, z którego **wychodzi** strzałka wsteczna, jest sędzią pętli i musi mieć agenta —
  przy Run, tą samą wagą, co `a_step_without_an_agent`.
- `one_folder_two_steps` bez zmian: rozwinięte podejścia biegną po kolei, więc nie tworzą nowej
  kolizji folderów.

## 3. Werdykt (`memory/handoff.rs`)

Front-matter przekazania dostaje `outcome`. Trzy stany, nie dwa:

| w pliku | znaczenie |
|---|---|
| `outcome: pass` | pętla się domyka, bieg idzie dalej |
| `outcome: fail` | kolejna runda, jeśli limit na to pozwala |
| brak linii | **traktowane jak `fail`** |

Trzeci wiersz jest treścią, nie ostrożnością: zapomniana linia nie ma prawa wyglądać jak sukces,
bo wtedy najtańszą drogą przez bramkę jest nie napisać werdyktu wcale.

Silnik dokleja sędziemu do promptu jedno zdanie o tym, że ma ten werdykt napisać — dokładnie tam,
gdzie dokleja zadanie całego biegu (`with_the_task`). Dzięki temu generyczny workflow nie musi
o tym wiedzieć, a człowiek nie musi tego wpisywać w `What to do`.

## 4. Planista (`commands/run.rs`)

`plan_run` rozwija pętlę na literalne węzły:

```
Implement · Tester · Implement · Tester · Implement · Tester · (kroki za pętlą)
```

- **Pass w rundzie k** → węzły rund od k+1 w górę są pomijane, bieg wchodzi w kroki za pętlą.
- **Fail w ostatniej rundzie** → bieg kończy się odmową nazywającą sędziego i liczbę rund
  („Tester never passed after 3 rounds."), a kroki za pętlą nie ruszają.
- `RunReport::steps` przechodzi z „jeden wpis na krok pliku" na **„jeden wpis na podejście"**.
  Zmiana jest wewnętrzna: `ipc::run_workflow` oddaje oknu `()`, a okno czyta wynik z katalogu
  biegu (niezmiennik 4).

Nazwa kroku w liniach zostaje **nazwą z pliku** (`Tester`), nigdy `Tester#2` — to jest warunek
z punktu 5 zapisany po stronie silnika.

## 5. Okno

- **Płótno:** strzałka wsteczna rysowana inaczej (przerywana, podpis `up to 3 tries`).
  `isValidConnection` przestaje odmawiać krawędzi domykającej koło — zamiast tego tworzy ją
  jako oznaczoną, z domyślną liczbą tur.
- **Panel kroku:** pole `Try again up to` na kroku-sędzim. **Tej kontrolki nie ma w makiecie** —
  zapisane wprost, bo makieta jest jedyną wyrocznią wyglądu, a ta funkcja w niej nie istnieje.
- **Szyna i strumień:** zero zmian. Linie niosą nazwę agenta (`engine/line.rs:547`), a szyna
  grupuje karty po `row.agent` (`rail/roster.ts:98`), więc rundy zlewają się w jedną kartę same
  z siebie.

## 6. Kryteria

Każde z mutacją, która musi je zapalić.

1. **Rozwinięcie.** Plik z oznaczoną strzałką i `max_turns: 3` planuje trzy rundy sędziego.
   *Mutacja:* rozwinięcie na jedną rundę.
2. **Nieoznaczony cykl dalej odrzucony.** Ten sam graf bez `max_turns` to Problem.
   *Mutacja:* zdjęcie warunku „oznaczona" z `a_circle`.
3. **Wyjście po `pass`.** Sędzia zdaje w rundzie 2 → runda 3 NIE biegnie, a krok za pętlą biegnie.
   *Słaba wersja:* sprawdzenie samego „krok za pętlą biegnie" — przechodzi dla implementacji,
   która i tak przepala wszystkie rundy.
4. **Wyczerpany limit.** Trzy razy `fail` → bieg odmawia, kroki za pętlą nie ruszyły.
5. **Brak werdyktu = fail.** Przekazanie bez linii `outcome` zachowuje się jak `fail`.
   *Mutacja:* brak linii czytany jako `pass`.
6. **Rozwinięcia nie widać.** Po trzech rundach szyna ma **jedną** kartę sędziego, a nie trzy.

## 7. Ryzyko zapisane wprost

W chwili pisania **15 testów silnika biegu jest czerwonych** na limicie czasu
(`runcmd_*`, `limits_*`, `product_path_end_to_end`). Sprawdzone: padają identycznie bez zmian
z tej gałęzi, a notatka `busy-machine-fakes-a-red-full-test` mówi, że cztery z nich mierzą czas
na prawdziwym zegarze. Pracujemy w `commands/run.rs`, więc **pierwszym krokiem wdrożenia jest
ustalenie stanu wyjściowego tych testów na spokojnej maszynie** — inaczej nie da się odróżnić
własnej regresji od zastanego szumu.
