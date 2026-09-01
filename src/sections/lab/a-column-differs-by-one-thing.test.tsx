import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { createLabStore } from '../../state/lab';
import { isDirty, modelOf, nextColumn, typedOf, withModel, withName, withTyped } from './columns';
import type { EvalBoard, EvalVariant, LabIo } from './io';
import LabScreen from './index';

/* Kolumna różni się od sąsiada JEDNĄ rzeczą, a tabela da się złożyć z ekranu.
 *
 * Bez tego zestaw ma tyle kolumn, ile dostał przy zakładaniu, a pytanie „czy inny model jest
 * lepszy" nie ma jak zostać zadane. Store miał na to metody, zanim ekran miał przycisk — czyli
 * dokładnie ten szew, przed którym stoi niezmiennik 16, tylko o warstwę głębiej.
 *
 * SŁABA WERSJA: asercja, że przycisk jest w markupie. Przechodzi ją ekran, którego „Add column"
 * dokłada kolumnę identyczną z sąsiadem — czyli dwa razy ten sam rachunek za tę samą odpowiedź.
 */

const ONE: EvalVariant = { id: 'without', name: 'Without', agent: 'a', overrides: {} };

function aBoard(variants: readonly EvalVariant[]): EvalBoard {
  return {
    set: {
      revision: 'set-rev',
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
            expect: [],
            command: '',
            proof: '',
            status: 'in-use',
            because: 'src/guard.ts:14',
          },
        ],
        variants: [...variants],
      },
    },
    runs: [],
    movement: null,
    cannotRun: null,
  };
}

function spying(saved: EvalVariant[], dropped: string[]): LabIo {
  const refuse = (): Promise<never> => Promise.reject(new Error('not part of this criterion'));
  return {
    list: () => Promise.resolve([]),
    board: () => Promise.resolve(aBoard([ONE])),
    create: refuse,
    remove: () => Promise.resolve(),
    propose: refuse,
    proposeFix: refuse,
    applyFix: refuse,
    stopProposing: () => Promise.resolve(),
    decide: refuse,
    putCase: refuse,
    putVariant: (_folder, _set, variant) => {
      saved.push(variant);
      return Promise.resolve(aBoard([ONE]).set);
    },
    dropVariant: (_folder, _set, id) => {
      dropped.push(id);
      return Promise.resolve(aBoard([ONE]).set);
    },
  };
}

function screen(variants: readonly EvalVariant[]): string {
  const store = createLabStore(spying([], []), () => Promise.resolve(null));
  const board = aBoard(variants);
  store.setState({
    sets: [board.set.set],
    agents: [{ id: 'a', name: 'Forge' }],
    openId: board.set.set.id,
    board,
    busy: 'idle',
    said: null,
    fix: null,
  });
  return renderToStaticMarkup(<LabScreen store={store} />);
}

describe('a column', () => {
  it('can be added, named and pointed at another model from the screen', () => {
    const markup = screen([ONE]);
    expect(markup).toContain('data-lab-columns');
    expect(markup).toContain('data-lab-add-column');
    expect(markup).toContain('data-lab-column-name="without"');
    expect(markup).toContain('data-lab-column-model="without"');
  });

  it('will not offer to remove the only column there is', () => {
    expect(
      screen([ONE]).includes('data-lab-drop-column'),
      'a set with no columns has nothing to run, and a control that refuses after the press is ' +
        'one that lies',
    ).toBe(false);
    expect(screen([ONE, { ...ONE, id: 'with', name: 'With' }])).toContain(
      'data-lab-drop-column="without"',
    );
  });

  it('starts a new column from the same agent and with nothing changed yet', () => {
    const made = nextColumn([ONE], 'fallback');
    expect(
      made?.agent,
      'which agent does the work was already answered when the set was made',
    ).toBe('a');
    expect(
      made?.overrides,
      'copying the neighbour patch gives two identical columns, which is the same bill twice ' +
        'for the same answer',
    ).toEqual({});
    expect(made?.id).toBe('column-2');
  });

  it('never hands a new column an address another one already holds', () => {
    const taken = [ONE, { ...ONE, id: 'column-2', name: 'Column 2' }];
    const made = nextColumn(taken, 'a');
    expect(
      made?.id,
      'an id that comes back after a removal makes the new column read the results of the old one',
    ).toBe('column-3');
  });

  it('turns an empty model back into the one the agent already has', () => {
    const pointed = withModel(ONE, 'opus');
    expect(modelOf(pointed)).toBe('opus');
    expect(
      withModel(pointed, '   ').overrides,
      'an empty model is a refusal to start on the far side, not a default',
    ).toEqual({});
  });

  it('keeps the old name rather than leaving a column with none', () => {
    expect(withName(ONE, '  ').name).toBe('Without');
    expect(withName(ONE, ' With ').name).toBe('With');
  });

  it('says out loud when a typed value has not reached the disk', () => {
    /* ZMIERZONE NA ZYWYM EKRANIE 2026-08-31. Pola byly niekontrolowane i zapisywaly sie
     * wylacznie przy `onBlur`, wiec wpisany model zyl w DOM i nigdzie indziej — a ekran nie
     * odroznial go niczym od zapisanego. Wlasciciel wpisal `Test`, zobaczyl go w polu i mial
     * wszelkie prawo sadzic, ze ustawil model; plik nie zmienil sie ani razu. */
    expect(isDirty(ONE, undefined), 'a row nobody touched has nothing to save').toBe(false);
    expect(
      isDirty(ONE, { name: ONE.name, model: 'opus' }),
      'a typed value the disk has not seen has to be tellable from a saved one',
    ).toBe(true);
    expect(
      isDirty(ONE, { name: ONE.name, model: modelOf(ONE) }),
      'typing a value back to what it already was is not an unsaved change',
    ).toBe(false);
  });

  it('shows the field what was typed, and the saved value until something is', () => {
    expect(typedOf(ONE, undefined)).toEqual({ name: 'Without', model: '' });
    expect(typedOf(ONE, { name: 'With', model: 'opus' })).toEqual({ name: 'With', model: 'opus' });
  });

  it('carries both halves of the row into the one thing that is saved', () => {
    expect(withTyped(ONE, { name: 'On opus', model: 'opus' })).toEqual({
      ...ONE,
      name: 'On opus',
      overrides: { model: 'opus' },
    });
  });

  it('offers Save only where there is something unsaved', () => {
    const clean = screen([ONE]);
    expect(
      clean.includes('data-lab-save-column'),
      'a Save button standing there with nothing to save is a control that spends its life ' +
        'with nothing to do',
    ).toBe(false);
  });

  it('sends the change to the disk with the revision it read', async () => {
    const saved: EvalVariant[] = [];
    const dropped: string[] = [];
    const store = createLabStore(spying(saved, dropped), () => Promise.resolve(null));
    store.setState({ openId: 'review-rubric', board: aBoard([ONE]) });

    await store.getState().putVariant(withModel(ONE, 'opus'));
    await store.getState().dropVariant('without');

    expect(saved).toEqual([{ ...ONE, overrides: { model: 'opus' } }]);
    expect(dropped).toEqual(['without']);
  });
});
