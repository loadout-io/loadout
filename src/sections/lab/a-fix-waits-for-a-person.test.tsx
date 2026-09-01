import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { createLabStore } from '../../state/lab';
import type { EvalBoard, EvalFix, EvalSubject, LabIo } from './io';
import LabScreen from './index';

/* Poprawka nie stosuje się sama, a Apply niesie rewizję z chwili PROPOZYCJI.
 *
 * Instrukcja agenta jest tym, co on robi w każdym biegu, także poza Labem. Dwa zabezpieczenia
 * i oba muszą być prawdziwe: czyta ją człowiek, zanim wejdzie, i wejdzie tylko wtedy, gdy
 * nikt w międzyczasie nie ruszył tej definicji.
 *
 * SŁABA WERSJA: asercja, że karta się renderuje. Przechodzi ją ekran, który przy Apply wysyła
 * `null` zamiast rewizji — czyli zapis, który cofa cudzą, nowszą zmianę bez jednego zdania.
 */

const FIX: EvalFix = {
  agent: '0198a1f2-3b4c-7d5e-8f60-112233445566',
  name: 'Reviewer',
  because: 'It guessed a path instead of opening the file.',
  instructions: 'Open the file before you name it.',
  insteadOf: 'Answer in two sentences.',
  revision: 'rev-at-proposal',
};

function aBoard(subject: EvalSubject): EvalBoard {
  return {
    set: {
      revision: 'set-rev',
      set: {
        format: 1,
        id: 'review-rubric',
        name: 'Review rubric',
        subject,
        cases: [
          {
            id: 'one',
            name: 'Reads the guard',
            task: 'say which file resolves the tenant',
            expect: [],
            command: '',
            proof: '',
            status: 'in-use',
            because: 'src/guard.ts:14',
          },
        ],
        variants: [{ id: 'without', name: 'Without', agent: 'a', overrides: {} }],
      },
    },
    runs: [
      {
        folder: '20260831-091412__abc',
        when: '2026-08-31 09:14',
        state: 'succeeded',
        passed: 0,
        judged: 1,
        costUsd: 0.4,
        cells: [
          {
            case: 'one',
            variant: 'without',
            outcome: 'did-not-pass',
            said: '"file" came back as "src/router.ts".',
            costUsd: 0.4,
          },
        ],
      },
    ],
    movement: null,
    cannotRun: null,
  };
}

interface Applied {
  readonly agent: string;
  readonly instructions: string;
  readonly revision: string | null;
}

function spying(applied: Applied[]): LabIo {
  const refuse = (): Promise<never> => Promise.reject(new Error('not part of this criterion'));
  return {
    list: () => Promise.resolve([]),
    board: () => Promise.resolve(aBoard({ kind: 'agent', id: 'a' })),
    create: refuse,
    remove: () => Promise.resolve(),
    propose: refuse,
    proposeFix: () => Promise.resolve(FIX),
    applyFix: (agent, instructions, revision) => {
      applied.push({ agent, instructions, revision });
      return Promise.resolve('rev-after');
    },
    stopProposing: () => Promise.resolve(),
    decide: refuse,
    putCase: refuse,
    putVariant: refuse,
    dropVariant: refuse,
  };
}

function screen(subject: EvalSubject, fix: EvalFix | null): string {
  const store = createLabStore(spying([]), () => Promise.resolve(null));
  const board = aBoard(subject);
  store.setState({
    sets: [board.set.set],
    agents: [{ id: 'a', name: 'Forge' }],
    openId: board.set.set.id,
    board,
    busy: 'idle',
    said: null,
    fix,
  });
  return renderToStaticMarkup(<LabScreen store={store} />);
}

describe('a fix', () => {
  it('offers to ask for one only where it could actually be applied', () => {
    expect(screen({ kind: 'agent', id: 'a' }, null)).toContain('data-lab-ask-fix');
    const forASkill = screen({ kind: 'skill', name: 'review-rubric' }, null);
    expect(
      forASkill.includes('data-lab-ask-fix'),
      'a change to a skill has to go through the same scan as one pasted from a link, and a ' +
        'button that refuses after the press is a control that lies',
    ).toBe(false);
    expect(
      forASkill,
      'and the screen has to say where that change is written instead, or the person is left ' +
        'with a list of failures and nowhere to go',
    ).toContain('written over in Skills');
  });

  it('shows what it fixes above the text it would save', () => {
    const markup = screen({ kind: 'agent', id: 'a' }, FIX);
    expect(markup).toContain('data-lab-fix');
    const reason = markup.indexOf('It guessed a path');
    const text = markup.indexOf('Open the file before you name it');
    expect(reason).toBeGreaterThan(-1);
    expect(
      reason < text,
      'a wall of text with no sentence about what it fixes is accepted or refused without being ' +
        'read, and this press changes how that agent behaves in every future run',
    ).toBe(true);
    expect(markup).toContain('data-lab-apply-fix');
    expect(markup).toContain('data-lab-drop-fix');
  });

  it('sends the revision it read, so a change underneath is refused instead of undone', async () => {
    const applied: Applied[] = [];
    const store = createLabStore(spying(applied), () => Promise.resolve(null));
    store.setState({
      openId: 'review-rubric',
      board: aBoard({ kind: 'agent', id: 'a' }),
      fix: FIX,
    });

    await store.getState().applyFix();

    expect(applied).toEqual([
      {
        agent: FIX.agent,
        instructions: FIX.instructions,
        revision: 'rev-at-proposal',
      },
    ]);
    expect(
      store.getState().fix,
      'the card comes down only after the write went through; taken down earlier it carries off ' +
        'a text there is nowhere left to read',
    ).toBe(null);
    expect(store.getState().said ?? '').toContain('Run the set again');
  });

  it('keeps the text on screen when the write did not go through', async () => {
    const io = spying([]);
    const store = createLabStore(
      { ...io, applyFix: () => Promise.reject(new Error('that agent changed on disk')) },
      () => Promise.resolve(null),
    );
    store.setState({
      openId: 'review-rubric',
      board: aBoard({ kind: 'agent', id: 'a' }),
      fix: FIX,
    });

    await store.getState().applyFix();

    expect(
      store.getState().fix,
      'a refusal that also throws away the proposal costs the person the turn they paid for',
    ).toEqual(FIX);
    expect(store.getState().said ?? '').toContain('changed on disk');
  });

  it('never applies anything on its own', async () => {
    const applied: Applied[] = [];
    const store = createLabStore(spying(applied), () => Promise.resolve(null));
    store.setState({ openId: 'review-rubric', board: aBoard({ kind: 'agent', id: 'a' }) });

    await store.getState().askForAFix('a');

    expect(store.getState().fix).toEqual(FIX);
    expect(
      applied,
      'a loop that rewrites an agent without a person in it changes how that agent behaves in ' +
        'the night, and nobody would know why',
    ).toEqual([]);
  });
});
