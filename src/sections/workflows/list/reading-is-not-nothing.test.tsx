/* „No workflows yet." nie ma prawa paść, zanim ktokolwiek zajrzał do katalogu.
 *
 * TA SAMA WADA, CO W AGENTS I W SKILLS, zgłoszona przez właściciela 2026-08-31: magazyn wstaje
 * z pustą listą, odczyt katalogu biegnie dopiero w efekcie po zamontowaniu, a ekran twierdzi
 * „nic tam nie ma", zanim cokolwiek przeczytał. „Nikt nie patrzył" i „nic tam nie ma" to dwa
 * różne zdania.
 *
 * DRUGA POŁOWA TEJ WADY JEST TU CIĘŻSZA NIŻ W AGENTS i to ona jest powodem, dla którego ten
 * plik pyta też o odmowę. `load()` w `./store.ts` nie miał ŻADNEGO `catch`: odrzucenie z granicy
 * IPC leciało jako nieobsłużona obietnica (`void store.getState().load()` w `../index.tsx`),
 * a na ekranie zostawało zaproszenie do utworzenia workflow w katalogu, którego nie da się
 * przeczytać. Cisza plus zaproszenie to najgorszy z możliwych stanów: wygląda dokładnie jak
 * pierwsze uruchomienie.
 *
 * SŁABĄ WERSJĄ jest asercja na polu magazynu. Przechodzi nad ekranem, który tego pola nigdy
 * nie czyta (niezmiennik 29), więc wszystkie trzy stany są tu czytane Z MARKUPU.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { WorkflowEntry, WorkflowFile, WorkflowListIo } from './store';
import { createWorkflowListStore } from './store';
import { WorkflowList } from './workflow-list';

const NO_WORKFLOWS_YET = 'No workflows yet.';
const READING = 'Reading the workflows you have saved';
const COULD_NOT_READ = 'the workflows folder is not readable, so nothing could be listed';

function workflow(id: string, name: string): WorkflowFile {
  return { format: 1, id, name, steps: [], links: [] };
}

function entry(path: string, id: string, name: string): WorkflowEntry {
  return { path, place: 'project', workflow: workflow(id, name) };
}

/** Atrapa katalogu: odpowiada tym, co jej podano, albo odmawia zdaniem Rusta. */
function io(answer: readonly WorkflowEntry[] | string): WorkflowListIo {
  return {
    list: () =>
      typeof answer === 'string' ? Promise.reject(answer) : Promise.resolve([...answer]),
    newId: () => Promise.resolve('wf-new'),
    write: () => Promise.resolve('after-the-write'),
    remove: () => Promise.resolve(),
  };
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Ekran listy z CAŁYM stanem magazynu — tak, jak podaje go sekcja (`../index.tsx`). */
function screenOf(store: ReturnType<typeof createWorkflowListStore>): string {
  const state = store.getState();
  return renderToStaticMarkup(
    <WorkflowList
      workflows={state.workflows}
      problems={state.problems}
      library={state.library}
      refusal={state.refusal}
      pendingDeleteId={state.pendingDeleteId}
      actions={state}
      onOpen={() => undefined}
      onRun={() => undefined}
    />,
  );
}

describe('the empty workflow list tells reading, nothing and unreadable apart', () => {
  it('does not say the folder is empty before anything has looked at it', () => {
    const store = createWorkflowListStore(io([entry('ship.json', 'w-1', 'Ship a feature')]));

    const markup = screenOf(store);

    expect(
      markup,
      'the list is empty because nobody has read the folder yet, and saying "No workflows yet." ' +
        'about a folder holding work is a statement the screen has no grounds for',
    ).not.toContain(NO_WORKFLOWS_YET);
    expect(
      markup,
      'and it has to say what it is doing instead — an empty rectangle reads as a section that ' +
        'failed to load',
    ).toContain(READING);
    expect(
      markup,
      'the moving dots say the reading is still going (DESIGN §7, the .thinking primitive)',
    ).toContain('data-reading');
    expect(
      occurrences(markup, 'data-create'),
      'and no invitation under it: at this moment nobody knows whether this folder is empty',
    ).toBe(0);
  });

  it('invites once the folder really answered with nothing', async () => {
    const store = createWorkflowListStore(io([]));
    await store.getState().load();

    const markup = screenOf(store);

    expect(markup, 'the folder answered and it holds nothing (DESIGN §6)').toContain(
      NO_WORKFLOWS_YET,
    );
    expect(markup, 'the reading is over, so that sentence goes').not.toContain(READING);
    expect(occurrences(markup, 'data-create'), 'exactly one way in at zero').toBe(1);
  });

  it('says so out loud when the folder could not be read, and drops the invitation', async () => {
    const store = createWorkflowListStore(io(COULD_NOT_READ));

    /* NIE ODRZUCA W GÓRĘ. Do 2026-08-31 `load()` nie miał `catch` i to odrzucenie kończyło jako
     * nieobsłużona obietnica — zero pikseli na ekranie, sekcja pusta i zapraszająca. */
    await expect(
      store.getState().load(),
      'the section calls this with a bare `void`, so a rejection here is a silence nobody ever ' +
        'sees. It has to end in the state instead',
    ).resolves.toBeUndefined();

    const markup = screenOf(store);

    expect(
      markup,
      'the refusal never reached the screen. A folder that cannot be read looks exactly like an ' +
        'empty one, and that is the mistake that kept a whole section blank for hours',
    ).toContain(COULD_NOT_READ);
    expect(
      markup,
      'it must not say the folder is empty at the same time. One of those two is false and a ' +
        'person cannot tell which',
    ).not.toContain(NO_WORKFLOWS_YET);
    expect(markup, 'and it is not still reading either: three states').not.toContain(READING);
    expect(
      occurrences(markup, 'data-create'),
      'the offer to create goes with it: "＋ Create" under a sentence saying the folder cannot ' +
        'be read is an offer to write into the dark',
    ).toBe(0);
  });

  it('control: with workflows on screen it is neither reading nor refusing', async () => {
    const store = createWorkflowListStore(io([entry('ship.json', 'w-1', 'Ship a feature')]));
    await store.getState().load();

    const markup = screenOf(store);

    expect(markup, 'the workflow on disk is on screen').toContain('Ship a feature');
    expect(markup, 'and the reading sentence is gone').not.toContain(READING);
    expect(markup, 'and so is the empty one').not.toContain(NO_WORKFLOWS_YET);
  });
});
