/* Paleta robi trzy rzeczy i mają one różną wagę — kolejność listy jest tym, co to wyraża.
 *
 * DLACZEGO TO JEST KRYTERIUM, A NIE GUST. Pierwsze naciśnięcie Enter po otwarciu palety trafia
 * w pozycję pierwszą. Lista posortowana po nazwie stawia tam to, co akurat zaczyna się na „A" —
 * czyli losowego agenta zamiast sekcji, po którą człowiek sięgnął. Zawężenie wpisanym słowem
 * NIE ma prawa tej hierarchii przestawić: to samo pytanie zadane z wpisanym słowem ma dostać
 * tę samą odpowiedź o ważności.
 */
import { describe, expect, it } from 'vitest';

import { SECTIONS } from '../sections';
import { keeps, keyOf, matching, paletteItems } from './items';
import type { Saved } from './items';

const WORKFLOWS: readonly Saved[] = [
  { id: 'ship-a-feature.json', label: 'Ship a feature' },
  { id: 'two-agents.json', label: 'Agents look at one another' },
];

const AGENTS: readonly Saved[] = [
  { id: 'agent-1', label: 'Reviewer' },
  { id: 'agent-2', label: 'Ship agent' },
];

describe('what the palette can do, in the order it matters', () => {
  it('puts every section first, in the order the one list of sections states', () => {
    const items = paletteItems(WORKFLOWS, AGENTS);
    expect(items.slice(0, SECTIONS.length).map((item) => item.label)).toEqual(
      SECTIONS.map((entry) => entry.label),
    );
    expect(items.slice(0, SECTIONS.length).every((item) => item.kind === 'section')).toBe(true);
  });

  it('puts saved workflows after the sections and saved agents after those', () => {
    const items = paletteItems(WORKFLOWS, AGENTS);
    expect(items.map((item) => item.kind).slice(SECTIONS.length)).toEqual([
      'workflow',
      'workflow',
      'agent',
      'agent',
    ]);
    expect(items.map((item) => item.label).slice(SECTIONS.length)).toEqual([
      'Ship a feature',
      'Agents look at one another',
      'Reviewer',
      'Ship agent',
    ]);
  });

  it('carries the letter of a section so the list can say how to get there without the mouse', () => {
    const run = paletteItems([], []).find((item) => item.label === 'Run');
    expect(run?.kind).toBe('section');
    expect(run !== undefined && run.kind === 'section' ? run.letter : null).toBe('R');
  });

  it('keeps that order after a word is typed', () => {
    const narrowed = matching(paletteItems(WORKFLOWS, AGENTS), 'agent');
    expect(narrowed.map((item) => [item.kind, item.label])).toEqual([
      ['section', 'Agents'],
      ['workflow', 'Agents look at one another'],
      ['agent', 'Ship agent'],
    ]);
  });

  it('shows everything before anybody types, and nothing that does not match', () => {
    const all = paletteItems(WORKFLOWS, AGENTS);
    /* Bez tej linii wszystko niżej jest prawdziwe o liście PUSTEJ — a pusta lista przechodzi
       każde „tyle samo co wszystko" i każde „nic nie pasuje". */
    expect(all).toHaveLength(SECTIONS.length + WORKFLOWS.length + AGENTS.length);
    expect(matching(all, '')).toHaveLength(all.length);
    expect(matching(all, '   ')).toHaveLength(all.length);
    expect(matching(all, 'nothing by that name')).toHaveLength(0);
  });

  it('matches a word from the middle of a name, not only its start', () => {
    expect(keeps('Ship a feature', 'feature')).toBe(true);
    expect(keeps('Ship a feature', 'SHIP')).toBe(true);
    expect(keeps('Ship a feature', 'zzz')).toBe(false);
  });

  it('tells a workflow and an agent apart even when they are named the same thing', () => {
    const items = paletteItems([{ id: 'same', label: 'Same' }], [{ id: 'same', label: 'Same' }]);
    const both = items.filter((item) => item.label === 'Same').map(keyOf);
    expect(both).toEqual(['workflow:same', 'agent:same']);
  });
});
