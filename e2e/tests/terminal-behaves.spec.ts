/* Prawdziwa klawiatura w prawdziwej przeglądarce: kursor, strzałka, ślad po komendzie, klik.
 *
 * PO CO TO ISTNIEJE TUTAJ, A NIE OBOK KOMPONENTU. `renderToStaticMarkup` nie odpala ani jednego
 * zdarzenia, więc kursor i strzałka są dla niego niewidzialne — markup, który NIESIE `autofocus`,
 * i pole, które NAPRAWDĘ dostaje ognisko, to dwie różne rzeczy, a to repo nie ma jsdom. Ten plik
 * steruje warstwą, którą da się wysterować (`../harness.ts`: prawdziwy chromium, prawdziwy React,
 * atrapa `__TAURI_INTERNALS__`) i pyta o to, co robi człowiek: otwiera, pisze, naciska, klika.
 *
 * ZGŁOSZENIE, Z KTÓREGO TO WZIĘŁO SIĘ W CAŁOŚCI (właściciel, 2026-08-20): kursor nie stoi
 * w polu — trzeba kliknąć, za każdym razem; strzałka w górę nie cofa do poprzedniej linii;
 * komendy nie zostawiają po sobie ani jednego wiersza w strumieniu.
 *
 * SŁABA WERSJA: sam pierwszy przypadek. Przechodzi dla wiersza z `autoFocus` i bez ani jednej
 * z pozostałych rzeczy — czyli dla pola, które traci kursor przy pierwszym kliknięciu w strumień
 * i nigdy go nie odzyskuje. Dlatego cztery przypadki niżej stoją w jednym pliku.
 *
 * KONTROLA PRZECIW „OGNISKO KRADNIEMY ZAWSZE" jest tu równie ważna jak sama naprawa: wiersz,
 * który zabiera kursor po każdym kliknięciu, psuje każdy przycisk w kolumnie strumienia. Dlatego
 * jeden z przypadków wymaga, żeby ognisko ZOSTAŁO na przycisku, w który człowiek celował.
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

/** Kontrolka WEWNĄTRZ kolumny strumienia — cokolwiek, co ten strumień sam rysuje. */
const IN_STREAM = '[data-stream-column] button';

/** Linia bez skutku ubocznego: przy braku biegu odpowiada zdaniem i nic nie uruchamia. */
const TYPED = '/stop';

/** Ile czekamy, aż React dorysuje skutek naciśnięcia klawisza. Render, nie sieć. */
const SETTLE = 500;

/** Ile czekamy na pierwsze pojawienie się elementu, który ma przyjść po zdarzeniu. */
const APPEARS = 4_000;

/** Co trzyma teraz kursor — w postaci, którą da się wpisać w komunikat porażki. */
interface Focused {
  readonly tag: string;
  readonly label: string | null;
  readonly text: string;
}

function focused(app: RunningApp): Promise<Focused> {
  return app.page.evaluate(() => {
    const on = document.activeElement;
    if (on === null) return { tag: 'nothing', label: null, text: '' };
    return {
      tag: on.tagName.toLowerCase(),
      label: on.getAttribute('aria-label'),
      text: (on.textContent ?? '').replace(/\s+/g, ' ').trim().slice(0, 60),
    };
  });
}

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

/** Wpisuje linię i naciska Enter — dokładnie to, co robi człowiek. */
async function send(app: RunningApp, line: string): Promise<void> {
  await app.page.fill(FIELD, line);
  await app.page.press(FIELD, 'Enter');
  await app.page
    .locator(LINE)
    .first()
    .waitFor({ state: 'attached', timeout: APPEARS })
    .catch(() => undefined);
  await app.page.waitForTimeout(SETTLE);
}

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku:
 * `openApp()` jest leniwy, więc bez tego haka pierwszy `it` płaci cały rozruch pod swoim
 * limitem i pada na nim, mimo że każdy następny przechodzi na już postawionej aplikacji.
 * Ta sama para haków stoi w `no-dead-controls.spec.ts` i z tego samego powodu. */
beforeAll(async () => {
  await openApp();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('the entry row behaves like a terminal under a real keyboard', () => {
  it('holds the caret on a freshly opened application, with nobody clicking anything', async () => {
    const app = await openWork();
    try {
      expect(
        await app.page.locator(FIELD).count(),
        'the work screen has to render the command line at all, or the question "where is the ' +
          'caret" is a question about an empty screen',
      ).toBe(1);

      const on = await focused(app);
      expect(
        on.label,
        'nothing was clicked and the caret is not in the command line: it sits on ' +
          JSON.stringify(on) +
          '. A row named a terminal that needs one click before it accepts a character charges ' +
          'that click on every single visit to this screen — and the person is already looking ' +
          'at a field with a prompt glyph in front of it.',
      ).toBe('Command line');
    } finally {
      await app.close();
    }
  }, 90_000);

  it('leaves the line you sent standing in the stream, and gives it back on ArrowUp', async () => {
    const app = await openWork();
    try {
      expect(
        await app.page.locator(LINE).count(),
        'the stream has to be empty before the line is sent, or "the line is there" says nothing ' +
          'about what sending it did',
      ).toBe(0);

      await send(app, TYPED);

      const rows = await app.page.locator(LINE).allInnerTexts();
      expect(
        rows.some((row) => row.includes(TYPED)),
        'after sending ' +
          JSON.stringify(TYPED) +
          ' the stream carries no row with that line. The stream says: ' +
          JSON.stringify(rows) +
          '. A terminal in which a typed command leaves no trace is indistinguishable from a ' +
          'terminal that never took the command — the person cannot tell "it was refused" from ' +
          '"it was never read", and there is nowhere else to look.',
      ).toBe(true);

      await app.page.press(FIELD, 'ArrowUp');
      await app.page.waitForTimeout(SETTLE);

      expect(
        await app.page.inputValue(FIELD),
        'ArrowUp has to put the line back into the field. Without it the only way to repeat a ' +
          'command is to type it again from memory, and the longest lines here are the ones ' +
          'nobody wants to retype.',
      ).toBe(TYPED);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('leaves the caret on a control inside the stream when that control was clicked', async () => {
    const app = await openWork();
    try {
      await send(app, TYPED);

      const controls = await app.page.locator(IN_STREAM).count();
      expect(
        controls,
        'this case needs one control inside the stream column to aim at. Zero means it is a ' +
          'statement about nothing — and the assertion below, the one that says the caret comes ' +
          'BACK to the field, would then prove nothing either.',
      ).toBeGreaterThan(0);

      const control = app.page.locator(IN_STREAM).first();
      const name = (await control.innerText()).replace(/\s+/g, ' ').trim();
      await control.click();
      await app.page.waitForTimeout(SETTLE);

      const on = await focused(app);
      expect(
        on.tag === 'button' && on.text.includes(name),
        'clicking ' +
          JSON.stringify(name) +
          ' inside the stream left the caret on ' +
          JSON.stringify(on) +
          ' instead of on the button. A row that takes the caret back after EVERY click in this ' +
          'column breaks every control the stream draws: the person aimed at one thing and the ' +
          'keyboard went somewhere else.',
      ).toBe(true);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('hands the caret back when the click landed in the stream, but on no control', async () => {
    const app = await openWork();
    try {
      await send(app, TYPED);

      /* Najpierw wyprowadzamy kursor z pola, i to prawdziwym kliknięciem: bez tego kroku ten
       * przypadek przechodzi na tym, że kursor nigdy nie wyszedł, czyli mierzy poprzedni. */
      expect(
        await app.page.locator(IN_STREAM).count(),
        'the caret has to be taken out of the field first, and a control inside the stream is ' +
          'what takes it. Without one this case measures nothing.',
      ).toBeGreaterThan(0);
      await app.page.locator(IN_STREAM).first().click();
      await app.page.waitForTimeout(SETTLE);

      const away = await focused(app);
      expect(
        away.label,
        'the click was supposed to move the caret off the command line, and it is still there. ' +
          'That makes the assertion below true for a screen that never gives the caret back.',
      ).not.toBe('Command line');

      /* Wiersz historii: element w kolumnie strumienia, na którym nie stoi żadna kontrolka. */
      await app.page.locator(LINE).first().click();
      await app.page.waitForTimeout(SETTLE);

      const on = await focused(app);
      expect(
        on.label,
        'clicking in the stream where there is nothing to click has to give the caret back to ' +
          'the command line; it went to ' +
          JSON.stringify(on) +
          ' instead. This is the second half of the caret defect: a field that starts focused ' +
          'and never recovers is a field that works exactly once, and the person is back to ' +
          'clicking it before every line.',
      ).toBe('Command line');
    } finally {
      await app.close();
    }
  }, 90_000);
});
