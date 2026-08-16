/* Autosave: to, co widać na płótnie, ląduje w pliku bez naciskania czegokolwiek
 * [TASK.md „Co to zadanie posiada" — `src/state/workflows.ts`, T3 §9 „MVP ships" punkt 6].
 *
 * Ten plik nie należy do żadnego kryterium akceptacji i to jest właśnie powód, dla którego
 * istnieje. Autosave był wymieniony w zadaniu, nie miał kryterium, i przez to jego brak był
 * niewidoczny dla bramki — pełny bieg testów czyta ten plik i od teraz go widzi (niezmiennik 21:
 * artefakt, którego żaden skrypt nie czyta, nie ma prawa powstać; ten czyta `checks/full-test.sh`).
 *
 * Słaba wersja tego sprawdzenia to „po zmianie `io.save` zostało zawołane". Przechodzi dla
 * zapisu na KAŻDY commit — czyli dla jednego pliku i jednego przebiegu walidatora Rusta na każdą
 * literę wpisaną w nazwie kroku i na każdą klatkę przeciągania kafelka. Przechodzi też dla zapisu
 * PIERWSZEGO stanu z serii, czyli dla pliku, który jest o jedną zmianę do tyłu wobec ekranu.
 * Rozróżnia to seria commitów w oknie odliczania z asercją na LICZBIE zapisów i na TREŚCI
 * tego jedynego.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AgentStep, Note, Point, WorkflowFile } from '../../../state/workflows';
import { createWorkflowStore } from '../../../state/workflows';

/** Okno ciszy z `AUTOSAVE_MS`. Powtórzone tutaj celowo: gdyby test importował tę stałą,
 * przesunięcie jej na 40 s dalej dawałoby zieleń, a użytkownik czekałby 40 s na zapis. */
const WINDOW_MS = 400;

function plan(at: Point): AgentStep {
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
    at,
  };
}

function file(at: Point = { x: 24, y: 24 }): WorkflowFile {
  return {
    format: 1,
    id: 'wf_ship_a_feature',
    name: 'Ship a feature',
    steps: [plan(at)],
    links: [],
  };
}

interface Recorder {
  saved: WorkflowFile[];
  notes: Note[];
}

/** Atrapa `WorkflowIo`, która zapisuje MIGAWKĘ każdego zapisu — magazyn oddaje ten sam obiekt,
 * którym potem dalej żyje, więc trzymanie odwołania pokazywałoby stan z końca testu. */
function io(recorder: Recorder) {
  return {
    save: (target: WorkflowFile) => {
      recorder.saved.push(structuredClone(target));
      return Promise.resolve();
    },
    check: () => Promise.resolve(recorder.notes),
    saveAgent: () => Promise.resolve(),
  };
}

function positionOf(doc: WorkflowFile): Point {
  const hit = doc.steps.find((step) => step.id === 's_plan');
  if (hit === undefined) throw new Error('the document no longer holds s_plan');
  return hit.at;
}

/** Przesunięcie kafelka — najczęstszy commit na tym ekranie i ten, który leci seriami. */
function moved(doc: WorkflowFile, at: Point): WorkflowFile {
  return {
    ...doc,
    steps: doc.steps.map((step) => (step.id === 's_plan' ? { ...step, at } : step)),
  };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('what is on the canvas reaches the file without anybody pressing Save', () => {
  it('writes the document down a moment after the change, and writes down what is on screen', async () => {
    const recorder: Recorder = { saved: [], notes: [] };
    const store = createWorkflowStore(io(recorder), file());

    store.getState().commit(moved(store.getState().document, { x: 240, y: 96 }));

    expect(
      recorder.saved,
      'not on the same tick: a write per change is a file per keystroke, and a run of the ' +
        'Rust validator behind each one',
    ).toEqual([]);

    await vi.advanceTimersByTimeAsync(WINDOW_MS);

    expect(recorder.saved, 'once the typing stops, exactly one write').toHaveLength(1);
    expect(
      recorder.saved[0],
      'and it is the document the screen is showing. A file that is one edit behind the ' +
        'canvas is the failure this whole screen is built against — the file is the truth',
    ).toEqual(store.getState().document);
    expect(positionOf(recorder.saved[0] ?? file()), 'down to the position that moved').toEqual({
      x: 240,
      y: 96,
    });
  });

  it('turns a run of changes into one write, carrying the state the run ended on', async () => {
    const recorder: Recorder = { saved: [], notes: [] };
    const store = createWorkflowStore(io(recorder), file());

    /* Przeciągnięcie kafelka to seria commitów w odstępach krótszych niż okno ciszy. */
    for (const at of [
      { x: 48, y: 24 },
      { x: 120, y: 48 },
      { x: 240, y: 96 },
    ]) {
      store.getState().commit(moved(store.getState().document, at));
      await vi.advanceTimersByTimeAsync(WINDOW_MS / 4);
    }

    expect(
      recorder.saved,
      'while the run is still going nothing is written: the interesting state is the one it ' +
        'ends on, not the three it passed through',
    ).toEqual([]);

    await vi.advanceTimersByTimeAsync(WINDOW_MS);

    expect(
      recorder.saved,
      'three changes, one write. A write per change also passes an "io.save was called" ' +
        'assertion, and produces a file history nobody can read',
    ).toHaveLength(1);
    expect(
      positionOf(recorder.saved[0] ?? file()),
      'and the one write carries the LAST state, not the first. Saving the state the run ' +
        'started on leaves the file permanently one edit behind',
    ).toEqual({ x: 240, y: 96 });
  });

  it('refreshes the notes it saved against, and stays quiet while nothing changes', async () => {
    const recorder: Recorder = {
      saved: [],
      notes: [
        {
          level: 'warning',
          stepId: 's_plan',
          message: 'This step is not connected to anything, so it would never run.',
        },
      ],
    };
    const store = createWorkflowStore(io(recorder), file());

    await vi.advanceTimersByTimeAsync(WINDOW_MS * 4);
    expect(
      recorder.saved,
      'an untouched document is not written. A timer that fires on its own rewrites files ' +
        'the user only ever looked at',
    ).toEqual([]);

    store.getState().commit(moved(store.getState().document, { x: 240, y: 96 }));
    await vi.advanceTimersByTimeAsync(WINDOW_MS);

    expect(
      store.getState().notes,
      'autosave goes through saveNow, so the notes on screen belong to the file that is now ' +
        'on disk. A second, quieter road to disk would leave them describing the old one',
    ).toEqual(recorder.notes);
  });
});
