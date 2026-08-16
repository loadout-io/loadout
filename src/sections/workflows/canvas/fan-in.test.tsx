/* Kryterium 8 dla T-13: krok czytający wiele wejść mówi o tym na kafelku.
 *
 * Synteza wyników jest jednym z pięciu zadań edytora (`DECISIONS-LOCKED.md` §D6), a wyraża się
 * liczbą strzałek wchodzących — więc musi być widoczna bez otwierania panelu.
 *
 * Słaba wersja tego kryterium to sam przypadek z trzema wejściami. Przechodzi w implementacji,
 * która ZAWSZE pisze „reads N handoffs", także dla jednego i dla zera — a wtedy pierwszy krok
 * każdego workflow kłamie o tym, na co czeka. Rozróżniają to trzy przypadki w jednym pliku plus
 * przejście 3 → 2 po usunięciu strzałki.
 *
 * To jest też miejsce, w którym mieszka niezmiennik 17: stopka jest LICZONA ze strzałek. Wpisana
 * na sztywno w komponent wygląda identycznie do chwili, w której ktoś przesunie jedną strzałkę,
 * a potem kłamie bez żadnego sygnału.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { AgentStep, Link, Step, WorkflowFile } from '../../../state/workflows';
import { StepTile } from './tile';

function step(id: string, name: string, y: number): AgentStep {
  return {
    kind: 'agent',
    id,
    name,
    agent: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    overrides: {},
    copies: 1,
    instructions: 'Do the part of the work this tile owns.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y },
  };
}

const STEPS: Step[] = [
  step('s_plan', 'Plan', 24),
  step('s_scout', 'Research', 168),
  step('s_check', 'Check', 168),
  step('s_merge', 'Write it up', 312),
];

const THREE: Link[] = [
  { from: 's_plan', to: 's_merge' },
  { from: 's_scout', to: 's_merge' },
  { from: 's_check', to: 's_merge' },
];

function file(links: Link[]): WorkflowFile {
  return {
    format: 1,
    id: 'wf_deep_research',
    name: 'Deep research',
    steps: STEPS,
    links,
  };
}

function named(doc: WorkflowFile, id: string): Step {
  const hit = doc.steps.find((one) => one.id === id);
  if (hit === undefined) throw new Error('the document no longer holds ' + id);
  return hit;
}

function plain(fragment: string): string {
  return fragment
    .replace(/<[^>]*>/g, ' ')
    .replace(/&#x27;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Tekst kafelka `Write it up` przy podanym zestawie strzałek. */
function merged(links: Link[]): string {
  const doc = file(links);
  return plain(
    renderToStaticMarkup(
      <StepTile step={named(doc, 's_merge')} steps={doc.steps} links={doc.links} />,
    ),
  );
}

describe('a step reading several handoffs says so on the tile', () => {
  it('says it goes first when nothing points at it', () => {
    const text = merged([]);

    expect(text, 'nothing runs before it, and that is what the footer says').toContain(
      'first step',
    );
    expect(text, 'it waits for nobody, so it names nobody').not.toContain('after');
    expect(
      text,
      'and it reads nothing handed over, so the count has no business being here',
    ).not.toContain('handoffs');
  });

  it('names the one step it waits for', () => {
    const text = merged([{ from: 's_plan', to: 's_merge' }]);

    expect(text, 'one arrow in, so the footer names the step by its name').toContain('after Plan');
    expect(text, 'and it is not the first step any more').not.toContain('first step');
    expect(
      text,
      'one handoff is not worth counting; naming it says strictly more than "reads 1 handoff"',
    ).not.toContain('handoffs');
  });

  it('gives the number, not the list, once three arrows come in', () => {
    const text = merged(THREE);

    expect(
      text,
      'three names would not fit in four lines, so the tile says how many. This is the one ' +
        'thing that makes synthesis visible without opening the panel',
    ).toContain('reads 3 handoffs');
    expect(text, 'it is not the first step').not.toContain('first step');
    expect(text, 'and it does not read out the list it just stopped listing').not.toContain(
      'after',
    );
  });

  it('follows the arrows down to two when one of them is removed', () => {
    expect(merged(THREE), 'three arrows in').toContain('reads 3 handoffs');
    expect(
      merged(THREE.slice(1)),
      'and two after one is deleted. A footer written into the component reads the same until ' +
        'somebody moves an arrow, and then it is simply wrong, silently',
    ).toContain('reads 2 handoffs');
  });
});
