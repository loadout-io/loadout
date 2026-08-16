/* Kryterium 2 dla T-14: duplikat jest osobnym plikiem, a nie drugą nazwą tego samego obiektu.
 *
 * Słaba wersja to `expect(list).toHaveLength(2)`. Przechodzi dla `list.push({ ...wf, id: newId })`
 * — kopia płytka, wspólna tablica `steps`, wspólna tablica `links`. Pierwsza edycja duplikatu
 * po cichu przepisuje wtedy oryginał, na którym użytkownik pracuje od miesiąca, a widać to
 * dopiero po biegu, który zrobił coś innego, niż mówi ekran.
 *
 * Rozróżnia to mutacja kopii z asercją na ORYGINALE: po `steps[0].name = …` krok oryginału ma
 * dalej swoją nazwę, a po `links.push(…)` `links` oryginału ma dalej swoją długość. Sama
 * różnica identyfikatorów tego nie łapie — dwa różne `id` da się nadać dwóm odwołaniom do
 * jednego obiektu.
 *
 * Atrapa dysku jest przepisana w tym pliku od zera, tak jak w pozostałych czterech
 * kryteriach: wspólny plik pomocniczy byłby jednym miejscem, w którym da się osłabić naraz
 * wszystkie pięć.
 */
import { describe, expect, it } from 'vitest';
import type { Step, WorkflowEntry, WorkflowFile, WorkflowListIo } from './store';
import { createWorkflowListStore } from './store';

interface Disk extends WorkflowListIo {
  files: Map<string, WorkflowFile>;
  writes: string[];
}

function disk(seed: readonly WorkflowEntry[]): Disk {
  const files = new Map<string, WorkflowFile>();
  for (const entry of seed) {
    files.set(entry.path, structuredClone(entry.workflow));
  }
  const writes: string[] = [];
  let minted = 0;

  return {
    files,
    writes,
    list: () =>
      Promise.resolve(
        [...files].map(([path, workflow]) => ({ path, workflow: structuredClone(workflow) })),
      ),
    newId: () => {
      minted += 1;
      return Promise.resolve('wf-minted-' + String(minted));
    },
    write: (path, workflow) => {
      writes.push(path);
      files.set(path, structuredClone(workflow));
      return Promise.resolve();
    },
    remove: (path) => {
      files.delete(path);
      return Promise.resolve();
    },
  };
}

/** Dwa kroki i jedna strzałka — tyle, ile trzeba, żeby płytka kopia miała co współdzielić. */
function deepResearch(): WorkflowEntry {
  return {
    path: 'deep-research.json',
    workflow: {
      format: 1,
      id: 'wf-deep-research',
      name: 'Deep research',
      description: 'Six readers on six questions, then one writer folds them into one document.',
      steps: [
        { kind: 'agent', id: 's_read', name: 'Read the sources', agent: 'scout' },
        { kind: 'agent', id: 's_write', name: 'Write it up', agent: 'scribe' },
      ],
      links: [{ from: 's_read', to: 's_write' }],
    },
  };
}

function named(entries: readonly WorkflowEntry[], name: string): WorkflowEntry {
  const hit = entries.find((entry) => entry.workflow.name === name);
  if (hit === undefined) {
    throw new Error('the list no longer holds the workflow this test put in it: ' + name);
  }
  return hit;
}

function firstStep(workflow: WorkflowFile): Step {
  const step = workflow.steps[0];
  if (step === undefined) {
    throw new Error('this workflow lost its first step, so there is nothing to compare');
  }
  return step;
}

describe('duplicating a workflow makes a second file, not a second name for the first', () => {
  it('gives the copy its own id, its own name and its own file', async () => {
    const io = disk([deepResearch()]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    await store.getState().duplicate('wf-deep-research');

    const entries = store.getState().workflows;
    expect(entries, 'duplicating turns one workflow into two').toHaveLength(2);

    const original = named(entries, 'Deep research');
    const copy = named(entries, 'Deep research (copy)');

    expect(
      copy.workflow.id,
      'the copy needs an id of its own. Two entries sharing one id are one workflow listed ' +
        'twice, and whichever was written last wins',
    ).not.toBe(original.workflow.id);
    expect(
      original.workflow.id,
      'and the original keeps the id it had — it is stable and never changes [T3 §3.1]',
    ).toBe('wf-deep-research');

    expect(
      copy.path,
      'the copy is a file of its own, so it cannot share the path of the file it was copied from',
    ).not.toBe(original.path);
    expect(copy.path, 'and its file name is a name a file system can carry').toMatch(
      /^[a-z0-9][a-z0-9-]*\.json$/,
    );
    expect(
      io.writes,
      'exactly one write, and it is the copy. Rewriting the original on a duplicate is how a ' +
        'file nobody edited gets a new modification time and a new chance to be wrong',
    ).toEqual([copy.path]);
  });

  it('keeps the step ids inside the copy, because they are local to the file', async () => {
    const io = disk([deepResearch()]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    await store.getState().duplicate('wf-deep-research');

    const copy = named(store.getState().workflows, 'Deep research (copy)');
    expect(
      copy.workflow.steps.map((step) => step.id),
      'step ids are local to the file, and the links point at them by those ids. Minting new ' +
        'step ids on a copy without rewriting the links leaves the copy wired to nothing',
    ).toEqual(['s_read', 's_write']);
    expect(
      copy.workflow.links,
      'and the wiring comes along, still pointing at those same steps',
    ).toEqual([{ from: 's_read', to: 's_write' }]);
  });

  it('copies the steps deeply, so renaming one in the copy cannot reach the original', async () => {
    const io = disk([deepResearch()]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    await store.getState().duplicate('wf-deep-research');

    firstStep(named(store.getState().workflows, 'Deep research (copy)').workflow).name = 'Renamed';

    expect(
      firstStep(named(store.getState().workflows, 'Deep research').workflow).name,
      'renaming a step in the copy must not rename it in the original. A shallow copy shares ' +
        'the steps array, so the first edit of the duplicate quietly rewrites the workflow the ' +
        'user has been running for a month',
    ).toBe('Read the sources');
  });

  it('copies the links deeply too, so wiring the copy cannot reach the original', async () => {
    const io = disk([deepResearch()]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    await store.getState().duplicate('wf-deep-research');

    const before = named(store.getState().workflows, 'Deep research').workflow.links.length;
    named(store.getState().workflows, 'Deep research (copy)').workflow.links.push({
      from: 's_write',
      to: 's_read',
    });

    expect(
      named(store.getState().workflows, 'Deep research').workflow.links.length,
      'drawing an arrow on the copy must not draw one on the original. Spreading the object ' +
        'copies the reference to the links array, not the array',
    ).toBe(before);
  });

  it('writes the copy out, because a duplicate that lives on screen is gone at the next start', async () => {
    const io = disk([deepResearch()]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    await store.getState().duplicate('wf-deep-research');

    const copy = named(store.getState().workflows, 'Deep research (copy)');
    const written = io.files.get(copy.path);
    expect(
      written?.name,
      'the copy has to reach the disk under its own name. Files are the truth and the list is ' +
        'a view on the folder (invariant 4)',
    ).toBe('Deep research (copy)');
    expect(
      written?.steps.map((step) => step.name),
      'and it reaches the disk with its steps, not as an empty shell',
    ).toEqual(['Read the sources', 'Write it up']);
  });
});
