/* Kryterium 3 dla T-14: usunięcie pyta, nazywa workflow po imieniu, a po anulowaniu nie
 * robi nic.
 *
 * Słaba wersja to `expect(io.remove).toHaveBeenCalled()` po potwierdzeniu. Przechodzi dla
 * implementacji, która kasuje plik ZANIM pokaże pytanie — pytanie staje się wtedy ozdobą,
 * a `Cancel` kłamie. Przechodzi też dla ekranu, na którym słowo `Delete` po prostu jest.
 * „Na ekranie jest słowo Delete" nie dowodzi ani tego, że coś się usuwa, ani tego, że
 * pytanie zadano PRZED usunięciem (niezmiennik 20).
 *
 * Rozróżniają to dwie asercje na ZERO wywołań: raz w stanie, w którym pytanie jest już
 * wyrenderowane, i drugi raz po `cancelDelete()`. Plus asercja na NAZWACH tego, co zostało,
 * a nie na długości listy — długość zgadza się także wtedy, gdy zniknął nie ten plik.
 *
 * Bez DOM-u: `renderToStaticMarkup` z `react-dom/server`. W repo nie ma `jsdom` ani
 * `@testing-library/react`, a `package.json` jest na liście `DENIED` w checks/quick-scope.sh,
 * więc dołożenie ich byłoby momentem na zatrzymanie się i zapytanie człowieka (AGENTS.md §7).
 * Komponent jest sterowany, więc żadne kryterium nie potrzebuje zdarzenia myszy: akcje woła
 * test, a render pokazuje, co z nich wynikło.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Step, WorkflowEntry, WorkflowFile, WorkflowListIo } from './store';
import { createWorkflowListStore } from './store';
import { WorkflowList } from './workflow-list';

interface Disk extends WorkflowListIo {
  files: Map<string, WorkflowFile>;
  /** Nazwy plików przekazane do usunięcia. Pusta tablica to twierdzenie, nie brak danych. */
  removed: string[];
}

function disk(seed: readonly WorkflowEntry[]): Disk {
  const files = new Map<string, WorkflowFile>();
  for (const entry of seed) {
    files.set(entry.path, structuredClone(entry.workflow));
  }
  const removed: string[] = [];
  let minted = 0;

  return {
    files,
    removed,
    list: () =>
      Promise.resolve(
        [...files].map(([path, workflow]) => ({ path, workflow: structuredClone(workflow) })),
      ),
    newId: () => {
      minted += 1;
      return Promise.resolve('wf-minted-' + String(minted));
    },
    write: (path, workflow) => {
      files.set(path, structuredClone(workflow));
      return Promise.resolve();
    },
    remove: (path) => {
      removed.push(path);
      files.delete(path);
      return Promise.resolve();
    },
  };
}

/* Krok agenta wypełniony do PEŁNEGO schematu pliku (2026-08-17: `list/store.ts` przestał
 * trzymać własne, węższe lustro i bierze typy z `src/state/workflows.ts`). Lista czyta z kroku
 * cztery pola, ale na dysku leży całość — fikstura ma wyglądać jak plik, a nie jak wycinek,
 * który akurat czyta ten ekran. */
function step(id: string, name: string, agent: string): Step {
  return {
    kind: 'agent',
    id,
    name,
    agent,
    overrides: {},
    copies: 1,
    instructions: '',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 0, y: 0 },
  };
}

function entry(path: string, id: string, name: string): WorkflowEntry {
  return {
    path,
    workflow: {
      format: 1,
      id,
      name,
      steps: [step('s_one', 'Do the thing', 'forge')],
      links: [],
    },
  };
}

/** Trzy pozycje, żeby „zostały dwie" dało się sprawdzić po nazwach, a nie po długości. */
function seed(): WorkflowEntry[] {
  return [
    entry('deep-research.json', 'wf-deep-research', 'Deep research'),
    entry('just-fix-it.json', 'wf-just-fix-it', 'Just fix it'),
    entry('ship-a-feature.json', 'wf-ship-a-feature', 'Ship a feature'),
  ];
}

type Store = ReturnType<typeof createWorkflowListStore>;

/** Ekran dokładnie w tym stanie, w którym stoi magazyn. Stan i akcje idą propsami. */
function screen(store: Store): string {
  const state = store.getState();
  return renderToStaticMarkup(
    <WorkflowList
      workflows={state.workflows}
      pendingDeleteId={state.pendingDeleteId}
      onOpen={() => undefined}
      actions={state}
    />,
  );
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** To, co czyta człowiek: bez znaczników, z rozwiniętymi encjami, bez nadmiarowych odstępów. */
function visibleText(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, '')
    .replaceAll('&quot;', '"')
    .replaceAll('&#x27;', "'")
    .replaceAll('&lt;', '<')
    .replaceAll('&gt;', '>')
    .replaceAll('&amp;', '&')
    .replace(/\s+/g, ' ')
    .trim();
}

function listedNames(store: Store): string[] {
  return store.getState().workflows.map((listed) => listed.workflow.name);
}

describe('deleting a workflow asks first, names it, and does nothing when cancelled', () => {
  it('asks, and touches not one file while the question is on screen', async () => {
    const io = disk(seed());
    const store = createWorkflowListStore(io);
    await store.getState().load();

    store.getState().requestDelete('wf-deep-research');

    expect(
      io.removed,
      'asking is not doing. An implementation that removes the file and then shows the ' +
        'question passes every "is the word Delete on screen" test and loses the file anyway',
    ).toEqual([]);

    const markup = screen(store);
    expect(
      occurrences(markup, 'data-confirm-delete'),
      'exactly one question on screen — one fact, one place (invariant 13)',
    ).toBe(1);
    expect(
      visibleText(markup),
      'the question names the workflow it is about and says what disappears. A question that ' +
        'says "Are you sure?" makes the user guess which of three files they are about to lose',
    ).toContain('Delete "Deep research"? The file goes away. Runs you already did stay.');
  });

  it('puts everything back when the question is cancelled', async () => {
    const io = disk(seed());
    const store = createWorkflowListStore(io);
    await store.getState().load();

    store.getState().requestDelete('wf-deep-research');
    store.getState().cancelDelete();

    expect(
      io.removed,
      'still not one file removed. This is the assertion that catches a Cancel which cancels ' +
        'nothing because the removal already happened',
    ).toEqual([]);
    expect(listedNames(store), 'and all three are still listed, by name').toEqual([
      'Deep research',
      'Just fix it',
      'Ship a feature',
    ]);
    expect(io.files.size, 'and all three are still on disk').toBe(3);

    const markup = screen(store);
    expect(occurrences(markup, 'data-confirm-delete'), 'and the question is gone').toBe(0);
    expect(
      visibleText(markup),
      'together with the sentence that came with it — a question that stays on screen after ' +
        'Cancel is a question the next click answers by accident',
    ).not.toContain('The file goes away');
  });

  it('removes that one file, and only that one, when the question is answered', async () => {
    const io = disk(seed());
    const store = createWorkflowListStore(io);
    await store.getState().load();

    store.getState().requestDelete('wf-deep-research');
    await store.getState().confirmDelete();

    expect(
      io.removed,
      'exactly one file goes, and it is the file of the workflow the question named. Removing ' +
        'the entry from the list without removing the file gives a state that comes back at ' +
        'the next start (invariant 4)',
    ).toEqual(['deep-research.json']);
    expect(
      listedNames(store),
      'and the two that were not asked about are still there, by name. Asserting the length ' +
        'alone passes when the wrong one disappeared',
    ).toEqual(['Just fix it', 'Ship a feature']);
    expect(
      store.getState().pendingDeleteId,
      'the question has been answered, so it stops being asked',
    ).toBe(null);
    expect(occurrences(screen(store), 'data-confirm-delete'), 'and leaves the screen').toBe(0);
  });

  it('counts the list rather than remembering a number', async () => {
    const io = disk(seed());
    const store = createWorkflowListStore(io);
    await store.getState().load();

    expect(visibleText(screen(store)), 'three files, three saved').toContain('3 saved');

    store.getState().requestDelete('wf-deep-research');
    await store.getState().confirmDelete();

    const after = visibleText(screen(store));
    expect(
      after,
      'the counter in the header is the length of the list. A separate number in state drifts ' +
        'at the first delete and nothing on screen looks wrong',
    ).toContain('2 saved');
    expect(after, 'and the old count is gone, not merely joined by a new one').not.toContain(
      '3 saved',
    );
  });
});
