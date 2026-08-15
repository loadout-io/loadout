# T-22 — Sprawdzacze w bramce: granice modułów, gęstość, testy, clippy

Sprawdzacz, którego nikt nigdy nie widział na czerwono, to sprawdzacz nieprzetestowany. To zadanie
dotyka czterech plików w `checks/` i dokłada piąty, a jego cicha awaria polega na tym, że wszystko
świeci na zielono z powodów, które nie mają nic wspólnego z jakością kodu. Trzy konkretne
przykłady, wszystkie obecne w drzewie **dziś**: `checks/quick-boundary.sh` pilnuje niezmiennika 3
przez grep po `#[cfg(windows)]`, więc `use libc::SIGTERM;` w `recovery.rs` przechodzi bez słowa —
a `libc` jest zależnością wyłącznie uniksową i to jest dokładnie ten kod platformowy, który
zamienia port na Windows w przepisanie. `checks/full-test.sh` uruchamia `cargo test --lib`, a
**każde** kryterium akceptacji w tym repo ma postać `cargo test --test <cel>`; pełna bramka nigdy
nie odpala celów, na których stoi cała wyrocznia projektu. I sufit gęstości z ARCHITECTURE §7,
który dopóki nikt go nie mierzy, jest zdaniem w dokumencie — poprzedni prototyp ustawił swój po fakcie,
wymuszany próg zamarzł na **29 regionach przy limicie 12**, czyli 2,4× wartości docelowej
`[03 §4.1]`. Czwarta cicha awaria jest najbardziej podstępna: metryka, której kolektor nie
zmierzył, zapisana jako `0` i porównana z sufitem 8. Zielono. Zawsze.

**Read first:**
`docs/ARCHITECTURE.md` §7 (**jedyne** źródło siedmiu liczb sufitu — skrypt je stąd parsuje, nie
kopiuje) i §3 (trzy granice, których pilnuje `quick-boundary.sh`).
`docs/research/projects/03-poprzedni prototyp-desktop-ux.md` §4.1 i §5.7 (rozstrzygają, czym jest zapadka:
mierzone w prawdziwym Chromium, dwie szerokości, wymuszany próg może tylko maleć — oraz dlaczego
zapadka ustawiona po fakcie zamarza tam, gdzie akurat jesteś).
`docs/research/projects/00-SYNTHESIS.md` §4.1 (reguła dowodu i lista `NOT_A_REAL_RED` — one
decydują, jak wolno napisać te testy) i §6 „Harness self-deception".
`docs/design/DESIGN.md` §9 (rozstrzyga najwęższą wspieraną szerokość okna: **1100 px**).
`AGENTS.md` §3 (niezmienniki 18, 19, 20, 21, 23, 26) i §7 (dlaczego to zadanie w ogóle wolno
dotknąć `checks/`: tylko dlatego, że te ścieżki stoją w jego bloku OWNS).
`harness/gate.py` (czytaj `discover`, `contract_problems`, `NOT_A_REAL_RED` — to jest kontrakt,
któremu te sprawdzenia muszą się poddać; nie zmieniasz go, `harness/` nie należy do tego zadania).

## Kto to robi

- **Agent:** `harness` — pisarz: Codex
- **Druga opinia:** Claude Code (nigdy ten sam vendor co pisarz, decyzja D3)
- **Artefakty biegu:** `runs/T-22/` — transkrypt, plik wyników, plan poprawki. Nigdy `$TMPDIR`.

## Co to zadanie posiada

- `checks/quick-boundary.sh` — niezmienniki 1 i 3 (i przy okazji 2). Ograniczenie: to jest grep po
  czystym drzewie, więc **na zawsze pozostaje sprawdzeniem projektowym, nigdy kryterium
  akceptacji.** Nie umie zaczerwienić się w tierze `before`.
- `checks/quick-density.sh` — nowy. Tabela wyjść: `0` zmierzone i pod zapadką, `0` nie ma czego
  mierzyć (z nazwanym warunkiem), `1` za gęsto albo powyżej zapadki, `2` **nie dało się** zmierzyć.
- `checks/full-test.sh` — obie suity, z licznikiem przejść.
- `checks/full-clippy.sh` — pełna forma clippy, raz, w bramce.
- `scripts/density-audit.mjs` — kolektor (w przeglądarce) i sędzia (w node) rozdzielone; sędzia
  jest czystą funkcją nad zrzutem JSON, więc daje się przetestować bez przeglądarki.
- `checks/density-baseline.json` — zapadka. Może **tylko maleć**. Leży w `checks/`, poza zasięgiem
  biegu, dokładnie jak `checks/vocabulary-baseline.json`.
- `checks/tests/` — wyrocznia tego zadania: siedem plików `*.test.ts` plus fikstury zrzutów.
  Żadne inne zadanie nie deklaruje tej ścieżki.

## Niezmienniki

- **18 — sufit gęstości jest mierzony, nie oceniany okiem; baseline może tylko maleć.** Łamie się
  cicho przez zapadkę, która przy każdym pomiarze zapisuje **aktualną** wartość: skrypt biegnie,
  plik się zmienia, nic nigdy nie jest czerwone. Drugi cichy wariant: sufit przepisany do
  `.mjs` obok tego w ARCHITECTURE §7 — dwie kopie, i po pierwszej edycji dokumentu bramka pilnuje
  liczby, której już nikt nie deklaruje.
- **19 — kod wyjścia to nie dowód.** Dotyczy zarówno testowanych skryptów, jak i testów tego
  zadania. `bash checks/quick-density.sh; echo $?` z zerem nic nie znaczy, dopóki skrypt nie
  wypisał, **co** zmierzył i **ile** tego było.
- **20 — test sprawdza zachowanie, nie obecność stringa.** To jest oś całego zadania. Selftest
  w spreadsheet asertował `"--sandbox workspace-write" in ship-task.sh`, przechodził **na
  komentarzu**, a żywa flaga brzmiała `danger-full-access`. Każde kryterium niżej zasadza
  prawdziwe naruszenie w drzewie tymczasowym, wymaga czerwonego, i w tym samym pliku wymaga
  zielonego na drzewie czystym.
- **21 — nie pisz artefaktu, którego żaden skrypt nie czyta.** `checks/density-baseline.json`
  ma być czytany przez `quick-density.sh` przy **każdym** biegu, a nie tylko zapisywany przez
  `--update-baseline`. Plik zapisywany i nieczytany to `design/<task>/plan.json` z repo
  źródłowego.
- **23 — polityka mieszka w jednym rdzeniu.** Polityka lintów to `[workspace.lints]` w głównym
  `Cargo.toml`. `full-clippy.sh` **nie ma prawa** dopisywać `-D clippy::unwrap_used` do wywołania
  — to jest przepisanie polityki w adapterze i dokładnie tak umarło po cichu skanowanie sekretów
  na PR #535 `[05 §4]`. Sprawdzenie ma weryfikować, że polityka jest **podłączona**, nie
  powtarzać jej treści.
- **26 — nie uruchamiaj dwóch ciężkich `cargo` naraz na tym Macu.** Testy tego zadania odpalają
  `cargo` w drzewie tymczasowym, **z wnętrza** `vitest`, który sam bywa odpalany przez
  `checks/full-test.sh`. Bez odizolowania zamka jest to zakleszczenie na 300 sekund, które czyta
  się jak losowy timeout. Patrz „Trzy rzeczy" pod nagłówkiem kryteriów.

## Kryteria akceptacji

**Trzy rzeczy, bez których te testy nie zadziałają.**

1. **Kopiuj sprawdzenie do piaskownicy zamiast łatać je o zmienne środowiskowe.** Każdy skrypt
   w `checks/` wylicza `ROOT` z `BASH_SOURCE`. `cp checks/quick-boundary.sh $scratch/checks/`
   sprawia, że `ROOT == $scratch` — sprawdzenie działa na drzewie testu bez ani jednej zmiany
   w kodzie produkcyjnym i bez dotykania prawdziwego repo. Dla sprawdzeń wołających `cargo`
   dokopiuj też `checks/_cargo-serialize.sh`.
2. **Ustaw `TMPDIR` na piaskownicę przy każdym wywołaniu, które woła `cargo`.**
   `_cargo-serialize.sh` bierze zamek w `${TMPDIR:-/tmp}/loadout-cargo.lock`; bez podmiany
   kopia w piaskownicy sięga po ten sam zamek, który trzyma zewnętrzny `full-test.sh`.
3. **Najpierw sprawdź, że artefakt istnieje, z własnym komunikatem — dopiero potem cokolwiek
   uruchamiaj.** `ENOENT`, `No such file or directory` i `command not found` są na liście
   `NOT_A_REAL_RED`, więc brakujący plik daje w tierze `before` fałszywą czerwień, którą bramka
   odrzuci. To też jest jedyny sposób, żeby zmieścić się w suficie 20 s na sprawdzenie w `before`.

**Dlaczego żadne kryterium nie ma `expect: none`.** Sprawdzenia z `checks/` same w sobie nie
mogłyby być kryteriami — grep po czystym drzewie przechodzi, zanim kod powstanie. Dlatego każde
kryterium niżej jest **testem zachowania sprawdzacza** uruchamianym przez `vitest`, a `vitest`
drukuje `Tests N passed (N)`, więc reguła dowodu jest spełniona normalną drogą. `expect: none`
byłoby tu rezygnacją bez powodu.

## AC-1 `quick-boundary.sh` widzi kod platformowy, który nie jest napisany `#[cfg(windows)]`
check: npx --no-install vitest run checks/tests/boundary-blind-spots.test.ts

Piaskownica z drzewem `src-tauri/src/{engine/{dag.rs,supervisor.rs,drivers/fake.rs},recovery.rs,
store/writer.rs}`. Musi dać **exit 1** z komunikatem niosącym ścieżkę pliku i numer niezmiennika:
`use libc::SIGTERM;` w `engine/dag.rs`; `use std::os::unix::process::CommandExt;` w
`store/writer.rs`; `if cfg!(unix) {}` w `recovery.rs`; `use tauri::AppHandle;` w
`engine/drivers/fake.rs` (dziś wyłączony ze wszystkich reguł, a jest kodem kompilowanym do
binarki, nie testem); `use crate::ipc::Line;` w `engine/dag.rs` (zależność od Tauri bez słowa
„tauri"). Musi dać **exit 0**: te same trzy tokeny platformowe wewnątrz `engine/supervisor.rs`;
`libc` w komentarzu `// libc::kill nie ma prawa tu wejść`; te same importy w
`engine/tests/helpers.rs`. Czyste drzewo: exit 0 i wypisana **niezerowa** liczba obejrzanych
plików.

*Słaba asercja:* `expect(run(planted).code).not.toBe(0)` dla każdego zasadzenia. Przechodzi skrypt
przepisany na `exit 1`. Asercja rozstrzygająca: trzy przypadki ciszy i drzewo czyste muszą w tym
samym pliku dać exit 0, a komunikat naruszenia musi zawierać **ścieżkę** naruszającego pliku —
odmowa, która nie mówi gdzie, jest nie do naprawienia i uczy ludzi ignorować sprawdzacz.

## AC-2 `full-test.sh` uruchamia cele testów integracyjnych i nie zostawia zamka
check: npx --no-install vitest run checks/tests/full-test-targets.test.ts

Piaskownica z minimalnym crate'em **bez zależności** (żeby `cargo` szedł sekundy, nie minuty):
`src/lib.rs` z jednym przechodzącym `#[test]`, `tests/thing.rs` z jednym `#[test]`.
Przypadek A: `tests/thing.rs` pada → **exit 1** (dziś `cargo test --lib` daje 0, bo nigdy nie
dotyka `tests/`). Przypadek B: oba przechodzą → exit 0, a wypisany licznik brzmi **2**, nie 1.
Przypadek C: `tests/thing.rs` istnieje, ale ma zero `#[test]` → exit 1 z komunikatem o zerze
przejść (niezmiennik 19). Po każdym wywołaniu `$TMPDIR/loadout-cargo.lock` **nie istnieje** —
`full-test.sh` musi oddać zamek przez `cargo_release` przed startem `vitest`, bo dziś trzyma go
przez całą suitę frontendu i `full-clippy.sh` stoi za nim bez powodu.

*Słaba asercja:* `expect(code).toBe(1)` w przypadku A. Przechodzi implementacja, która woła
`cargo test` i tylko przepisuje jego kod wyjścia, wciąż licząc przejścia z jednego celu. Asercja
rozstrzygająca: przypadek B musi zaraportować dokładnie **2** przejścia — jedno przejście dowodzi,
że policzono tylko `--lib`.

## AC-3 `full-clippy.sh` odmawia, kiedy polityka lintów nie jest podłączona
check: npx --no-install vitest run checks/tests/full-clippy-policy.test.ts

Piaskownica z workspace'em, którego `[workspace.lints.clippy]` denuje `unwrap_used`.
Przypadek A: członek ma `[lints] workspace = true` i `tests/x.rs` z `Some(1).unwrap()` →
**exit 1** (polityka gryzie, i gryzie w celu testowym, którego `--lib` nie widzi).
Przypadek B: ten sam kod, ale w `Cargo.toml` członka zamiast `lints.workspace = true` stoi
`# lints.workspace = true` w komentarzu → **exit 2** z komunikatem, że to nasza konfiguracja
jest zepsuta, a nie kod. Przypadek C: członek podłączony, `unwrap()` usunięty → exit 0.
Sprawdzenie **nie** dokłada żadnej flagi `-D clippy::…` do wywołania (niezmiennik 23) — jeśli
w B odpaliłoby clippy z własną listą lintów, dostałoby exit 1 i zamaskowało rozłączoną politykę.

*Słaba asercja:* `expect(stdout).toContain('workspace = true')` albo grep po tym ciągu w
`Cargo.toml`. Przechodzi **na komentarzu** — to jest dosłownie incydent
`--sandbox workspace-write` z raportu 06 §2. Asercja rozstrzygająca: przypadek B, w którym ciąg
jest w pliku obecny, a wymagany jest exit **2**.

## AC-4 Sufit gęstości ma jedną kopię i mieszka w `docs/ARCHITECTURE.md` §7
check: npx --no-install vitest run checks/tests/density-ceiling.test.ts

`readCeiling(path)` sparsowany z **prawdziwego** `docs/ARCHITECTURE.md` daje siedem wpisów o
wartościach `8, 96, 60, 1, 4, 2, 1` w kolejności wierszy tabeli §7. Ten sam parser puszczony po
kopii w piaskownicy, w której `| Oznaczone regiony na ekranie | **8** |` zmieniono na `**9**`,
zwraca `9` — dowód, że w `.mjs` nie ma drugiej kopii liczby. Kopia, w której cały wiersz
`| Elementy niosące tekst w widoku domyślnym |` usunięto, powoduje **błąd z nazwą brakującego
wiersza**, nie wartość domyślną i nie ciche pominięcie metryki.

*Słaba asercja:* `expect(readCeiling(REAL)).toEqual({regions: 8, …})`. Przechodzi funkcja
zwracająca stały obiekt i w ogóle nieczytająca pliku. Asercja rozstrzygająca: kopia z podmienioną
liczbą **oraz** kopia z usuniętym wierszem — stała pada na pierwszej, a domyślna wartość na
drugiej.

## AC-5 Sędzia mówi, która metryka i o ile — a wartość równa limitowi przechodzi
check: npx --no-install vitest run checks/tests/density-judge.test.ts

`judge(snapshot, ceiling, baseline)` nad zrzutami-fiksturami. Zrzut dokładnie na suficie
(`8, 96, 60, 1, 4, 2, 1`) → `verdict === 'pass'` — limit 8 znaczy „osiem wolno", nie „siedem".
Zrzut o jeden wyżej na każdej metryce → `verdict === 'over'`, `over` ma siedem pozycji, a każda
niesie nazwę metryki, wartość zmierzoną i limit. Zrzut mieszany (dwie metryki wyżej, pięć niżej)
→ dokładnie dwie pozycje, z nazwami tych dwóch. Kiedy skrypt bierze pomiar z dwóch szerokości
okna — `1100 px` (najwęższa wspierana, DESIGN.md §9) i `1512 px` (`[03 §4.1]`) — dla każdej
metryki liczy się **gorsza** z dwóch wartości, i test to sprawdza na zrzucie, w którym gorsza
wartość jest przy 1512.

*Słaba asercja:* `expect(judge(bad).verdict).toBe('over')`. Przechodzi sędzia, który nigdy nie
mówi `pass`. Asercja rozstrzygająca: zrzut dokładnie na suficie musi dać `pass`, a zrzut mieszany
musi wskazać **dokładnie te dwie** metryki po nazwie — komunikat „za gęsto" bez nazwy nie daje
się naprawić.

## AC-6 Metryka niezmierzona nigdy nie czyta się jak zero, a „nie dało się" to inne wyjście niż „nie ma czego"
check: npx --no-install vitest run checks/tests/density-unmeasured.test.ts

Dwie warstwy. Sędzia: zrzut, w którym brakuje klucza `agentCardLines`, daje
`notMeasured === ['agentCardLines']` i `verdict !== 'pass'`; zrzut, w którym ten sam klucz ma
wartość **`0`**, jest traktowany jako zmierzone zero i przechodzi — rozróżnienie `0` od `undefined`
jest całym sednem tej metryki. Skrypt: `bash checks/quick-density.sh` w piaskownicy,
z `LOADOUT_DENSITY_SNAPSHOT` wskazującym gotowy zrzut (szew, który pozwala pominąć przeglądarkę),
daje **0** przy zrzucie pod zapadką, **1** przy zrzucie z metryką nad sufitem, **1** przy zrzucie
z metryką niezmierzoną bez podanego powodu, **0** z wypisanym zdaniem `not measured: <metryka> —
<powód>` przy zrzucie, w którym kolektor podał powód, i **2** bez zmiennej i bez `dist/`, kiedy
`src/main.tsx` istnieje (jest co mierzyć, ale nie dało się). Kiedy `src/main.tsx` nie istnieje:
**0** ze zdaniem nazywającym brakującą ścieżkę.

*Słaba asercja:* `expect(judge(missing).verdict).not.toBe('pass')`. Przechodzi sędzia, który
odrzuca wszystko. Asercja rozstrzygająca: zrzut z jawnym zerem musi dać `pass` w tym samym teście,
plus tabela wyjść skryptu, w której `0` (nie ma czego) i `2` (nie dało się) są różne — to jest
dokładnie ta różnica, którą poprzedni prototyp zgubił, publikując „czysty przebieg axe, który nie zmierzył
niczego" `[03 §4.1]`.

## AC-7 Zapadka może tylko maleć
check: npx --no-install vitest run checks/tests/density-ratchet.test.ts

`checks/density-baseline.json` trzyma ostatnio zmierzoną wartość per metryka. Pomiar **powyżej**
zapadki, ale poniżej sufitu, daje exit 1 z komunikatem, że baseline może tylko maleć — sufit i
zapadka to dwie różne odmowy. `--update-baseline` przy pomiarze niższym przepisuje plik; przy
pomiarze wyższym kończy się kodem niezerowym, a plik jest **bajt w bajt** taki sam jak przed
próbą. Metryka nieobecna w pliku jest przyjmowana przy pierwszym pomiarze. Żadna wartość zapisana
do pliku nie może przekraczać sufitu z ARCHITECTURE §7 — zapadka nie ma prawa zabetonować stanu
gorszego niż deklarowany.

*Słaba asercja:* `expect(code).not.toBe(0)` przy próbie podniesienia. Przechodzi implementacja,
która odmawia i **mimo to** zapisuje plik. Asercja rozstrzygająca: porównanie bajtów pliku przed
i po odmowie **oraz** dowód, że dozwolone obniżenie faktycznie plik zmieniło — bez drugiej połowy
przechodzi też skrypt, który nie zapisuje nigdy.

## Świadomie poza zakresem

- **Kryterium akceptacji na nogę przeglądarkową `quick-density.sh`.** Świadomie go nie ma.
  `NOT_A_REAL_RED` zawiera `browser could not launch`, `Executable doesn't exist` i
  `Failed to launch`, więc kryterium wymagające Chromium na maszynie bez pobranych przeglądarek
  daje w tierze `before` czerwień, którą bramka odrzuca — a w tierze `full` daje zieleń, która
  nic nie znaczy. Kolektor jest więc oddzielony od sędziego i to **sędzia** ma kryteria; kolektor
  jest pokryty tabelą wyjść z AC-6 przez szew `LOADOUT_DENSITY_SNAPSHOT`. Repo źródłowe
  scertyfikowało siedem kryteriów na Chromium, które nie startowało.
- **Dwie z siedmiu metryk sufitu nie są mierzone mechanicznie.** „Metafory nawigacji na ekranie"
  jest osądem człowieka; „regiony animujące się od jednego zdarzenia" wymaga porównania dwóch
  klatek wokół zdarzenia. Obie mają wyjść ze skryptu jako `not measured` z **zapisanym powodem**
  (AC-6), nigdy jako zero. Zmierzenie ich to osobna decyzja.
- **`checks/quick-vocabulary.sh` i `checks/quick-tokens.sh`.** Istnieją i nie należą do tego
  zadania. `docs/PLAN.md` wymienia je przy T-22 jako obszar; ich pliki nie są w bloku OWNS,
  więc ich nie dotykamy.
- **`harness/guards.sh`.** Ma dziś funkcje `guard_quick_invariants` i `guard_quick_platform` dla
  sprawdzeń o nazwach, których w `checks/` nie ma, i nie ma funkcji dla `quick-boundary`,
  `quick-tokens` ani `quick-density`. To jest **prawdziwa luka do zgłoszenia**, ale `harness/`
  nie należy do tego zadania (AGENTS.md §7). Zgłoś ją w wynikach biegu; nie naprawiaj tutaj.
- **Uruchamianie `quick-density.sh` w CI.** `scripts/ci.sh` nie należy do tego zadania. Sprawdzenie
  odkryje bramka po nazwie pliku; wpięcie do CI to osobna zmiana.
- **Zapadka na czymkolwiek poza siedmioma liczbami z ARCHITECTURE §7.** Żadnych zrzutów
  ekranu jako baseline, żadnego axe. Pomysł „widoczność różnicowa" z `paint-audit.mjs` jest
  zanotowany jako notatka na później `[06 §10]`, nie jako kod teraz.

<!-- OWNS
checks/quick-boundary.sh
checks/quick-density.sh
checks/full-test.sh
checks/full-clippy.sh
checks/density-baseline.json
checks/tests
scripts/density-audit.mjs
-->
