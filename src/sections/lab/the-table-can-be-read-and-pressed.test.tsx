import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { createLabStore } from '../../state/lab';
import type { EvalBoard, LabIo } from './io';
import LabScreen from './index';

/* DWA MIEJSCA, W KTÓRE CZŁOWIEK NATURALNIE KLIKA, I OBA BYŁY MARTWYM TEKSTEM.
 *
 * Wiersz (`<th data-lab-row>`) i komórka (`<td data-lab-cell>`) nie miały ani jednego handlera,
 * ani jednego `title` i ani jednego zdania obok. Nie dało się z tabeli zobaczyć, CZEGO ten wiersz
 * właściwie żąda — `task`, `expect`, `command` i `proof` leżą w modelu od początku i nie miały
 * drogi na ekran. Formalnie niezmiennik 16 nie był złamany, bo przycisku bez skutku tam nie było;
 * była gorsza rzecz — treść bez przycisku.
 *
 * ZNAK `·` NIE MIAŁ LEGENDY. Miał `aria-label="not measured"` i nic poza tym, więc dla oka był
 * kropką. Trzy kropki obok trzech krzyżyków czyta się jako „nic tam nie ma", a znaczą „nikt tego
 * nie zmierzył" — i to jest ta sama różnica, o którą rozbija się liczba w nagłówku.
 *
 * KAŻDA KONTROLKA MA POWIEDZIEĆ, CO ZROBI, ZANIM SIĘ JĄ NACIŚNIE. `Run`, `Write cases` i `Stop`
 * stały jako trzy gołe czasowniki; jedyne zdanie o którymkolwiek z nich pojawiało się wtedy, gdy
 * naciśnięcie było NIEMOŻLIWE.
 *
 * SŁABA WERSJA: `expect(markup).toContain('npm test')`. Przechodzi ją ekran, który wypisuje
 * komendę gdziekolwiek — także w liście kandydatek pod tabelą, gdzie stała zawsze. Dlatego
 * pytamy o zawartość TEGO wiersza, wyciętą po głębokości.
 */

const NEVER: LabIo = {
  list: () => Promise.reject(new Error('the screen under test never reads the disk')),
  board: () => Promise.reject(new Error('the screen under test never reads the disk')),
  create: () => Promise.reject(new Error('the screen under test never reads the disk')),
  remove: () => Promise.reject(new Error('the screen under test never reads the disk')),
  propose: () => Promise.reject(new Error('the screen under test never reads the disk')),
  proposeFix: () => Promise.reject(new Error('the screen under test never reads the disk')),
  applyFix: () => Promise.reject(new Error('the screen under test never reads the disk')),
  stopProposing: () => Promise.resolve(),
  decide: () => Promise.reject(new Error('the screen under test never reads the disk')),
  putCase: () => Promise.reject(new Error('the screen under test never reads the disk')),
  putVariant: () => Promise.reject(new Error('the screen under test never reads the disk')),
  dropVariant: () => Promise.reject(new Error('the screen under test never reads the disk')),
};

const BOARD: EvalBoard = {
  set: {
    revision: 'rev-1',
    set: {
      format: 1,
      id: 'review-rubric',
      name: 'Review rubric',
      subject: { kind: 'agent', id: 'a' },
      cases: [
        {
          id: 'one',
          name: 'Reads the guard',
          task: 'say which file resolves the tenant',
          expect: [{ field: 'file', contains: 'guard', describe: 'the file it read' }],
          command: 'npm run guard',
          proof: 'guard ok',
          status: 'in-use',
          because: 'src/guard.ts:14',
        },
      ],
      variants: [{ id: 'as-it-is', name: 'As it is', agent: 'a', overrides: {} }],
    },
  },
  runs: [
    {
      folder: '20260831-091412__abc',
      when: '2026-08-31 09:14',
      state: 'succeeded',
      passed: 0,
      judged: 0,
      costUsd: null,
      cells: [
        {
          case: 'one',
          variant: 'as-it-is',
          outcome: 'not-judged',
          said: 'This never started.',
          costUsd: null,
        },
      ],
    },
  ],
  movement: null,
  cannotRun: null,
};

function screen(): string {
  const store = createLabStore(NEVER, () => Promise.resolve(null));
  store.setState({
    sets: [BOARD.set.set],
    agents: [{ id: 'a', name: 'Forge' }],
    openId: BOARD.set.set.id,
    board: BOARD,
    busy: 'idle',
    said: null,
    fix: null,
  });
  return renderToStaticMarkup(<LabScreen store={store} />);
}

/** Treść oznaczonego elementu, wycięta po głębokości — nie leniwym wzorcem. */
function region(markup: string, marker: string): string {
  const open = new RegExp('<([a-z]+)[^>]*\\s' + marker + '\\b[^>]*>');
  const hit = open.exec(markup);
  if (hit === null) return '';
  const name = hit[1] ?? '';
  const from = hit.index + hit[0].length;
  const walk = new RegExp('<(/?)' + name + '\\b[^>]*>', 'g');
  walk.lastIndex = from;
  let depth = 1;
  let step = walk.exec(markup);
  while (step !== null) {
    depth += step[1] === '/' ? -1 : 1;
    if (depth === 0) return markup.slice(from, step.index);
    step = walk.exec(markup);
  }
  return markup.slice(from);
}

function words(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

const MARKUP = screen();

describe('a row of the table', () => {
  const row = region(MARKUP, 'data-lab-row');

  it('opens what the case actually asks for', () => {
    const said = words(row);
    expect(
      said,
      'the row is dead text. The work it orders lives in the file and has no way onto the ' +
        'screen, so the only readable thing about a row is its name.',
    ).toContain('say which file resolves the tenant');
    expect(said, 'nor is the command that decides it readable anywhere near it').toContain(
      'npm run guard',
    );
    expect(said, 'nor the text that has to show up in its output').toContain('guard ok');
    expect(
      said,
      'nor the place the case was drafted from, which is the whole of how it is judged',
    ).toContain('src/guard.ts:14');
  });

  it('is a thing a person can open, not a paragraph laid out flat', () => {
    expect(
      row,
      'the row spills every field of the case into the table at once. Six rows across three ' +
        'columns of full sentences is a wall, and the ceiling on how much text one view may ' +
        'carry is measured and may only fall.',
    ).toContain('<summary');
  });
});

describe('the marks in the cells', () => {
  it('are named on the screen, not only to a screen reader', () => {
    const legend = words(region(MARKUP, 'data-lab-legend'));
    expect(
      legend,
      'a dot in a cell is a dot. It means "nobody measured this" and says so only through a ' +
        'label the eye never reads, so the three of them read as three empty cells.',
    ).toContain('not measured');
    expect(legend, 'and the two marks beside it are as unnamed as the dot').toContain(
      'did not pass',
    );
    expect(legend).toContain('passed');
  });

  it('carry the reason of the cell they stand in', () => {
    expect(
      region(MARKUP, 'data-lab-cell'),
      'the sentence saying why this cell ended as it did is in the model and drawn nowhere. A ' +
        'cell nobody measured says nothing at all about why not.',
    ).toContain('This never started.');
  });
});

describe('the surface as a whole', () => {
  it('says what a person can do with it, in gestures', () => {
    expect(
      words(region(MARKUP, 'data-lab-gesture')),
      'a table of marks with no sentence under it leaves every one of its parts to be guessed at',
    ).toMatch(/click/i);
  });

  it('lets every control say what it will do before it is pressed', () => {
    for (const control of ['data-lab-run', 'data-lab-propose']) {
      const button = new RegExp('<button[^>]*' + control + '[^>]*>').exec(MARKUP)?.[0] ?? '';
      expect(button, 'the ' + control + ' control has to be on screen to be judged').not.toBe('');
      expect(
        /\btitle="[^"]{12,}"/.test(button),
        'the ' +
          control +
          ' control is one bare verb. What it will cost, what it will read and what it will ' +
          'change are all things a person finds out by pressing it.',
      ).toBe(true);
    }
  });
});
