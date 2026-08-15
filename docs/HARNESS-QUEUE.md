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
