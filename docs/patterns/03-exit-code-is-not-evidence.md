# 03 — Kod wyjścia to nie dowód

**Zielone bez licznika przejść jest czerwone.**

Kod, który testujesz, biegnie w tym samym procesie, którego kod wyjścia czytasz.
`os._exit(0)` na poziomie modułu zazielenia całą suitę. Import, który wywala się cicho,
też potrafi dać zero.

## Reguła dowodu

`harness/gate.py` wymaga, żeby wyjście zawierało licznik wykonanych testów:

```python
DEFAULT_EXPECT = r"(?:Ran\s+(\d+)\s+tests?|(\d+)\s+(?:passed|tests?\s+passed))"
```

Exit 0 bez trafienia → **czerwone**, z powodem `exit 0 but no evidence of execution`.

Zawężaj do linii samego runnera. W repo źródłowym `Test Files 1 passed (1)` stojące nad
`Tests 4 skipped (4)` raz zaraportowało przechodzący test dla biegu, w którym nic nie wykonano.

## Czerwone, które nie jest czerwone

W warstwie `before` sprawdzenie musi paść **z właściwego powodu**. `NOT_A_REAL_RED` wylicza
~24 podpisy porażki bez uruchomienia:

```
module not found · command not found · cannot find module · ENOENT
connection refused · browser could not launch
Tests N skipped (N) · No test files found
error[E0432] unresolved import · no targets specified
rc 124 (timeout) · rc 127 (brak komendy)
```

Bez tego `verify.sh before` przechodzi na pustym repo — i cała dyscyplina „udowodnij czerwone
przed napisaniem kodu" staje się teatrem.

Sprawdzenia projektowe (`checks/*.sh`) **nie są** odwracane w warstwie `before`. Tylko kryteria akceptacji.

## To samo dotyczy testów harnessu

Test, który sprawdza, że plik **zawiera string**, nie sprawdza zachowania.
`selftest.py` w repo źródłowym asertował `"--sandbox workspace-write" in ship-task.sh`,
przechodził **na komentarzu**, a żywa flaga brzmiała `danger-full-access`.

Zasadź prawdziwe naruszenie, wymagaj czerwonego, przywróć, wymagaj zielonego.
To robi `harness/guards.sh` — i odmawia startu na brudnym drzewie, bo pominięty strażnik
wygląda dokładnie tak samo jak przechodzący.
