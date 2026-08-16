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

## Q-5 — ROZSTRZYGNIĘTE: nikt nie montował sekcji

Decyzja Jakuba 2026-08-15: **nowe zadanie T-25**, wariant A (konwencja zamiast rejestru).

`src/App.tsx` szuka `src/sections/<id>/index.tsx`. Każde zadanie sekcji tworzy własny `index.tsx`
w poddrzewie, które już posiada — zero plików dzielonych. `src/ui/sections.tsx` zostaje bez zmian
i **nie** dostaje pola `component`: to by zrobiło z niego drugi wspólny kręgosłup obok `lib.rs`,
z tą samą klasą kolizji, a front — inaczej niż Rust — niczego takiego nie wymaga.

T-25 stoi w kolejce **przed T-08**, bo T-08 jest pierwszym zadaniem sekcji. Dowód end-to-end nie
został w T-25 (nie ma tam czego montować, a atrapa zostałaby w repo na zawsze — niezmiennik 17):
poszedł do T-08 jako AC-8, wraz z `src/sections/run/index.tsx` w jego OWNS. Przekazanie ma
mechanizm, nie tylko zdanie — to była cała wada, którą Q-5 opisywało.

---

## Czego świadomie NIE mechanizujemy

**„Jedna komenda na wywołanie Bash".** Kusi, żeby zrobić z tego hak `PreToolUse`, ale hak
odmawiający też kosztuje turę — dokładnie tę samą, którą kosztuje odmowa uprawnień. Zysku
zero, a dochodzi ryzyko fałszywej odmowy na poprawnym łańcuchu. Zostaje promptem, bo to
zachowanie, a nie stan, który da się wykryć i naprawić.

**Asercja, że hak formatujący jest podpięty.** `.claude/**` jest dla biegu zabronione do
zapisu — jedynym, kto może go odpiąć, jest orchestrator. Sprawdzenie pilnowałoby wyłącznie
mnie, a `checks/MANIFEST` kazałby dopisać wpis. Ceremonia większa niż ryzyko.

**Sufit czekania na muteks cargo (300 s) — decyzja ODWRÓCONA 2026-08-16.**

Pierwotnie: „jeśli żywe cargo trzyma zamek pięć minut, to nie jest sytuacja do przeczekania,
tylko sygnał, że fala jest za szeroka; podniesienie limitu ukryłoby ten sygnał".

To rozumowanie było poprawne **dla biegu szeregowego** i przestaje obowiązywać przy wachlarzu.
Przy sześciu zadaniach naraz kolejkowanie na muteksie jest **projektowanym zachowaniem**
(niezmiennik 26 przepuszcza jeden ciężki cargo), a nie objawem czegokolwiek. Sufit 300 s
zamieniłby wtedy normalną kolejkę w `exit 2` i fałszywą czerwień u ostatniego w kolejce.

Nie zmieniamy domyślnej wartości — zmieniamy ją **tam, gdzie zmienia się założenie**:
wachlarz eksportuje `LOADOUT_CARGO_LOCK_WAIT=2400`, bieg szeregowy zostaje przy 300 s.
Jedno założenie, jedno miejsce, obie wartości uzasadnione.
