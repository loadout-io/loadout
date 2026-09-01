/* AC-4 dla T-72: człowiek wpisuje komendę i WIDZI kafelek — na prawdziwym froncie.
 *
 * PO CO TO ISTNIEJE OBOK KRYTERIUM O CZYSTEJ FUNKCJI. Niezmiennik 29, słowo w słowo: rejestr,
 * który zna uruchomioną rzecz, nie jest dowodem, że człowiek ją widzi. Fala z 2026-08-20 dała
 * cztery trafienia tej klasy NA ZIELONEJ BRAMCE — przycisk rysowany wyłącznie z propsa, którego
 * żaden produkcyjny wołający nie podawał; odmowa dowodzona na wartości, a nie na zdaniu; wiersz
 * istniejący w modelu i bez drogi na ekran. Kryterium obok tego (`../../src/sections/run/rail/
 * processes-are-not-agents.test.ts`) pyta funkcję. Ten plik pyta o to, co się STAŁO: prawdziwy
 * chromium, prawdziwy React, prawdziwa klawiatura, prawdziwe kliknięcie.
 *
 * SŁABA WERSJA: „w kolumnie agentów coś się pojawiło". Przechodzi, gdy kafelek narysował się dla
 * agenta biegu, który akurat idzie — czyli dla ekranu, na którym wpisana komenda nie zrobiła
 * absolutnie nic. Rozróżniają to dwie rzeczy naraz: kafelek musi stać we WŁASNEJ grupie i nieść
 * TĘ komendę (a), a przed naciśnięciem Enter takiego kafelka nie ma ani jednego (d).
 *
 * CZEGO TEN PLIK NIE DOWODZI, i to jest granica, nie kompromis do ukrycia. Granica Rusta jest tu
 * atrapą (`../harness.ts`): `invoke` odpowiada kształtem i nic nie uruchamia. Ten plik nie mówi
 * więc nic o tym, czy po drugiej stronie naprawdę wstał proces we własnej grupie — to jest pytanie
 * kryteriów rustowych tego samego zadania (`src-tauri/tests/it/started_process_is_ours.rs`).
 * Mówi za to jedyną rzecz, której tamte nie umieją powiedzieć: że droga od klawiatury do kafelka
 * istnieje i jest przejezdna.
 *
 * KAFELEK MUSI ZOSTAĆ NA EKRANIE PO POWROCIE `invoke`. Atrapa odpowiada natychmiast, więc
 * implementacja, która zdejmuje kafelek w `finally` wywołania — tak jak `io.ts` zdejmuje pasek
 * biegu — zgasi go w tym samym tyknięciu, w którym go postawiła. To nie jest ograniczenie
 * harnessu: rzecz zamówiona komendą kończy się wtedy, kiedy KOŃCZY SIĘ ONA, a nie wtedy, kiedy
 * wraca wywołanie, które ją zamówiło. Kafelek zgaszony powrotem wywołania jest tą samą wadą
 * w drugą stronę co „Running" nad rzeczą, która zeszła.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp } from '../harness';
import { closeEverything, openApp } from '../harness';

/** Ekran pracy jest pierwszą sekcją okna (`src/ui/shell/section-store.ts`), więc nikt nie klika. */
const WORK = 'main[data-section="run"]';

/** Pole wiersza wejścia. Ta sama etykieta, po której idzie czytnik ekranu. */
const FIELD = '[aria-label="Command line"]';

/** Wiersz historii; `data-line` niesie identyfikator linii (`src/sections/run/feed/line.tsx`). */
const LINE = '[data-line]';

/**
 * Kafelek rzeczy uruchomionej komendą — własna grupa nad obrazem planu.
 *
 * Osobny znacznik od kafelka kroku, bo to jest dokładnie ta różnica, którą kryterium mierzy:
 * kroki biegu i rzeczy uruchomione przez człowieka nie mieszają się w jednej liście. Rzecz
 * uruchomiona komendą nie stoi na żadnym kroku i nie ma czego na tym obrazie narysować.
 */
const TILE = '[data-plan-column] [data-started]';

/** Kafelek kroku biegu. Stoi tu po to, żeby dało się powiedzieć, że to NIE on się pojawił. */
const AGENT_TILE = '[data-plan-column] [data-step]';

/** To, co otwiera kliknięcie w kafelek: wyjście tej jednej rzeczy. */
const OUTPUT = '[data-started-output]';

/** Wiersz powłoki, który wpisuje człowiek. */
const COMMAND = 'npm run dev';

/** Cała linia, dokładnie tak, jak stanie w polu. */
const TYPED = '/start ' + COMMAND;

/** Ile czekamy, aż React dorysuje skutek naciśnięcia klawisza. Render, nie sieć. */
const SETTLE = 500;

/** Ile czekamy na pierwsze pojawienie się elementu, który ma przyjść po zdarzeniu. */
const APPEARS = 4_000;

/** Otwiera aplikację i czeka na ekran pracy. Ani jednego kliknięcia — praca jest pierwsza. */
async function openWork(): Promise<RunningApp> {
  const app = await openApp();
  await app.page
    .locator(WORK)
    .waitFor({ state: 'attached', timeout: APPEARS })
    .catch(() => undefined);
  await app.page
    .locator(FIELD)
    .waitFor({ state: 'attached', timeout: APPEARS })
    .catch(() => undefined);
  return app;
}

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku:
 * `openApp()` jest leniwy, więc bez tego haka jedyny `it` płaci cały rozruch pod swoim limitem.
 * Ta sama para haków stoi w `terminal-behaves.spec.ts` i z tego samego powodu. */
beforeAll(async () => {
  await openApp();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('typing a command puts a tile in the plan column and opens it on a click', () => {
  /* JEDEN PRZYPADEK NA CAŁĄ SCENĘ, i to jest wybór, nie skrót. Cztery zdania tego kryterium są
   * czterema krokami JEDNEJ czynności człowieka — nie ma kafelka, wpisuję, jest kafelek, wchodzę
   * w niego — a rozbite na cztery `it` każde z nich otwierałoby własną kartę i sprawdzałoby stan
   * sprzed swojego własnego kliknięcia. Kontrola (d) ma sens wyłącznie w tej samej karcie, w której
   * padnie (a): „przedtem nie było" i „potem jest" to jedno zdanie o jednym oknie. */
  it('shows nothing before, a tile carrying the line after, and its output on a click', async () => {
    const app = await openWork();
    try {
      expect(
        await app.page.locator(FIELD).count(),
        'the work screen has to render the command line at all, or every question below is a ' +
          'question about an empty screen',
      ).toBe(1);

      // ── (d) KONTROLA PRZECIW PUSTEMU PRZEJŚCIU ────────────────────────────────────────
      expect(
        await app.page.locator(TILE).count(),
        'there is no tile of this kind before anything was typed — and there had better not be. ' +
          'Without this line "a tile is there" says nothing about what typing the line did: a ' +
          'column that draws one for every run, or one placeholder for looks, passes the whole ' +
          'rest of this case on furniture that was already on screen.',
      ).toBe(0);

      await app.page.fill(FIELD, TYPED);
      await app.page.press(FIELD, 'Enter');
      await app.page
        .locator(TILE)
        .first()
        .waitFor({ state: 'attached', timeout: APPEARS })
        .catch(() => undefined);
      await app.page.waitForTimeout(SETTLE);

      // ── (a) KAFELEK NIESIE TĘ KOMENDĘ, WE WŁASNEJ GRUPIE ──────────────────────────────
      const tiles = await app.page.locator(TILE).allInnerTexts();
      expect(
        tiles.some((tile) => tile.includes(COMMAND)),
        'after sending ' +
          JSON.stringify(TYPED) +
          ' the column carries no tile with that line on it. The column says: ' +
          JSON.stringify(tiles) +
          '. This is the whole request: the person asked Loadout to run something and wants to ' +
          'see it standing there afterwards. A line that leaves nothing behind is ' +
          'indistinguishable from a line the row never took — and the only other place to look ' +
          'would be ps.',
      ).toBe(true);

      const agents = await app.page.locator(AGENT_TILE).allInnerTexts();
      expect(
        agents.some((tile) => tile.includes(COMMAND)),
        'and it must not have joined the steps of the run: ' +
          JSON.stringify(agents) +
          '. Those are two different kinds of thing, and the control under each one means ' +
          'something else. Mixing them also makes the assertion above true for a screen that ' +
          'grew an agent tile named after the line, which is a relation that is not in the data.',
      ).toBe(false);

      // ── (b) WIERSZ ECHA STOI W STRUMIENIU (T-58 AC-2 NIE MA SIĘ ZEPSUĆ) ───────────────
      const rows = await app.page.locator(LINE).allInnerTexts();
      expect(
        rows.some((row) => row.includes(TYPED)),
        'the line the person typed has to stay in the stream too. The stream says: ' +
          JSON.stringify(rows) +
          '. A new command that skips the echo takes back the fix from the wave before this one: ' +
          'a terminal in which what you typed leaves no trace cannot tell you "it was refused" ' +
          'from "it was never read".',
      ).toBe(true);

      // ── (c) KLIKNIĘCIE OTWIERA JEGO WYJŚCIE ───────────────────────────────────────────
      expect(
        await app.page.locator(OUTPUT).count(),
        'nothing of this kind is open before the tile is clicked, or "the click opened it" is ' +
          'a statement about something that was already there.',
      ).toBe(0);

      await app.page.locator(TILE).first().click();
      await app.page.waitForTimeout(SETTLE);

      const opened = await app.page.locator(OUTPUT).allInnerTexts();
      expect(
        opened.length,
        'clicking the tile changed nothing in the document. The owner asked for exactly this — ' +
          '"po kliku moge tam wejsc" — and a tile you cannot enter is a control without a ' +
          'handler with extra steps (invariant 16).',
      ).toBeGreaterThan(0);
      expect(
        opened.some((panel) => panel.includes(COMMAND)),
        'and what opened has to be THIS one: ' +
          JSON.stringify(opened) +
          '. A panel that opens without saying which line it belongs to is the same screen for ' +
          'every tile, and with two of them running the person is reading one and looking at ' +
          'the other.',
      ).toBe(true);
    } finally {
      await app.close();
    }
  }, 90_000);
});
