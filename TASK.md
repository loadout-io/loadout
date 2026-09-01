# T-161 — Długi workflow zostaje wewnątrz ekranu Run

Pasek kroków ma własne przewijanie, ale jego dzieci nadal potrafią narzucić szerokość całemu
ekranowi. Przy długim grafie albo szerokich kontrolkach prawa kolumna Agents wypada poza
viewport, a globalne `overflow: hidden` tylko ukrywa skutek. To zadanie ustala jedną twardą
granicę geometrii: nadmiar zostaje wewnątrz paska, a dwukolumnowy ekran nigdy się od niego
nie rozszerza.

Zadanie nie przenosi ustawień biegu, nie wybiera Lead, nie zmienia semantyki kroków i nie
naprawia listy NOW. Dzięki temu nie współdzieli ścieżek z T-151…T-160 ani z przyszłym Settings.

**Zależy od:** brak.

**Read first:** `AGENTS.md` niezmienniki 13, 17, 18 i 29 ·
`docs/ARCHITECTURE.md` §7 · `docs/design/DESIGN.md` §2 ·
`src/sections/run/index.tsx` (`WORK_COLUMNS`, `FEED_ROWS`) ·
`src/sections/run/strip/strip.tsx` · `src/sections/run/rail/rail.tsx` ·
`src-tauri/tauri.conf.json` · `e2e/harness.ts`.

## Tryb wykonania

- Codex w osobnym worktree, bez osobnego Harnessu.
- Obowiązuje uczciwe czerwone `before`, quick/full i read-only review.
- Bez zmian w `index.tsx`, `start.tsx`, railu, ustawieniach ani `e2e/harness.ts`.

## Uczciwe `before`

Target istnieje przed `./verify.sh before` i montuje prawdziwą aplikację w Chromium. Ładuje
workflow z co najmniej trzydziestoma długimi nazwami kroków, ustawia wspierane minimum
`1100 × 700` i doprowadza prawdziwy pasek do stanu z krokiem biegnącym blisko końca grafu.
Przed poprawką pada na asercji geometrii railu albo overflow całego ekranu. Brak Vite,
Chromium, fixture, railu lub zero testów nie są czerwienią.

## AC-1 Pasek zatrzymuje własny nadmiar, a reszta Run pozostaje dostępna
check: npx --no-install vitest run e2e/tests/t161-long-workflow-stays-inside-run.spec.ts
expect: (\d+) passed

Spec wykonuje scenę przy `1100 × 700` i kontrolę przy `1440 × 900`. W obu rozmiarach:

- dokument, sekcja Run i dwukolumnowy obszar pracy nie mają poziomego overflow;
- prawy brzeg `[data-rail]` mieści się w widocznym `main`, a rail ma niezerową szerokość;
- pole `Command line` pozostaje widoczne w pionowym viewport;
- `[data-strip]` zachowuje dokładnie `STRIP_HEIGHT` i nie tworzy drugiego rzędu chrome;
- wszystkie kroki pozostają w DOM, a ich nadmiar przewija wyłącznie `[data-blocks]`;
- pierwszy naprawdę biegnący blok jest wewnątrz widocznego wycinka bez płynnej animacji;
- prawdziwe kontrolki paska pozostają osiągalne i klikalne — poprawka nie może ich ukryć,
  przeskalować, uciąć bez drogi dostępu ani przesunąć poza ekran.

Test mierzy `scrollWidth`, `clientWidth` i bounding boxy prawdziwego DOM; nie asertuje samych
nazw klas.

<!-- OWNS
tasks/T-161.md
src/sections/run/strip/strip.tsx
e2e/tests/t161-long-workflow-stays-inside-run.spec.ts
-->
