import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { LabBusy } from '../../state/lab';
import { createLabStore } from '../../state/lab';
import type { EvalBoard, LabIo } from './io';
import LabScreen from './index';

/* CZTERY STANY, KTÓRYCH TEN EKRAN NIE MIAŁ — i jeden, który KŁAMAŁ.
 *
 * ZARZUT WŁAŚCICIELA, słowo w słowo: „nie ma żadnych informacji jak klikniesz co się dzieje /
 * stanów". Zmierzone w kodzie, nie odczute:
 *
 *   CZYTANIE LISTY   `sets` startuje jako `[]`, a `index.tsx` rozgałęzia się po `length === 0` —
 *                    więc przez czas DWÓCH odczytów granicy człowiek z dwudziestoma agentami
 *                    czyta „Sets you build to test agents will be listed here." i „Save an agent
 *                    first, over in Agents." To jest awaria, która wygląda jak działanie, i ten
 *                    sam plik broni się przed nią w `src/ui/shell/what-you-have.ts` trzema
 *                    stanami. Lab tej obrony nie miał.
 *   PISANIE          pojawiał się sam `Stop`. Ani słowa o tym, KTO pisze i CO czyta.
 *   MIERZENIE        ZERO. Wszystko gasło i tyle. Naciśnięty `Run` nie zmieniał ani jednego
 *                    piksela, więc kliknięcie, po którym ekran milczy, czyta się jak kliknięcie,
 *                    które nie doszło — i drugie kliknięcie jest wtedy winą interfejsu.
 *   ZAPIS            to samo, o krok ciszej.
 *
 * SŁABA WERSJA KAŻDEGO Z NICH: `expect(markup).toContain('data-lab-running')`. Przechodzi ją
 * pusty `<div>` ze znacznikiem. Dlatego każdy punkt pyta o ZDANIE, które czyta człowiek, i o to,
 * czego na ekranie ma w tym stanie NIE być.
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

/** Zestaw, który da się uruchomić: trzy wiersze, dwie kolumny, zero przebiegów. */
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
          command: 'npm test',
          proof: '0 failing',
          status: 'in-use',
          because: 'src/guard.ts:14',
        },
        {
          id: 'two',
          name: 'Names the file',
          task: 'name the file the failure starts in',
          expect: [],
          command: '',
          proof: '',
          status: 'in-use',
          because: 'src/router.ts:8',
        },
        {
          id: 'three',
          name: 'Keeps the shape',
          task: 'leave the shape of the answer alone',
          expect: [],
          command: '',
          proof: '',
          status: 'in-use',
          because: 'src/shape.ts:2',
        },
      ],
      variants: [
        { id: 'without', name: 'Without', agent: 'a', overrides: {} },
        { id: 'with', name: 'With', agent: 'a', overrides: {} },
      ],
    },
  },
  runs: [],
  movement: null,
  cannotRun: null,
};

function screen(busy: LabBusy, board: EvalBoard | null = BOARD): string {
  const store = createLabStore(NEVER, () => Promise.resolve(null));
  store.setState({
    sets: board === null ? [] : [board.set.set],
    agents: [{ id: 'a', name: 'Forge' }],
    openId: board === null ? null : board.set.set.id,
    board,
    busy,
    said: null,
    fix: null,
  });
  return renderToStaticMarkup(<LabScreen store={store} />);
}

/** Tekst bez znaczników, ze ściśniętymi odstępami. */
function words(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
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

describe('while Loadout is still reading the sets of this project', () => {
  const markup = screen('loading', null);

  it('says it is reading, instead of reporting a lack nobody measured', () => {
    expect(
      words(region(markup, 'data-lab-loading')),
      'the screen has no state for "I do not know yet whether there are any sets here". A person ' +
        'with three sets and twenty agents reads, for the length of two reads across the ' +
        'boundary, that they have neither — and that is a failure which looks exactly like ' +
        'working.',
    ).toContain('Reading');
  });

  it('does not report an empty project it has not looked at yet', () => {
    const said = words(markup);
    expect(
      said.includes('Sets you build to test agents will be listed here.'),
      'the invitation for an empty project stands over a project nobody has read yet',
    ).toBe(false);
    expect(
      said.includes('Save an agent first, over in Agents.'),
      'the screen tells a person who has agents saved to go and save one',
    ).toBe(false);
  });
});

describe('while an agent is drafting cases', () => {
  const markup = screen('proposing');

  it('says who is writing and what they are reading', () => {
    const said = words(region(markup, 'data-lab-proposing'));
    expect(
      said,
      'the only thing this state drew was a Stop button. It never said who is working, so the ' +
        'one control on screen stops something a person cannot name.',
    ).toContain('Forge');
    expect(said, 'nor what the draft is made from, which is the whole of how it is judged').toMatch(
      /project/i,
    );
  });

  it('offers a way out that names what it will stop', () => {
    const stop = /<button[^>]*data-lab-stop[^>]*>/.exec(markup)?.[0] ?? '';
    expect(stop, 'a state with no way out is a state a person waits inside').not.toBe('');
    expect(
      /\btitle="[^"]{12,}"/.test(stop),
      'the one control this state offers is the bare word Stop. Two things are running in this ' +
        'application at once — the drafting and the measuring — and this button ends only the ' +
        'first of them, which a person can find out only by pressing it.',
    ).toBe(true);
  });
});

describe('while the set is being measured', () => {
  const markup = screen('running');

  it('says how much work is under way, counted off the set itself', () => {
    const said = words(region(markup, 'data-lab-running'));
    expect(
      said,
      'pressing Run changed nothing on screen at all: every control went quiet and the table ' +
        'froze. A press that answers with silence reads as a press that never landed, and the ' +
        'second press is then the fault of the screen.',
    ).not.toBe('');
    expect(
      said,
      'the state does not say how much there is to measure. Six cells is three cases across two ' +
        'columns, and both numbers are in the set already: ' +
        JSON.stringify(said),
    ).toContain('6');
    expect(said, 'nor how that six is arrived at').toContain('3');
    expect(said).toContain('2');
  });

  it('says where the work can be watched, because it does not happen here', () => {
    expect(
      words(region(markup, 'data-lab-running')),
      'the measuring goes out as an ordinary run and lands in Run, and this screen hands back ' +
        'not one line of it. Without a word about where it went, a person waits in front of a ' +
        'frozen table.',
    ).toContain('Run');
  });
});

describe('while a change is going to disk', () => {
  it('says so, rather than only greying the controls out', () => {
    expect(
      words(region(screen('saving'), 'data-lab-saving')),
      'a save shows itself only by disabling every control, which is the same picture as a ' +
        'screen that has stopped answering',
    ).toContain('Saving');
  });
});

describe('when nothing at all is happening', () => {
  const markup = screen('idle');

  it('gives the screen one thing that is bigger than everything else', () => {
    /* Bez bohatera hierarchia wychodzi jako szary prostokąt obok szarego prostokąta — to jest
     * zmierzona przyczyna, dla której właściciel odrzucił dwie poprzednie przebudowy tego
     * interfejsu, a nie kwestia gustu. Ten sam warunek stawia `run/first-open-is-a-door`. */
    const hero = region(markup, 'data-lab-hero');
    expect(
      hero,
      'the screen has no hero at all: every word on it is the size of every other',
    ).not.toBe('');
    expect(
      /<h1[^>]*\btext-display\b/.test(hero),
      'the title of this screen does not stand on the top step of the ladder, so nothing on it ' +
        'leads the eye',
    ).toBe(true);
    expect(
      words(hero),
      'the hero does not name the set a person is looking at, which is the only thing this ' +
        'screen is about',
    ).toContain('Review rubric');
  });

  it('draws none of the four working states', () => {
    for (const marker of [
      'data-lab-loading',
      'data-lab-proposing',
      'data-lab-running',
      'data-lab-saving',
    ]) {
      expect(
        markup.includes(marker),
        'the resting screen still draws ' + marker + ', so the mark says nothing about the state',
      ).toBe(false);
    }
  });
});
