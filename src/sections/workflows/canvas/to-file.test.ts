/* Kryterium 7 dla T-13: zaznaczenie, najechanie i przesunięcie widoku nie zmieniają
 * zapisywanego pliku.
 *
 * To jest cicha porażka numer jeden tego ekranu. Zaznaczasz kafelek, plik się brudzi, autosave
 * zapisuje, historia dostaje wiersz, w którym nie ma żadnej twojej decyzji — a po roku nikt nie
 * umie odczytać historii workflow, bo każde spojrzenie na ekran zostawiło w niej ślad.
 * `NodeBase` w `@xyflow/system@0.0.80` niesie `selected`, `dragging` i `measured`,
 * a `toObject()` dokłada `viewport` [T3 §3.3].
 *
 * Słaba wersja tego kryterium to `expect(out.viewport).toBeUndefined()`. Przechodzi, kiedy
 * `selected` siedzi w `nodes[i].data` i wjeżdża do pliku razem z krokiem — bo `data` JEST
 * krokiem. Rozróżnia to rekurencyjny obchód po całym wyniku, szukający czterech nazw kluczy na
 * dowolnej głębokości, plus porównanie zapisanego tekstu przed i po zaznaczeniu.
 *
 * Trzeci przypadek jest tu po to, żeby dwa pierwsze nie dały się spełnić funkcją zwracającą
 * stałą: zmiana nazwy kroku MUSI zmienić zapisany tekst.
 */
import { describe, expect, it } from 'vitest';
import type { AgentStep, WorkflowFile } from '../../../state/workflows';
import type { CanvasEdge, CanvasNode } from './map';
import { toFile } from './map';

/** Trzy pierwsze siedzą w `NodeBase`, `viewport` dokłada `toObject()`, a `position` jest nazwą,
 * której plik nie zna — pozycja nazywa się w nim `at`. */
const COSMETIC = ['selected', 'dragging', 'measured', 'position', 'viewport'];

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

function build(): AgentStep {
  return {
    ...plan(),
    id: 's_build',
    name: 'Build',
    instructions: 'Fix the failing parser tests. Keep the public API unchanged.',
    at: { x: 240, y: 96 },
  };
}

function prev(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_ship_a_feature',
    name: 'Ship a feature',
    steps: [plan(), build()],
    links: [{ from: 's_plan', to: 's_build' }],
  };
}

/* Kafelki tak, jak oddaje je płótno: z całym bagażem, którego plik nie ma prawa zobaczyć. */
function nodes(): CanvasNode[] {
  return [
    {
      id: 's_plan',
      position: { x: 24, y: 24 },
      data: plan(),
      selected: false,
      dragging: false,
      measured: { width: 280, height: 96 },
    },
    {
      id: 's_build',
      position: { x: 240, y: 96 },
      data: build(),
      selected: false,
      dragging: false,
      measured: { width: 280, height: 112 },
    },
  ];
}

function edges(): CanvasEdge[] {
  return [{ id: 's_plan->s_build', source: 's_plan', target: 's_build' }];
}

/** Wszystkie nazwy kluczy w wyniku, na każdej głębokości. */
function keysDeep(value: unknown, into: string[] = []): string[] {
  if (Array.isArray(value)) {
    for (const item of value) keysDeep(item, into);
    return into;
  }
  if (value !== null && typeof value === 'object') {
    for (const [key, inner] of Object.entries(value)) {
      into.push(key);
      keysDeep(inner, into);
    }
  }
  return into;
}

function agentStep(doc: WorkflowFile, id: string): AgentStep {
  const hit = doc.steps.find((one) => one.id === id);
  if (hit === undefined || hit.kind !== 'agent') {
    throw new Error('the file no longer holds an agent step called ' + id);
  }
  return hit;
}

describe('what the canvas knows about itself never reaches the file', () => {
  it('writes the step and where it stands, and none of the canvas bookkeeping', () => {
    const out = toFile(prev(), nodes(), edges());

    const step = agentStep(out, 's_build');
    expect(step.name, 'the step itself survives the trip').toBe('Build');
    expect(step.at, 'and so does where it stands, under the name the file uses').toEqual({
      x: 240,
      y: 96,
    });
    expect(out.links, 'the arrows come from the edges, one for one').toEqual([
      { from: 's_plan', to: 's_build' },
    ]);

    const keys = keysDeep(out);
    for (const key of COSMETIC) {
      expect(
        keys,
        'no "' +
          key +
          '" anywhere in the file, at any depth. The step IS the tile data, so this one hides ' +
          'inside the step rather than next to it',
      ).not.toContain(key);
    }
  });

  it('writes the same bytes whether or not a tile happens to be selected', () => {
    const quiet = JSON.stringify(toFile(prev(), nodes(), edges()));

    const looked = nodes().map((one) =>
      one.id === 's_build'
        ? { ...one, selected: true, dragging: true, measured: { width: 281, height: 113 } }
        : one,
    );

    expect(
      JSON.stringify(toFile(prev(), looked, edges())),
      'clicking a tile is not a decision. If it changes the file, autosave writes it, and the ' +
        'history of this workflow fills up with lines nobody made',
    ).toBe(quiet);
  });

  it('does change when the step is renamed, or a function returning a constant would pass', () => {
    const quiet = JSON.stringify(toFile(prev(), nodes(), edges()));

    const renamed = nodes().map((one) =>
      one.id === 's_build' ? { ...one, data: { ...build(), name: 'Build it properly' } } : one,
    );

    expect(
      JSON.stringify(toFile(prev(), renamed, edges())),
      'this is a real decision and it has to reach the file. Without this line the two checks ' +
        'above are satisfied by a mapper that writes the same thing every time',
    ).not.toBe(quiet);
  });
});
