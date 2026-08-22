/* `/history` pyta o TEN folder, i tylko o niego — plus wszystkie zdania, którymi odpowiada.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(await openHistoryFromLine('')).toBe(null)`. Przechodzi
 * implementacja, która pyta Rusta bez klucza `folder` — czyli ta, która pokazuje historię
 * katalogu, pod którym wstała aplikacja, pod nazwą projektu, na który człowiek patrzy. To jest
 * dokładnie ten jeden warunek, który właściciel postawił dwa razy w jednym zdaniu („pamiętaj że
 * wszystko ma być per workspace ta historia"), i jedyne, co go dowozi, to KLUCZ W ŻĄDANIU.
 * Rozróżnia je odczytanie tego, co pojechało: ścieżka w żądaniu ma być ścieżką AKTYWNEGO
 * zakresu, a przy braku zakresu do Rusta nie ma pojechać nic.
 *
 * DRUGA SŁABA WERSJA: sprawdzić same zdania odmowy. Zdanie, które wróciło z funkcji, dowodzi, że
 * mechanizm istnieje; nie dowodzi, że ktokolwiek je zobaczył (niezmiennik 29). Tamtą połowę
 * mierzy `./past/history-reaches-the-screen.test.tsx`, na prawdziwym markupie ekranu pracy. Tutaj
 * mierzymy politykę — bo to repo nie ma jsdom i naciśnięcia Enter nie da się odpalić.
 *
 * Granica jest atrapą: żadnego żywego Tauri i żadnej przeglądarki. Kryterium, które ich wymaga,
 * nie umie być czerwone z właściwego powodu.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { PastRunRow } from './io';

const { invoked, answers } = vi.hoisted(() => {
  const answers = { rows: [] as unknown[], refuse: null as string | null };
  return {
    answers,
    invoked: vi.fn((command: string) => {
      if (answers.refuse !== null) return Promise.reject(answers.refuse);
      if (command === 'list_runs') return Promise.resolve(answers.rows);
      return Promise.resolve(undefined);
    }),
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const {
  COULD_NOT_READ,
  NO_FOLDER_TO_LOOK_IN,
  NOTHING_YET,
  costText,
  matching,
  nothingLikeThat,
  openHistoryFromLine,
  stateWord,
} = await import('./history-command');
const { closeHistory, pastNow } = await import('./past/store');
const { useWorkspaces } = await import('../../state/workspaces');

/** Zakres, w którym pracujemy. `id === folder` — kontrakt granicy z 2026-08-18. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };

/** Drugi zakres, otwarty i NIEaktywny. Jego ścieżka nie ma prawa pojechać do Rusta. */
const NEXT_DOOR = { id: '/Users/x/other-app', name: 'Other', folder: '/Users/x/other-app' };

function row(when: string, title: string, extra: Partial<PastRunRow> = {}): PastRunRow {
  return {
    folder: when.replace(/[-: ]/g, '') + '__0198a1f2',
    when,
    title,
    state: 'succeeded',
    steps: 2,
    costUsd: 1,
    said: null,
    ...extra,
  };
}

const SHIP = row('2026-08-16 19:48', 'Ship a feature');
const LOOK = row('2026-08-10 08:15', 'Look around', { state: 'cancelled', costUsd: null });
const TORN = row('2026-08-12 10:11', '', {
  state: '',
  steps: 0,
  costUsd: null,
  said: 'Loadout could not read the record of this one, so all it can say is when it ran.',
});

function sentFor(command: string): Record<string, unknown> | null {
  const call = invoked.mock.calls.find((one) => one.at(0) === command);
  return call === undefined ? null : ((call.at(1) ?? {}) as Record<string, unknown>);
}

beforeEach(() => {
  invoked.mockClear();
  answers.rows = [];
  answers.refuse = null;
  closeHistory();
  useWorkspaces.setState({ all: [HERE, NEXT_DOOR], activeId: HERE.id, said: null });
});

describe('/history asks about the folder a person is working in, and no other', () => {
  it('carries the active folder to Rust, never the one next door', async () => {
    answers.rows = [SHIP];
    const said = await openHistoryFromLine('');

    expect(
      said,
      'the list opened, so the line has nothing left to say — the answer is the panel that just ' +
        'stood up',
    ).toBe(null);

    const sent = sentFor('list_runs');
    expect(
      sent,
      'nothing was asked of Rust at all, so the folder this list belongs to came from nowhere',
    ).not.toBe(null);
    expect(
      sent?.['folder'],
      'the folder a person chose has to travel in the request. Runs live under the project ' +
        'folder, so a request without it reads whatever folder the app happened to start in — ' +
        'and shows it under the name of the project on screen. The other open folder is ' +
        NEXT_DOOR.folder +
        ', and it must never be the answer.',
    ).toBe(HERE.folder);
  });

  it('refuses before touching the disk when no folder is chosen', async () => {
    useWorkspaces.setState({ all: [], activeId: null, said: null });
    const said = await openHistoryFromLine('');

    expect(
      said,
      'with no folder chosen there is nothing whose history could be shown, and the refusal has ' +
        'to name the way out. Falling back to whatever folder the app started in would show one ' +
        "project's runs under another project's name.",
    ).toBe(NO_FOLDER_TO_LOOK_IN);
    expect(
      invoked.mock.calls.length,
      'and nothing may be asked of Rust, because there is nothing to ask about',
    ).toBe(0);
    expect(pastNow().open, 'and no panel may stand up over an answer that is a refusal').toBe(
      false,
    );
  });

  it('invites instead of reporting emptiness when this folder has run nothing', async () => {
    answers.rows = [];
    const said = await openHistoryFromLine('');

    expect(
      said,
      'an empty history in a fresh folder is a normal state, and the sentence has to name the ' +
        'next move rather than announce that a lookup found nothing (DESIGN section 6)',
    ).toBe(NOTHING_YET);
    expect(
      pastNow().open,
      'and an empty panel may not stand up over that sentence: a page showing nothing under a ' +
        'heading is worse than the sentence alone',
    ).toBe(false);
  });

  it('puts the rows it read into the panel, newest first, with the folder they came from', async () => {
    answers.rows = [SHIP, TORN, LOOK];
    await openHistoryFromLine('');

    expect(
      pastNow().rows.map((one) => one.title),
      'every row Rust handed over has to reach the panel, in the order it arrived — that order ' +
        'is newest first and it is the reason the first row is the thing that just happened',
    ).toEqual(['Ship a feature', '', 'Look around']);
    expect(
      pastNow().folder,
      'the panel has to remember which folder this list came from, because picking a row reads ' +
        'that folder again — and a person may switch the side menu between looking and picking',
    ).toBe(HERE.folder);
  });

  it('says what it could not read, in the words Rust used', async () => {
    answers.refuse = 'the folder is not readable';
    const said = await openHistoryFromLine('');

    expect(
      said,
      'a refusal from disk has to come back as a sentence a person can read. Silence after ' +
        'typing a command is indistinguishable from a command that was never accepted.',
    ).toContain('the folder is not readable');
    expect(
      said === null ? '' : said,
      'and it may not swallow the reason and answer with the fallback sentence alone: ' +
        COULD_NOT_READ,
    ).not.toBe(COULD_NOT_READ);
  });
});

describe('what a person typed after /history narrows the list', () => {
  it('takes everything when nothing was typed', () => {
    expect(
      matching([SHIP, LOOK], ''),
      '/history on its own is a question about the whole history, and that is what the prompt ' +
        'in the line promises',
    ).toEqual([SHIP, LOOK]);
  });

  it('matches on the words a person can actually see', () => {
    expect(
      matching([SHIP, LOOK], 'ship').map((one) => one.title),
      'the name a workflow gives itself is on the screen, so it is what a person will type. ' +
        'Case may not decide the answer.',
    ).toEqual(['Ship a feature']);
    expect(
      matching([SHIP, LOOK], '2026-08-10').map((one) => one.title),
      'the day is on the screen too, and it is the only thing a run with an unreadable record ' +
        'still shows',
    ).toEqual(['Look around']);
  });

  it('names what is here when nothing matches, instead of leaving a person guessing', async () => {
    answers.rows = [SHIP, LOOK];
    const said = await openHistoryFromLine('nowhere');

    expect(
      said,
      'a word that matches nothing has to come back with a sentence, not an empty panel',
    ).toBe(nothingLikeThat('nowhere', [SHIP, LOOK]));
    expect(
      said ?? '',
      'and that sentence has to list what did run here — a question about "which one" with no ' +
        'list is a riddle, because the list is built from files nobody can guess',
    ).toContain('Ship a feature');
    expect(pastNow().open, 'and no empty panel may stand up over it').toBe(false);
  });

  it('opens the panel on the narrowed list, not on all of it', async () => {
    answers.rows = [SHIP, LOOK];
    await openHistoryFromLine('look');

    expect(
      pastNow().rows.map((one) => one.title),
      'the word a person typed has to narrow what they are shown. A panel that ignores it looks ' +
        'exactly like one that never read it.',
    ).toEqual(['Look around']);
  });
});

describe('the words a person reads for a state and a cost', () => {
  it('turns every state the run file can hold into a word from the agents list', () => {
    const table: ReadonlyArray<readonly [string, string]> = [
      ['running', 'working'],
      ['paused', 'needs you'],
      ['succeeded', 'done'],
      ['failed', 'failed'],
      ['cancelled', 'stopped'],
    ];
    for (const [wire, word] of table) {
      expect(
        stateWord(wire),
        'the word written in the run file is for the engine, and the one on screen is for a ' +
          'person (invariant 14). These are the same six words the agents list already uses, so ' +
          'one screen never calls one state by two names.',
      ).toBe(word);
    }
    expect(
      stateWord('teleported'),
      'a state this window has never heard of may not reach the screen as itself. A newer ' +
        'Loadout can write one, and a raw word from the wire on screen is exactly what ' +
        'invariant 14 forbids.',
    ).toBe('');
  });

  it('keeps "nobody measured it" apart from "it cost nothing"', () => {
    expect(
      costText(null),
      'not one step said what it cost, and an amount printed there would be a number nobody ' +
        'measured (invariant 17)',
    ).toBe('');
    expect(costText(0), 'zero is a measurement and reads as one').toBe('$0.00');
    expect(costText(1.256), 'and money is written the way the loadout bar writes it').toBe('$1.26');
  });
});
