/* Kryterium 5 dla T-14: pusty ekran jest zaproszeniem, a jego przycisk naprawdę tworzy.
 *
 * Słaba wersja to `expect(html).toContain('No workflows yet')`. Przechodzi dla ekranu
 * z martwym przyciskiem — poprzedni prototyp ma trzy takie, „dead controls with no onClick"
 * (00-SYNTHESIS §6) — i przechodzi dla ekranu, na którym zaproszenie zostaje na widoku także
 * wtedy, gdy workflow już są.
 *
 * Rozróżniają to dwie rzeczy. Po pierwsze: `actions.create('Ship it')` wołane na TYM SAMYM
 * obiekcie, który dostał ekran, ma naprawdę zapisać plik przez atrapę dysku i dołożyć pozycję
 * do listy. Po drugie: render tego samego ekranu z jednym workflow, w którym zaproszenia
 * już nie ma.
 *
 * Czego ten plik NIE sprawdza i dlaczego. Że przycisk jest podpięty pod `actions.create`
 * atrybutem `onClick` — `renderToStaticMarkup` nie renderuje handlerów, a jsdomu w repo nie
 * ma i `package.json` jest na liście `DENIED` w checks/quick-scope.sh (AGENTS.md §7).
 * Zostaje więc: typ propsów wymaga `actions`, ekran w pustym stanie ma DOKŁADNIE JEDEN
 * przycisk, a funkcja, którą ten ekran dostał, jest sprawdzona wprost. Rozjazd, którego to
 * nie złapie — przycisk podpięty pod inną, drugą ścieżkę tworzenia — jest tym, przed czym
 * broni niezmiennik 16 w recenzji, i jest wypisany tutaj zamiast być przemilczany.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { WorkflowEntry, WorkflowFile, WorkflowListIo } from './store';
import { createWorkflowListStore } from './store';
import { WorkflowList } from './workflow-list';

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
        [...files].map(([path, workflow]) => ({
          path,
          place: 'project' as const,
          workflow: structuredClone(workflow),
        })),
      ),
    newId: () => {
      minted += 1;
      return Promise.resolve('wf-minted-' + String(minted));
    },
    write: (path, workflow) => {
      writes.push(path);
      files.set(path, structuredClone(workflow));
      return Promise.resolve(JSON.stringify(workflow));
    },
    remove: (path) => {
      files.delete(path);
      return Promise.resolve();
    },
  };
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** To, co czyta człowiek: bez znaczników, z rozwiniętymi encjami, bez nadmiarowych odstępów. */
function visibleText(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, ' ')
    .replaceAll('&quot;', '"')
    .replaceAll('&#x27;', "'")
    .replaceAll('&lt;', '<')
    .replaceAll('&gt;', '>')
    .replaceAll('&amp;', '&')
    .replace(/\s+/g, ' ')
    .trim();
}

function onlyPath(paths: readonly string[]): string {
  const [first] = paths;
  if (paths.length !== 1 || first === undefined) {
    throw new Error(
      'this test expects exactly one file to be written, and got ' + String(paths.length),
    );
  }
  return first;
}

describe('the empty workflow list invites, and the button that invites really creates', () => {
  it('says what is missing and what to do about it, in two English sentences', async () => {
    const io = disk([]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    const markup = renderToStaticMarkup(
      <WorkflowList
        workflows={store.getState().workflows}
        pendingDeleteId={null}
        onOpen={() => undefined}
        onRun={() => undefined}
        actions={store.getState()}
      />,
    );
    const text = visibleText(markup);

    expect(text, 'the invitation names what is not there yet').toContain('No workflows yet.');
    expect(
      text,
      'and the second sentence says what to do, which is the whole difference between an ' +
        'invitation and a report about missing data (DESIGN §6)',
    ).toContain('Create one to lay out the steps a run follows.');
    expect(text, 'never the language of a database with no rows in it (DESIGN §8)').not.toMatch(
      /no records|no results|nothing found|not found|is empty/i,
    );
  });

  it('offers exactly one button and nothing else to look at', async () => {
    const io = disk([]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    const markup = renderToStaticMarkup(
      <WorkflowList
        workflows={store.getState().workflows}
        pendingDeleteId={null}
        onOpen={() => undefined}
        onRun={() => undefined}
        actions={store.getState()}
      />,
    );

    expect(
      occurrences(markup, '<button'),
      'one primary button on an empty screen (DESIGN §6). A second way in from the header at ' +
        'the same time is a second flow, and a second place where a file gets created',
    ).toBe(1);
    expect(occurrences(markup, 'data-create'), 'and it is the one that creates').toBe(1);
    expect(
      occurrences(markup, 'data-tile'),
      'no tiles, because there is nothing to put in them',
    ).toBe(0);
    expect(
      markup,
      'and no table headers standing over an empty grid, waiting to be filled',
    ).not.toMatch(/<t(?:able|head|body|r|h|d)\b/i);
  });

  it('creates for real when the action the screen was handed is called', async () => {
    const io = disk([]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    /* Ten sam obiekt, który dostaje ekran — akcje w zustandzie są stabilne między renderami,
     * więc `create` wzięte stąd jest tym samym `create`, które siedzi pod przyciskiem. */
    const actions = store.getState();
    renderToStaticMarkup(
      <WorkflowList
        workflows={store.getState().workflows}
        pendingDeleteId={null}
        onOpen={() => undefined}
        onRun={() => undefined}
        actions={actions}
      />,
    );

    await actions.create('Ship it');

    expect(
      io.writes,
      'the invitation has to create a file, not a screen state. A button that only changes ' +
        'what is on screen leaves nothing behind at the next start (invariant 4)',
    ).toHaveLength(1);
    expect(
      io.files.get(onlyPath(io.writes))?.name,
      'and the file carries the name that was asked for',
    ).toBe('Ship it');
    expect(
      store.getState().workflows.map((listed) => listed.workflow.name),
      'and the new workflow joins the list without a reload',
    ).toEqual(['Ship it']);
  });

  it('drops the invitation as soon as there is one workflow', async () => {
    const io = disk([]);
    const store = createWorkflowListStore(io);
    await store.getState().load();
    await store.getState().create('Ship it');

    const markup = renderToStaticMarkup(
      <WorkflowList
        workflows={store.getState().workflows}
        pendingDeleteId={null}
        onOpen={() => undefined}
        onRun={() => undefined}
        actions={store.getState()}
      />,
    );

    expect(
      visibleText(markup),
      'an invitation that stays after the first workflow is a permanent apology for a state ' +
        'the user has already left',
    ).not.toContain('No workflows yet');
    expect(occurrences(markup, 'data-empty'), 'the empty state is gone from the tree').toBe(0);
    expect(occurrences(markup, 'data-tile'), 'and the one workflow is on screen').toBe(1);
    expect(
      occurrences(markup, 'data-create'),
      'the way in never disappears — with workflows on screen it lives in the header',
    ).toBeGreaterThanOrEqual(1);
  });
});
