# Kolejka poprawek harnessu

Zmiany gotowe do naniesienia, których **nie wolno nanieść w trakcie biegu**.

**Od `4f9a558` ta kolejka jest w dużej mierze zbędna.** Q-1 sprawiło, że `ship-task.sh`
i `scripts/build-loop.sh` odpalają się z przypiętej kopii (`exec bash "$snap"`), więc edycja
w trakcie biegu przestała psuć biegnący proces. Kolejka zostaje dla tego, co nadal jest
niebezpieczne w trakcie biegu, i dla zapisu, czego świadomie **nie** mechanizujemy.

Co jeszcze jest wrażliwe na moment:

| Co | Dlaczego nie w trakcie |
|---|---|
| `checks/*.sh` | zmieniają bramkę pod zadaniem, które jest przez nią właśnie sądzone |
| `harness/gate.py` | to samo, plus `integrate.sh` sądzi trunk przed merge'em |
| cokolwiek, gdy biegną strażnicy | `harness/guards.sh` przerywa się kodem 2 na brudnym drzewie, meldując „restore failed" — czyli twoja edycja wygląda jak wada strażnika |
| `tasks/*.md` | plik zadania jest bajt w bajt porównywany z `TASK.md` gałęzi (N-08) |

Bezpieczne w każdym momencie: `docs/**`, `AGENTS.md`, `.claude/**` (proces agenta wczytał
ustawienia przy starcie), nowe pliki, których nikt jeszcze nie woła.

---

## Puste

Q-1 … Q-4 naniesione w `4f9a558`. Poprawka muteksu cargo w `689e432`.

---

## Q-5 — nikt nie montuje sekcji w aplikacji (DECYZJA CZŁOWIEKA)

**Klasa:** nie zatrzyma pętli, i właśnie dlatego jest groźna. Wszystko zaświeci na zielono,
a okno pokaże pięć pustych ekranów.

`src/App.tsx` (T-01, wylądowany) renderuje `<EmptyState>` dla każdej sekcji. `src/ui/sections.tsx`
wylicza pięć sekcji z etykietą i zdaniem pustego ekranu — i **nie ma pola na komponent**.
Nagłówek tego pliku mówi:

> „Ten plik jest znanym przekazaniem własności: T-08, T-09, T-11, T-13, T-14, T-17 i T-19
> dopisują tu po jednej linii, mimo że go nie posiadają."

Przekazanie bez mechanizmu. Żadne z tych siedmiu zadań nie ma `src/ui` ani `App.tsx` w OWNS
(ma je wyłącznie T-01), a `checks/quick-scope.sh` odrzuci zapis poza blokiem. Nie ma też
„jednej linii" do dopisania, bo rejestr nie zna pojęcia komponentu. Kryteria tych zadań to
testy komponentowe wołane wprost na plikach, więc **przechodzą bez montażu** — bramka nigdy
o tym nie powie.

Dwa wyjścia, i to jest wybór projektowy, nie porządkowy:

**A. Konwencja zamiast rejestru.** `App.tsx` szuka `src/sections/<id>/index.tsx`. Każde zadanie
sekcji tworzy własny `index.tsx` **wewnątrz swojego poddrzewa**, które już posiada — zero plików
dzielonych, zero wpisów do OWNS. Koszt: jednorazowa zmiana `App.tsx` i jego testu, których nikt
dziś nie posiada.

**B. Rejestr dostaje pole `component`.** Siedem zadań dopisuje po jednym wierszu do
`src/ui/sections.tsx`, tak jak dopisują `pub mod x;` do `lib.rs`. Koszt: `src/ui/sections.tsx`
ląduje w OWNS siedmiu zadań i staje się drugim wspólnym kręgosłupem, z tą samą klasą konfliktów.

Rekomendacja: **A.** Kręgosłup rustowy jest wspólny, bo Rust tego wymaga; front nie wymaga,
więc dokładanie sobie drugiego wspólnego pliku jest kosztem bez powodu. A to znosi całą klasę
zamiast ją powielać.

Czego brakuje do decyzji: kto wykonuje zmianę `App.tsx`. Nie orchestrator — to kod produktu,
nie harness (`.claude/commands/build.md` §5a). Naturalne miejsca: nowe zadanie **T-25**, albo
doklejenie do T-08 (pierwsze zadanie sekcji w kolejności).

---

## Czego świadomie NIE mechanizujemy

**„Jedna komenda na wywołanie Bash".** Kusi, żeby zrobić z tego hak `PreToolUse`, ale hak
odmawiający też kosztuje turę — dokładnie tę samą, którą kosztuje odmowa uprawnień. Zysku
zero, a dochodzi ryzyko fałszywej odmowy na poprawnym łańcuchu. Zostaje promptem, bo to
zachowanie, a nie stan, który da się wykryć i naprawić.

**Asercja, że hak formatujący jest podpięty.** `.claude/**` jest dla biegu zabronione do
zapisu — jedynym, kto może go odpiąć, jest orchestrator. Sprawdzenie pilnowałoby wyłącznie
mnie, a `checks/MANIFEST` kazałby dopisać wpis. Ceremonia większa niż ryzyko.

**Sufit czekania na muteks cargo (300 s).** Kusi, żeby go podnieść pod zadania równoległe,
ale po `689e432` zamek po martwym właścicielu jest odzyskiwany natychmiast, więc 300 s dotyczy
już wyłącznie **żywego** cargo. Jeśli żywe cargo trzyma zamek pięć minut, to nie jest sytuacja
do przeczekania — to sygnał, że fala jest za szeroka. Podniesienie limitu ukryłoby ten sygnał.
