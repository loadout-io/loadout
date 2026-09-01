import { describe, expect, it } from 'vitest';

import { createLabStore, saidAboutCases } from '../../state/lab';
import type { EvalBoard, EvalSet, LabIo, OpenEvalSet, ProposedCases } from './io';

/* Kontrolki Labu robią to, co obiecują — sądzone przez SKUTEK, nie przez obecność.
 *
 * To repo nie ma jsdom, więc kliknięcia nie da się odpalić. Kryterium woła więc dokładnie to,
 * co woła przycisk (`store.getState().<czasownik>`), i pyta o dwie rzeczy naraz: czy krawędź
 * dostała to, co trzeba, i czy STAN po tym się zmienił. Sprawdzenie samej krawędzi przechodzi
 * dla ekranu, który po zapisie nie odświeża tabeli — a wtedy człowiek klika „Accept" i nic się
 * nie rusza.
 *
 * SŁABA WERSJA: `expect(io.decide).toHaveBeenCalled()`. Przechodzi ją implementacja wysyłająca
 * `keep: false` przy „Accept", bo nikt nie sprawdził, co pojechało.
 */

const A_SET: EvalSet = {
  format: 1,
  id: 'review-rubric',
  name: 'Review rubric',
  subject: { kind: 'agent', id: 'a' },
  cases: [
    {
      id: 'draft',
      name: 'Still a draft',
      task: 'say which file resolves the tenant',
      expect: [],
      command: '',
      proof: '',
      status: 'suggested',
      because: 'src/guard.ts:14',
    },
  ],
  variants: [{ id: 'without', name: 'Without', agent: 'a', overrides: {} }],
};

const OPEN: OpenEvalSet = { set: A_SET, revision: 'rev-1' };

function aBoard(cannotRun: string | null = null): EvalBoard {
  return { set: OPEN, runs: [], movement: null, cannotRun };
}

interface Seen {
  readonly edited: unknown[];
  readonly decided: unknown[];
  readonly proposed: unknown[];
  readonly launched: unknown[];
  readonly boards: number;
}

function spying(): { readonly io: LabIo; readonly seen: Seen } {
  const seen = {
    edited: [] as unknown[],
    decided: [] as unknown[],
    proposed: [] as unknown[],
    launched: [] as unknown[],
    boards: 0,
  };
  const io: LabIo = {
    list: () => Promise.resolve([A_SET]),
    board: (...args) => {
      seen.boards += 1;
      void args;
      return Promise.resolve(aBoard());
    },
    create: () => Promise.resolve(OPEN),
    remove: () => Promise.resolve(),
    propose: (...args) => {
      seen.proposed.push(args);
      const proposed: ProposedCases = {
        set: OPEN,
        written: 2,
        withoutAReason: 1,
        unfinished: 0,
      };
      return Promise.resolve(proposed);
    },
    proposeFix: () => Promise.reject(new Error('this screen never asks for a fix here')),
    applyFix: () => Promise.reject(new Error('this screen never applies one here')),
    stopProposing: () => Promise.resolve(),
    decide: (...args) => {
      seen.decided.push(args);
      return Promise.resolve(OPEN);
    },
    putCase: (...args) => {
      seen.edited.push(args);
      return Promise.resolve(OPEN);
    },
    putVariant: () => Promise.resolve(OPEN),
    dropVariant: () => Promise.resolve(OPEN),
  };
  return { io, seen };
}

describe('the controls on this screen', () => {
  it('turns Accept into the one change that lets a case measure', async () => {
    const { io, seen } = spying();
    const store = createLabStore(io, () => Promise.resolve(null));
    store.setState({ openId: 'review-rubric', board: aBoard() });

    await store.getState().decide('draft', true);

    expect(seen.decided, 'Accept has to reach the disk, or the case stays a suggestion').toEqual([
      [null, 'review-rubric', 'draft', true, 'rev-1'],
    ]);
    expect(
      seen.boards,
      'the table has to be read again afterwards; a screen that saves and does not refresh looks ' +
        'to a person exactly like a button that does nothing',
    ).toBe(1);
  });

  it('sends Discard as its own answer, not as Accept', async () => {
    const { io, seen } = spying();
    const store = createLabStore(io, () => Promise.resolve(null));
    store.setState({ openId: 'review-rubric', board: aBoard() });

    await store.getState().decide('draft', false);

    expect(seen.decided).toEqual([[null, 'review-rubric', 'draft', false, 'rev-1']]);
  });

  it('will not start a run it has already been told cannot start, and says so once', async () => {
    const { io } = spying();
    const launched: unknown[] = [];
    const store = createLabStore(io, (...args) => {
      launched.push(args);
      return Promise.resolve(null);
    });
    const why = 'This set has no columns yet, so there is nothing to compare.';
    store.setState({ openId: 'review-rubric', board: aBoard(why) });

    await store.getState().run();

    expect(
      launched,
      'a run with nothing in it is refused on the far side anyway — but only after the card is ' +
        'made and the bar has flashed a run that never was',
    ).toEqual([]);
    /* JEDEN FAKT, JEDNO MIEJSCE. Zdanie rysuje juz plansza; przepisane tu drugi raz staje
     * pod samym soba na ekranie. Zmierzone na zywym ekranie 2026-08-31. */
    expect(
      store.getState().said,
      'the reason is already on the board; a copy of it here shows the same sentence twice',
    ).toBe(null);
  });

  it('starts the run under the name a person gave the set', async () => {
    const { io } = spying();
    const launched: unknown[] = [];
    const store = createLabStore(io, (...args) => {
      launched.push(args);
      return Promise.resolve(null);
    });
    store.setState({ openId: 'review-rubric', board: aBoard() });

    await store.getState().run();

    expect(launched.length).toBe(1);
    const [id, , , name] = launched[0] as [string, number, string | null, string];
    expect(id).toBe('review-rubric');
    expect(name, 'the bar over the blocks says what is running, in the words a person chose').toBe(
      'Review rubric',
    );
  });

  it('lets a person mend the command on a suggestion before accepting it', async () => {
    const { io, seen } = spying();
    const store = createLabStore(io, () => Promise.resolve(null));
    store.setState({ openId: 'review-rubric', board: aBoard() });
    const mended = { ...A_SET.cases[0]!, command: 'npm test -- guard', proof: '0 failed' };

    await store.getState().putCase(mended);

    expect(
      seen.edited,
      'the model proposes a command that is often one switch away from the right one, and this ' +
        'is the moment a person fixes it — before it ever measures anything',
    ).toEqual([[null, 'review-rubric', mended, 'rev-1']]);
    expect(seen.boards, 'and the screen reads the set again afterwards').toBe(1);
  });

  it('says how many cases came back and how many were thrown away', async () => {
    const { io, seen } = spying();
    const store = createLabStore(io, () => Promise.resolve(null));
    store.setState({ openId: 'review-rubric', board: aBoard() });

    await store.getState().propose('a');

    expect(seen.proposed).toEqual([[null, 'review-rubric', 'a']]);
    expect(store.getState().said).toBe(saidAboutCases(2, 1));
    expect(
      saidAboutCases(2, 1),
      'a silent drop teaches a person that the number on screen is smaller than the work they ' +
        'paid for',
    ).toContain('thrown away');
  });
});
