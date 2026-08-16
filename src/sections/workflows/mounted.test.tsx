/* Kryterium 1 dla T-26: sekcja Workflows montuje się NAPRAWDĘ i pokazuje listę, a nie zdanie
 * z rejestru.
 *
 * DWIE POŁOWY I ANI JEDNEJ MNIEJ.
 *   (a) montaż przez PRAWDZIWE odkrywanie — `<App section="workflows" />` BEZ propsu `screens`,
 *       czyli przez glob z `src/ui/screens.ts`. To jest połowa, dla której to zadanie istnieje:
 *       lista workflow była zielona od T-14, a w oknie stało zdanie z rejestru, bo nikt nie
 *       napisał `src/sections/workflows/index.tsx`. Powłoka z propsem `screens` tego nie widzi
 *       — i dlatego cztery sekcje przeżyły całą bramkę bez ekranu.
 *   (b) treść — ten sam ekran wprost, z magazynem zasianym atrapą `WorkflowListIo`. Bez tej
 *       połowy ekran, który ZAWSZE rysuje pustkę, przechodzi (a) i nikt tego nie zauważa.
 *
 * KONTROLA PRZECIW PUSTEJ ASERCJI, w osobnym teście. „Nie ma zdania z rejestru" przechodzi
 * także wtedy, gdy powłoka przestała je renderować W OGÓLE — czyli gdy zamiast zamontować
 * ekran ktoś zepsuł pustkę. Kontrola pyta powłokę BEZ ekranów o to samo zdanie i wymaga,
 * żeby tam było.
 *
 * DWA ZDANIA, DWA SPOSOBY ZAPISANIA, I TO NIE JEST NIEKONSEKWENCJA.
 *   `sectionEntry('workflows').empty` jest CZYTANE z rejestru: rejestr jest jedynym miejscem,
 *   w którym mieszka zdanie pustej sekcji (niezmiennik 13), więc kopia w teście rozjechałaby
 *   się z nim przy pierwszej zmianie brzmienia i nikt by o tym nie wiedział.
 *   `No workflows yet.` jest WPISANE tutaj ręcznie: to jest zdanie pustej LISTY i to ono jest
 *   kontraktem tego kryterium. Zaimportowane z `workflow-list.tsx` zgadzałoby się z komponentem
 *   zawsze — także wtedy, gdyby komponent przestał je pokazywać.
 *
 * Asercja o BRAKU `data-empty` byłaby tu pomyłką i jest świadomie nieobecna: `WorkflowList`
 * używa `data-empty` przy zerze workflow, więc kryterium mierzyłoby co innego, niż mówi.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import { sectionEntry } from '../../ui/sections';
import WorkflowsScreen from './index';
import type { WorkflowEntry, WorkflowFile, WorkflowListIo } from './list/store';
import { createWorkflowListStore } from './list/store';

/** Zdanie pustej LISTY (`workflow-list.tsx`) — nie zdanie pustej SEKCJI z rejestru. */
const LIST_IS_EMPTY = 'No workflows yet.';

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

function workflow(id: string, name: string): WorkflowFile {
  return { format: 1, id, name, steps: [], links: [] };
}

/** Atrapa dysku: `list` oddaje to, co zasialiśmy, a reszta nie ma prawa zostać zawołana. */
function ioWith(entries: readonly WorkflowEntry[]): WorkflowListIo {
  return {
    list: () => Promise.resolve([...entries]),
    newId: () => {
      throw new Error('the seeded list never asks for a fresh id');
    },
    write: () => {
      throw new Error('the seeded list never writes to disk');
    },
    remove: () => {
      throw new Error('the seeded list never removes anything');
    },
  };
}

describe('the workflows section mounts for real and shows the list', () => {
  it('mounts through real discovery and says what an empty list says', () => {
    const markup = renderToStaticMarkup(<App section="workflows" />);

    expect(
      markup,
      'asking the shell for workflows WITHOUT handing it screens has to reach the file on ' +
        'disk. What stood here instead was the sentence from the registry — with the list ' +
        'itself landed, green and mounted by nobody',
    ).toContain(LIST_IS_EMPTY);
    expect(
      occurrences(markup, 'data-create'),
      'the empty list is an invitation, so exactly one way to create a workflow is on ' +
        'screen. Zero means the list is not really mounted; two means a second way to make ' +
        'a file, which is the first chance for the two to drift apart (invariant 16)',
    ).toBe(1);
    expect(
      markup,
      'with a screen mounted, the sentence the registry keeps for an empty workflows section ' +
        'has no business being in the document: one fact, one place (invariant 13)',
    ).not.toContain(sectionEntry('workflows').empty);
  });

  it('control: with no screen in hand the shell still says the registry sentence', () => {
    const markup = renderToStaticMarkup(<App section="workflows" screens={{}} />);

    expect(
      markup,
      'this is the control against an empty assertion. Without it, "the registry sentence is ' +
        'gone" also passes on a shell that stopped rendering that sentence at all — that is, ' +
        'when somebody broke the empty screen instead of mounting a real one',
    ).toContain(sectionEntry('workflows').empty);
  });

  it('lists what the store holds, and drops the empty sentence once there is something', async () => {
    const store = createWorkflowListStore(
      ioWith([
        { path: 'ship-a-feature.json', workflow: workflow('w-1', 'Ship a feature') },
        { path: 'deep-research.json', workflow: workflow('w-2', 'Deep research') },
      ]),
    );
    await store.getState().load();

    const markup = renderToStaticMarkup(<WorkflowsScreen store={store} />);

    expect(
      markup,
      'both names the store holds have to reach the document. A screen that mounts and then ' +
        'draws its own idea of the folder is worth exactly as much as no screen at all',
    ).toContain('Ship a feature');
    expect(markup, 'the second workflow has to be there too, not just the first').toContain(
      'Deep research',
    );
    expect(
      markup,
      'with two workflows in the store the empty sentence has to be gone. Without this half, ' +
        'a screen that always draws the empty list passes the mounting half above',
    ).not.toContain(LIST_IS_EMPTY);
  });
});
