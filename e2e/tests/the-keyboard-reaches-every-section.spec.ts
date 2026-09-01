/* Paleta poleceń i skoki klawiaturą, sprawdzone w prawdziwej przeglądarce, prawdziwym
 * naciśnięciem klawisza.
 *
 * PO CO TO ISTNIEJE, A NIE WYSTARCZY `renderToStaticMarkup`. Wszystko, o co pyta ten plik,
 * jest zdarzeniem: nasłuch na dokumencie, ognisko, które wraca tam, skąd przyszło, `Escape`,
 * który zamyka okno, klik w tło. Statyczny render nie odpala ani jednego z nich, więc paleta
 * mogłaby mieć komplet poprawnego markupu i nie otwierać się ani razu — to jest ta sama klasa
 * wady, dla której ten harness w ogóle powstał (nagłówek `e2e/harness.ts`).
 *
 * PIERWSZY PRZYPADEK NIŻEJ JEST NAJWAŻNIEJSZY I NIE DOTYCZY PALETY. Ekran pracy otwiera się
 * z kursorem w wierszu wejścia (`src/sections/run/entry/entry.tsx`, `autoFocus`), więc pierwsze,
 * co ta aplikacja robi po starcie, to czeka na pisanie. Skok bez modyfikatora, który nie pyta
 * o ognisko, zamienia wpisanie słowa „grand" w dwa przeskoki ekranu — i jest to najczęstsza
 * wada tego wzorca, nie skrajny przypadek.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp } from '../harness';
import { closeEverything, openApp } from '../harness';

/** Wiersz wejścia ekranu pracy — prawdziwe pole tekstowe, w którym stoi kursor po starcie. */
const COMMAND_LINE = 'main input[aria-label="Command line"]';

/** Okno palety. `data-palette` niesie to, która z dwóch list w nim stoi. */
const PALETTE = '[role="dialog"][data-palette]';

/** Ile czekamy na przemontowanie sekcji albo pojawienie się okna. React, nie sieć. */
const SETTLES = 4_000;

/** Agent lezacy na dysku, w komplecie. Panel otwiera sie na kazde jego pole. */
const SAVED_AGENT = {
  schema: 1,
  id: 'agent-1',
  name: 'Reviewer',
  summary: 'Reads a change and says what it breaks.',
  color: 'clay',
  instructions: 'Read the change and say what it breaks.',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'look-only',
  giveUpAfterMinutes: 10,
  tools: 'everything',
  reachesTheWeb: false,
  skills: [],
  connections: [],
  writeResultsTo: '',
};

beforeAll(async () => {
  await openApp();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

/** Która sekcja stoi w powłoce w tej chwili. */
async function sectionNow(app: RunningApp): Promise<string> {
  return app.page.evaluate(
    () => document.querySelector('main[data-section]')?.getAttribute('data-section') ?? '',
  );
}

/** Zdejmuje ognisko z pola tekstowego — tak jak człowiek, który przestał pisać. */
async function stopTyping(app: RunningApp): Promise<void> {
  await app.page.evaluate(() => {
    const standing = document.activeElement as { blur?: () => void } | null;
    if (standing !== null && typeof standing.blur === 'function') standing.blur();
  });
}

describe('the keyboard reaches every section, and never while somebody is typing', () => {
  it('leaves the screen alone while the word "grand" goes into the command line', async () => {
    const app = await openApp();
    try {
      await app.page.locator(COMMAND_LINE).waitFor({ state: 'visible', timeout: SETTLES });
      /* Kontrola przeciw pustej asercji: gdyby kursor NIE stał w polu, ten przypadek nie mówiłby
         nic o piszącym człowieku — mówiłby o klawiszach wysłanych w próżnię. */
      expect(
        await app.page.evaluate(() => document.activeElement?.getAttribute('aria-label') ?? ''),
        'the work screen is supposed to open with the cursor already in its command line',
      ).toBe('Command line');

      await app.page.keyboard.type('grand');

      expect(
        await sectionNow(app),
        'typing a word into a field must not move the screen. "g" arming a jump and "r" taking ' +
          'it is the defect this whole file exists for.',
      ).toBe('run');
      expect(await app.page.inputValue(COMMAND_LINE)).toBe('grand');
      expect(await app.page.locator(PALETTE).count()).toBe(0);

      /* KONTROLA PRZECIW PUSTEJ ASERCJI, w tym samym przypadku. Wszystko wyżej jest prawdziwe
         także o aplikacji, która nie ma ŻADNEGO skrótu — czyli o tej sprzed tej zmiany. Te same
         litery, ten sam ekran, tylko kursor już nie stoi w polu: teraz skok MA się wydarzyć. */
      await stopTyping(app);
      await app.page.keyboard.press('g');
      await app.page.keyboard.press('w');
      await app.page
        .locator('main[data-section="workflows"]')
        .waitFor({ state: 'attached', timeout: SETTLES })
        .catch(() => undefined);
      expect(await sectionNow(app)).toBe('workflows');
    } finally {
      await app.close();
    }
  }, 60_000);

  it('takes the same two letters as a jump once nobody is typing', async () => {
    const app = await openApp();
    try {
      await app.page.locator(COMMAND_LINE).waitFor({ state: 'visible', timeout: SETTLES });
      await stopTyping(app);

      await app.page.keyboard.press('g');
      await app.page.keyboard.press('w');
      await app.page
        .locator('main[data-section="workflows"]')
        .waitFor({ state: 'attached', timeout: SETTLES })
        .catch(() => undefined);
      expect(await sectionNow(app)).toBe('workflows');

      await stopTyping(app);
      await app.page.keyboard.press('g');
      await app.page.keyboard.press('a');
      await app.page
        .locator('main[data-section="agents"]')
        .waitFor({ state: 'attached', timeout: SETTLES })
        .catch(() => undefined);
      expect(await sectionNow(app)).toBe('agents');
    } finally {
      await app.close();
    }
  }, 60_000);

  it('opens on the one shortcut that carries a modifier, even from inside the field', async () => {
    const app = await openApp();
    try {
      await app.page.locator(COMMAND_LINE).waitFor({ state: 'visible', timeout: SETTLES });
      await app.page.keyboard.press('Meta+k');
      await app.page.locator(PALETTE).waitFor({ state: 'visible', timeout: SETTLES });

      expect(await app.page.getAttribute(PALETTE, 'data-palette')).toBe('items');
      expect(await app.page.getAttribute(PALETTE, 'aria-modal')).toBe('true');
      /* Kursor ma stać w palecie, a nie tam, gdzie stał przed nią: okno, które się otwiera
         i nie bierze klawiatury, każe sięgnąć po mysz po to, żeby zacząć pisać. */
      expect(
        await app.page.evaluate(() => document.activeElement?.closest('[data-palette]') !== null),
      ).toBe(true);
    } finally {
      await app.close();
    }
  }, 60_000);

  it('closes on Escape and hands the cursor back where it came from', async () => {
    const app = await openApp();
    try {
      await app.page.locator(COMMAND_LINE).waitFor({ state: 'visible', timeout: SETTLES });
      await app.page.keyboard.press('Meta+k');
      await app.page.locator(PALETTE).waitFor({ state: 'visible', timeout: SETTLES });

      await app.page.keyboard.press('Escape');
      await app.page.locator(PALETTE).waitFor({ state: 'detached', timeout: SETTLES });

      expect(await app.page.locator(PALETTE).count()).toBe(0);
      expect(
        await app.page.evaluate(() => document.activeElement?.getAttribute('aria-label') ?? ''),
        'the cursor has to come back to the field it was taken from, or every shortcut costs ' +
          'a click to get back to work',
      ).toBe('Command line');
    } finally {
      await app.close();
    }
  }, 60_000);

  it('closes on a click in the darkened space behind it', async () => {
    const app = await openApp();
    try {
      await app.page.keyboard.press('Meta+k');
      await app.page.locator(PALETTE).waitFor({ state: 'visible', timeout: SETTLES });
      /* Lewy górny róg tła. Okno stoi u góry NA ŚRODKU (`max-w-160` i `pt-20`), więc ten punkt
         leży pod przyciemnieniem i nad niczym innym — tło jest `fixed inset-0` i najwyżej. */
      await app.page.mouse.click(8, 8);
      await app.page.locator(PALETTE).waitFor({ state: 'detached', timeout: SETTLES });
      expect(await app.page.locator(PALETTE).count()).toBe(0);
    } finally {
      await app.close();
    }
  }, 60_000);

  it('narrows on what is typed and moves the screen on Enter', async () => {
    const app = await openApp();
    try {
      await app.page.locator(COMMAND_LINE).waitFor({ state: 'visible', timeout: SETTLES });
      await app.page.keyboard.press('Meta+k');
      await app.page.locator(PALETTE).waitFor({ state: 'visible', timeout: SETTLES });

      await app.page.keyboard.type('knowl');
      expect(await app.page.locator(PALETTE + ' [role="option"]').count()).toBe(1);

      await app.page.keyboard.press('Enter');
      await app.page
        .locator('main[data-section="knowledge"]')
        .waitFor({ state: 'attached', timeout: SETTLES })
        .catch(() => undefined);
      expect(await sectionNow(app)).toBe('knowledge');
      expect(await app.page.locator(PALETTE).count()).toBe(0);
    } finally {
      await app.close();
    }
  }, 60_000);

  it('walks the list with the arrows before Enter picks anything', async () => {
    const app = await openApp();
    try {
      await app.page.locator(COMMAND_LINE).waitFor({ state: 'visible', timeout: SETTLES });
      await app.page.keyboard.press('Meta+k');
      await app.page.locator(PALETTE).waitFor({ state: 'visible', timeout: SETTLES });

      /* Trzecia pozycja listy to trzecia sekcja rejestru — Agents. Dwie strzałki w dół
         z pierwszej pozycji. */
      await app.page.keyboard.press('ArrowDown');
      await app.page.keyboard.press('ArrowDown');
      await app.page.keyboard.press('Enter');
      await app.page
        .locator('main[data-section="agents"]')
        .waitFor({ state: 'attached', timeout: SETTLES })
        .catch(() => undefined);
      expect(await sectionNow(app)).toBe('agents');
    } finally {
      await app.close();
    }
  }, 60_000);

  it('answers "?" with the shortcuts, and never while somebody is typing', async () => {
    const app = await openApp();
    try {
      await app.page.locator(COMMAND_LINE).waitFor({ state: 'visible', timeout: SETTLES });
      await app.page.keyboard.type('?');
      expect(
        await app.page.locator(PALETTE).count(),
        'a question mark typed into a field is a question mark, not a shortcut',
      ).toBe(0);

      await stopTyping(app);
      await app.page.keyboard.press('?');
      await app.page.locator(PALETTE).waitFor({ state: 'visible', timeout: SETTLES });
      expect(await app.page.getAttribute(PALETTE, 'data-palette')).toBe('shortcuts');

      const listed = await app.page.evaluate(() =>
        [...document.querySelectorAll('[data-shortcut]')].map(
          (row) => row.getAttribute('data-shortcut') ?? '',
        ),
      );
      expect(listed).toContain('G W');
      expect(listed).toContain('⌘K');
      /* Krótka, nie ściana — ta sama liczba, której pilnuje kryterium bez przeglądarki. */
      expect(listed.length).toBeLessThanOrEqual(13);
    } finally {
      await app.close();
    }
  }, 60_000);

  it('lists a saved workflow and asks the work screen to run the one that was picked', async () => {
    const app = await openApp({
      replies: {
        /* ZMIERZONE 2026-08-31: zanim paleta zdąży zapytać, ta sama komenda leci CZTERY razy
           (ekran pracy przy montażu, pasek kart, lista wyboru). Kolejka odpowiedzi jest zdejmowana
           po jednej, więc krótsza od tego dawałaby palecie pustą listę i kryterium mówiłoby
           o głębokości kolejki, a nie o palecie. Jedna pozycja i ani jednego zmyślonego pola
           poza jej nazwą. */
        list_workflows: Array.from({ length: 12 }, () => ({
          value: [
            {
              path: 'ship-a-feature.json',
              place: 'library',
              workflow: {
                format: 1,
                id: 'wf-1',
                name: 'Ship a feature',
                steps: [],
                links: [],
              },
            },
          ],
        })),
      },
    });
    try {
      await app.page.locator(COMMAND_LINE).waitFor({ state: 'visible', timeout: SETTLES });
      await app.page.keyboard.press('Meta+k');
      await app.page.locator(PALETTE).waitFor({ state: 'visible', timeout: SETTLES });
      await app.page.keyboard.type('Ship a feature');

      const rowKinds = await app.page.evaluate(() =>
        [...document.querySelectorAll('[data-palette-item]')].map(
          (row) => row.getAttribute('data-palette-item') ?? '',
        ),
      );
      expect(
        rowKinds,
        'a workflow saved on disk has to reach the palette, or the palette can only ever jump',
      ).toContain('workflow');

      await app.page.keyboard.press('Enter');
      await app.page
        .locator('main[data-section="run"]')
        .waitFor({ state: 'attached', timeout: SETTLES })
        .catch(() => undefined);
      expect(
        await sectionNow(app),
        'picking a workflow has to take the person to the screen where work happens',
      ).toBe('run');

      /* I MA TAM COŚ ZROBIĆ. Sam przeskok ekranu przechodzi także dla pozycji, która wyrzuca
         nazwę pliku — dokładnie ta wada stała przez całe T-38 pod zielonym `Run` w edytorze
         workflow. Zdanie „Nothing started:" powstaje WYŁĄCZNIE w `src/sections/run/launch.ts`,
         więc jego obecność znaczy, że prośba palety doszła do polityki startu i została
         rozpatrzona. Ta scena nie ma wybranego zakresu ani kroków, więc bieg nie ma prawa
         ruszyć — i to jest w porządku: kryterium pyta o drogę, nie o cudzą odmowę. */
      await app.page
        .locator('main:has-text("Nothing started:")')
        .waitFor({ state: 'attached', timeout: SETTLES })
        .catch(() => undefined);
      expect(
        await app.page.evaluate(() => document.body.textContent ?? ''),
        'the work screen has to answer the pick — silence after Enter is the defect this whole ' +
          'road exists to close',
      ).toContain('Nothing started:');
    } finally {
      await app.close();
    }
  }, 60_000);

  it('lists a saved agent and takes the person to the shelf that holds it', async () => {
    const app = await openApp({
      replies: {
        /* Ta sama głębokość kolejki i ten sam powód, co przy workflow wyżej. Sama paleta czyta
           z tej odpowiedzi DWA pola — identyfikator i nazwę — ale od 2026-08-31 wybór agenta
           OTWIERA jego panel, a panel czyta każde pole. Skrócony agent zostawiał tu ekran
           przewrócony na `ScreenBoundary` („This screen stopped working"), a oba zdania niżej
           i tak przechodziły: kryterium mierzyło wtedy rozbity ekran i nie miało jak tego
           powiedzieć. Pełny agent jest jedyną fiksturą, która sądzi to, co obiecuje. */
        list_agents: Array.from({ length: 12 }, () => ({
          value: [SAVED_AGENT],
        })),
      },
    });
    try {
      await app.page.locator(COMMAND_LINE).waitFor({ state: 'visible', timeout: SETTLES });
      await app.page.keyboard.press('Meta+k');
      await app.page.locator(PALETTE).waitFor({ state: 'visible', timeout: SETTLES });
      await app.page.keyboard.type('Reviewer');

      expect(
        await app.page.evaluate(() =>
          [...document.querySelectorAll('[data-palette-item]')].map(
            (row) => row.getAttribute('data-palette-item') ?? '',
          ),
        ),
        'an agent saved on disk has to reach the palette next to the sections and the workflows',
      ).toEqual(['agent']);

      await app.page.keyboard.press('Enter');
      await app.page
        .locator('main[data-section="agents"]')
        .waitFor({ state: 'attached', timeout: SETTLES })
        .catch(() => undefined);
      /* CO TO DOWODZI. Że paleta zamyka się i powłoka staje na półce, na której ten agent leży.
         O drugą połowę drogi — czy panel TEGO agenta jest otwarty — pyta osobno przypadek
         niżej, bo to osobna rzecz i psuje się osobno. */
      expect(await sectionNow(app)).toBe('agents');
      expect(await app.page.locator(PALETTE).count()).toBe(0);
    } finally {
      await app.close();
    }
  }, 60_000);

  it('opens the panel of the agent that was picked, not just the shelf it lies on', async () => {
    const app = await openApp({
      replies: {
        /* Pelny agent, nie sama nazwa: panel czyta kazde pole, a agent z brakami dowiodlby
           tylko tego, ze panel wstaje z dziurami. Kolejka tak gleboka jak wyzej -- ekran
           Agents czyta ten sam `list_agents`, co paleta, i czyta go przy kazdym wejsciu. */
        list_agents: Array.from({ length: 12 }, () => ({
          value: [SAVED_AGENT],
        })),
      },
    });
    try {
      await app.page.locator(COMMAND_LINE).waitFor({ state: 'visible', timeout: SETTLES });
      await app.page.keyboard.press('Meta+k');
      await app.page.locator(PALETTE).waitFor({ state: 'visible', timeout: SETTLES });
      await app.page.keyboard.type('Reviewer');
      await app.page.keyboard.press('Enter');

      await app.page
        .locator('#agent-name')
        .waitFor({ state: 'visible', timeout: SETTLES })
        .catch(() => undefined);

      /* TO JEST DRUGA POLOWA DROGI. Pierwsza -- „powloka staje na polce Agents" -- stoi
         w przypadku wyzej i przechodzila, kiedy ta tu jeszcze nie istniala: prosba zapisana
         przez palete nie miala ani jednego czytelnika, wiec czlowiek ladowal na liscie i sam
         musial znalezc na niej kafelek, o ktory wlasnie poprosil. Pytamy o POLE, ktore czlowiek
         widzi i w ktorym moze poprawic litere, a nie o wartosc oddana przez funkcje. */
      expect(
        await app.page.inputValue('#agent-name').catch(() => ''),
        'picking a saved agent in the palette has to open that agent, not leave the person on ' +
          'a list to find it again by eye',
      ).toBe('Reviewer');
    } finally {
      await app.close();
    }
  }, 60_000);
});
