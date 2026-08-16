/* Kryterium 1 dla T-13: upuszczenie połączenia na pustym płótnie tworzy PODŁĄCZONY krok,
 * a upuszczenie na kafelku nie tworzy nic.
 *
 * Słaba wersja tego kryterium to `expect(steps).toHaveLength(2)`. Przechodzi dla implementacji,
 * która tworzy kafelek i NIE tworzy strzałki — czyli dla tej, w której gest „utwórz i połącz
 * jednym ruchem" jest połową gestu i użytkownik musi domykać go ręcznie. Przechodzi też dla tej,
 * która robi kafelek-widmo przy KAŻDYM udanym połączeniu, bo `isValid: true` też liczy się jako
 * upuszczenie. Dlatego niżej stoi równość całej tablicy `links` z konkretną krotką oraz osobny
 * przypadek `isValid: true` z zerem nowych kroków.
 *
 * Czego ten plik NIE sprawdza i dlaczego. Samego gestu — `pointerdown` na uchwycie, ruch,
 * `pointerup` nad pustym płótnem — nie da się odtworzyć bez przeglądarki; nie da się nawet
 * w jsdom, którego w repo nie ma, i React Flow kieruje w tym miejscu do Playwrighta
 * [T3 §2.3, ryzyko 7]. Sprawdzamy więc funkcję, którą ten gest woła, ze stanem połączenia
 * podanym wprost. Procedura ręczna na czas oddania stoi w TASK.md.
 */
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { AgentStep, Step, WorkflowFile } from '../../../state/workflows';
import { onConnectEnd } from './connect';
import { StepTile } from './tile';

function plan(): AgentStep {
  return {
    kind: 'agent',
    id: 's_plan',
    name: 'Plan',
    agent: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    overrides: {},
    copies: 1,
    instructions: 'Turn the goal into steps and say what each one owns.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y: 24 },
  };
}

function file(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_ship_a_feature',
    name: 'Ship a feature',
    steps: [plan()],
    links: [],
  };
}

/** Kroki, których w wyjściowym dokumencie nie było. */
function added(before: WorkflowFile, after: WorkflowFile): Step[] {
  const known = new Set(before.steps.map((step) => step.id));
  return after.steps.filter((step) => !known.has(step.id));
}

function only(steps: Step[]): Step {
  const first = steps[0];
  if (steps.length !== 1 || first === undefined) {
    throw new Error('expected exactly one fresh step, got ' + String(steps.length));
  }
  return first;
}

function named(doc: WorkflowFile, id: string): Step {
  const hit = doc.steps.find((step) => step.id === id);
  if (hit === undefined) throw new Error('the document no longer holds ' + id);
  return hit;
}

/** Tekst bez znaczników i bez encji. React zapisuje apostrof jako `&#x27;`. */
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

function tileText(step: Step, doc: WorkflowFile): string {
  return plain(
    renderToStaticMarkup(createElement(StepTile, { step, steps: doc.steps, links: doc.links })),
  );
}

const DROP = { at: { x: 241.4, y: 95.2 } };

describe('letting go of an arrow over empty canvas builds the step and wires it in one go', () => {
  it('adds exactly one step, on the grid, with exactly one arrow into it', () => {
    const before = file();
    const after = onConnectEnd(DROP, { isValid: false, fromNode: { id: 's_plan' } }, before);

    expect(
      after.steps,
      'one tile was there, one was dropped, so the document holds two',
    ).toHaveLength(2);

    const fresh = only(added(before, after));
    expect(fresh.kind, 'a dropped arrow makes a step, never a checkpoint').toBe('agent');
    expect(
      fresh.at,
      'the drop point lands on the grid on the way in, not on the way out. A position that is ' +
        'snapped only when the file is written shows a changed line on every save',
    ).toEqual({ x: 240, y: 96 });

    expect(
      after.links,
      'this is the whole gesture: create AND connect. A tile with no arrow into it is half of ' +
        'the gesture, and the user has to finish it by hand every single time',
    ).toEqual([{ from: 's_plan', to: fresh.id }]);

    expect(
      new Set(after.steps.map((step) => step.id)).size,
      'the fresh id may not collide with one already in the file. Two steps under one id means ' +
        'every arrow pointing there means two things at once',
    ).toBe(after.steps.length);
  });

  it('adds nothing at all when the arrow lands on a tile that is already there', () => {
    const before = file();
    const after = onConnectEnd(DROP, { isValid: true, fromNode: { id: 's_plan' } }, before);

    expect(
      after.steps,
      'a valid end means the pointer was over an existing tile, and that arrow is drawn by ' +
        'onConnect. Making a step here too leaves a ghost tile behind every successful wiring',
    ).toEqual(before.steps);
    expect(after.links, 'and the arrow itself is not this function to draw either').toEqual(
      before.links,
    );
  });

  it('leaves the document it was handed exactly as it found it', () => {
    const before = file();
    const untouched = structuredClone(before);

    onConnectEnd(DROP, { isValid: false, fromNode: { id: 's_plan' } }, before);

    expect(
      before,
      'the mapper hands back a new document; pushing into the one it was given is how undo ' +
        'and autosave end up disagreeing about what the file says',
    ).toEqual(untouched);
  });

  it('reads the tile footers off the arrows, so the fresh tile says it waits for Plan', () => {
    const before = file();
    expect(
      tileText(named(before, 's_plan'), before),
      'nothing points at Plan yet, so its footer says so. Without this line the two checks ' +
        'below would also pass for a footer written into the component by hand',
    ).toContain('first step');

    const after = onConnectEnd(DROP, { isValid: false, fromNode: { id: 's_plan' } }, before);
    const fresh = only(added(before, after));

    expect(
      tileText(fresh, after),
      'the footer is computed from links, and there is now exactly one arrow into this tile',
    ).toContain('after Plan');
    expect(
      tileText(fresh, after),
      'and it is no longer the first step, because something points at it now',
    ).not.toContain('first step');
    expect(
      tileText(named(after, 's_plan'), after),
      'Plan now hands work on, and the right half of its footer says so',
    ).toContain('runs before ▸');
  });
});
