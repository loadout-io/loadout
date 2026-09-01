/* Zgłoszenie właściciela 2026-08-31: „rozjebałeś tę sekcję". Zrzut pokazuje CAŁĄ sekcję
 * Workflows jako czarny ekran z dwoma zdaniami i ani jedną kontrolką:
 *
 *   „This workflow file could not be read: Operation not permitted (os error 1)."
 *   „Nothing is lost. Open that folder, put it right, and come back to this section."
 *
 * Zmierzone, zanim powstał ten plik — trzy fakty, każdy z osobnym źródłem:
 *
 *   1. `~/.loadout/workflows/` trzyma PIĘĆ czytelnych plików. Wszystkie stały za tą ścianą.
 *   2. `~/.loadout/loadout.log` ma dziesiątki wierszy `Operation not permitted (os error 1)`
 *      na ścieżkach pod `~/Desktop/...`, a ta sama powłoka, w której to mierzyłem, czyta te
 *      katalogi bez problemu. Odmawia się WYŁĄCZNIE aplikacji: to jest TCC, nie uszkodzony plik.
 *   3. Katalog `<workspace>/.loadout/workflows/` po prostu NIE ISTNIEJE. Bez TCC ta półka byłaby
 *      legalnie pusta (`shelf()` zamienia `NotFound` na pustą listę). Pod TCC nie da się jej
 *      nawet obejrzeć, więc zamiast `NotFound` przyjeżdża `EPERM` — i to jest cała droga od
 *      „nie mam zgody na Desktop" do czarnego ekranu.
 *
 * O CZYM TE KRYTERIA SĄ, A O CZYM NIE SĄ. Nie o tym, że plik jest nieczytelny — to jest prawda
 * o dysku i ekran nie ma jej zaprzeczać. O tym, co ekran z tą prawdą ROBI: dokąd wysyła
 * człowieka i czy zostawia mu jakiekolwiek wyjście.
 *
 * DLACZEGO CZĘŚĆ Z NICH WOŁA KOMPONENT JAK FUNKCJĘ, a nie przez `renderToStaticMarkup`.
 * Kontrolka bez handlera nie wchodzi do repo (AGENTS.md §16), a markup tego nie widzi: `onClick`
 * nie zostawia w statycznym HTML ani jednego znaku, więc asercja „jest tam <button>" przechodzi
 * dla martwego przycisku tak samo jak dla żywego. W repo nie ma jsdom ani biblioteki do klikania
 * (`package.json` jest na liście DENIED w checks/quick-scope.sh), a `WorkflowList` nie używa ani
 * jednego haka Reacta — jest czystą funkcją propsów. Wolno ją więc zawołać wprost, przejść drzewo
 * elementów i URUCHOMIĆ ten handler. To jest mocniejsze niż klik: dowodzi, że przycisk woła
 * dokładnie `actions.load`, a nie cokolwiek, co też zmienia ekran.
 */
import type { ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Definition } from '../../../state/library';
import type { WorkflowEntry, WorkflowFile, WorkflowListIo } from './store';
import { createWorkflowListStore } from './store';
import { WorkflowList } from './workflow-list';

/** Zdanie, którym Rust opisuje odmowę dysku — słowo w słowo z `workflow::file::LoadError`. */
const RUST_SAID = 'This workflow file could not be read: Operation not permitted (os error 1).';

/** Zdanie ze zrzutu, które wysyła człowieka poprawiać zawartość pliku. */
const WRONG_MOVE = 'Open that folder, put it right, and come back to this section.';

function workflow(id: string, name: string): WorkflowFile {
  return { format: 1, id, name, steps: [], links: [] };
}

function entry(path: string, id: string, name: string): WorkflowEntry {
  return { path, place: 'library', workflow: workflow(id, name) };
}

/** Wpis o pliku, którego nie dało się przeczytać — dokładnie to, co wypisuje Rust. */
const ONE_BAD_FILE: Definition<WorkflowEntry> = {
  kind: 'definitionProblem',
  shelf: 'workflows',
  fileName: 'half-written.json',
  problem: 'unreadable',
};

interface Disk extends WorkflowListIo {
  /** Ile razy ktoś poprosił o zawartość katalogu. Liczba, nie wartość logiczna. */
  readonly asked: () => number;
}

/**
 * Katalog, który odmawia dokładnie `refusals` pierwszych razy, a potem oddaje `answer`.
 *
 * Dwie odpowiedzi z jednej atrapy, bo cała treść kryterium o „Try again" brzmi: DRUGI odczyt
 * ma się odbyć i ma dojść na ekran. Atrapa, która odmawia zawsze, nie odróżnia przycisku
 * żywego od przycisku, który tylko przerysowuje ten sam stan.
 */
function disk(answer: readonly Definition<WorkflowEntry>[], refusals = 0): Disk {
  let asked = 0;
  return {
    asked: () => asked,
    list: () => {
      asked += 1;
      return asked <= refusals ? Promise.reject(RUST_SAID) : Promise.resolve([...answer]);
    },
    newId: () => Promise.resolve('wf-new'),
    write: () => Promise.resolve('after-the-write'),
    remove: () => Promise.resolve(),
  };
}

type Store = ReturnType<typeof createWorkflowListStore>;

/**
 * Czeka, aż praca, którą ruszył handler, dobiegnie końca.
 *
 * `onClick` oddaje `void`, więc jego obietnicy nie da się złapać, a zawołanie `load()` drugi raz
 * po to, żeby mieć na co czekać, POLICZYŁOBY ODCZYT, którego przycisk nie zrobił — i kryterium
 * mierzyłoby własną rękę zamiast kontrolki (zmierzone: `expected 3 to be 2`).
 *
 * Granica makrozadania jest tu rozstrzygnięciem, nie przybliżeniem: node opróżnia CAŁĄ kolejkę
 * mikrozadań, zanim odpali timer, a cała ta droga to `await` na obietnicach rozwiązanych od razu.
 */
function afterTheClickSettles(): Promise<void> {
  return new Promise<void>((resolve) => {
    setTimeout(resolve, 0);
  });
}

/** Propsy dokładnie takie, jakie podaje sekcja (`../index.tsx`): cały stan magazynu. */
function screenOf(store: Store): ReactElement {
  const state = store.getState();
  return (
    <WorkflowList
      workflows={state.workflows}
      problems={state.problems}
      library={state.library}
      refusal={state.refusal}
      pendingDeleteId={state.pendingDeleteId}
      actions={state}
      onOpen={() => undefined}
      onRun={() => undefined}
    />
  );
}

function markupOf(store: Store): string {
  return renderToStaticMarkup(screenOf(store));
}

type Piece = ReactElement<{ children?: unknown; onClick?: () => void }>;

function isPiece(value: unknown): value is Piece {
  return typeof value === 'object' && value !== null && 'props' in value && 'type' in value;
}

/** Każdy element drzewa, w kolejności wgłąb. Wołane na WYWOŁANEJ funkcji, nie na `<JSX/>`. */
function* everyPiece(value: unknown): Generator<Piece> {
  if (Array.isArray(value)) {
    for (const one of value) yield* everyPiece(one);
    return;
  }
  if (!isPiece(value)) return;
  yield value;
  yield* everyPiece(value.props.children);
}

/**
 * Kontrolki, które ekran naprawdę oddaje, razem z ich handlerami.
 *
 * `WorkflowList` jest czystą funkcją propsów (ani jednego haka), więc to zwykłe wywołanie —
 * a nie render — i dlatego `onClick` wciąż tu jest.
 */
function buttonsOn(store: Store): Piece[] {
  const props = screenOf(store).props as Parameters<typeof WorkflowList>[0];
  return [...everyPiece(WorkflowList(props))].filter((piece) => piece.type === 'button');
}

describe('a folder Loadout cannot open leaves the person somewhere to go', () => {
  it('names what to do about a refused folder instead of sending the person to edit a file', async () => {
    const store = createWorkflowListStore(disk([], 1));
    await store.getState().load();

    const markup = markupOf(store);

    expect(markup, 'the sentence from the disk still stands, word for word').toContain(RUST_SAID);
    expect(
      markup,
      'and the line under it must stop sending the person to open the folder and repair what ' +
        'is inside. "Operation not permitted" is macOS refusing Loadout, and there is nothing ' +
        'wrong with the contents to put right — following that line changes nothing and costs ' +
        'the evening',
    ).not.toContain(WRONG_MOVE);
    expect(
      markup,
      'the line has to name the first real cause: Loadout has not been given access to that ' +
        'place. Without those words the person has no way to guess that the fix is a switch in ' +
        'System Settings and not a text editor',
    ).toContain('System Settings');
    expect(
      markup,
      'and it has to name the second one, because from here the two read identically: a folder ' +
        'that was moved, renamed or removed refuses in exactly the same way',
    ).toMatch(/moved, renamed or removed/);
    expect(
      markup,
      'nothing on disk was touched, and that stays said: a person reading a red sentence is ' +
        'asking first whether the work survived',
    ).toContain('Nothing is lost');
  });

  it('leaves a live control on that screen, and it reads the folder again', async () => {
    const io = disk([{ kind: 'healthy', value: entry('ship.json', 'wf-1', 'Ship a feature') }], 1);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    expect(io.asked(), 'the first reading happened and was refused').toBe(1);
    expect(
      markupOf(store),
      'so the screen is the refusal, not the list — that is the state this criterion is about',
    ).toContain(RUST_SAID);

    const buttons = buttonsOn(store);
    expect(
      buttons.length,
      'the screenshot has a black rectangle and not one control on it: no way to read the ' +
        'folder again, no way anywhere. A person who fixes the permission in System Settings ' +
        'has to guess that leaving the section and coming back is what reloads it',
      /* Dokładnie jeden: to jest ekran bez treści, a rząd wyborów na nim byłby zgadywaniem
         za człowieka, czego akurat potrzebuje (DESIGN §6, jedna czynność główna na ekran). */
    ).toBe(1);

    const control = buttons[0];
    expect(control, 'the walk found a button and it is a real element').toBeDefined();
    const press = control?.props.onClick;
    expect(
      typeof press,
      'and it carries a handler. A control without one does not enter this repo (AGENTS.md §16) ' +
        'and this screen is the last place that can afford a dead one',
    ).toBe('function');

    press?.();
    await afterTheClickSettles();

    expect(
      io.asked(),
      'pressing it asked the folder a second time. A button that only redraws the same refusal ' +
        'is the black rectangle with a lid on it',
    ).toBe(2);
    const after = markupOf(store);
    expect(
      after,
      'and the second answer reached the screen: the workflow that was there all along',
    ).toContain('Ship a feature');
    expect(after, 'the refusal is gone with it').not.toContain(RUST_SAID);
  });

  it('control: one file it cannot read is a row on the list, never a wall over it', async () => {
    const store = createWorkflowListStore(
      disk([
        { kind: 'healthy', value: entry('ship.json', 'wf-1', 'Ship a feature') },
        ONE_BAD_FILE,
      ]),
    );
    await store.getState().load();

    const markup = markupOf(store);

    expect(
      markup,
      'the good workflow is on screen. The whole point of carrying a problem PER FILE is that ' +
        'one bad file costs one row',
    ).toContain('Ship a feature');
    expect(markup, 'and the bad one is a row of its own, named by its file').toContain(
      'data-definition-problem="half-written.json"',
    );
    expect(
      markup,
      'and the full-screen refusal is NOT up. This is the half the owner suspected and it is ' +
        'already right: the screen goes dark only when the listing itself was refused',
    ).not.toContain('data-refusal');
  });

  it('mutation: the wall is still there for the one case that earns it — nothing to list', async () => {
    const store = createWorkflowListStore(disk([], 1));
    await store.getState().load();

    const markup = markupOf(store);

    expect(
      markup,
      'the folder itself refused, so there is genuinely nothing to put on a list and the screen ' +
        'says so. Without this the control above would pass for a screen that simply never ' +
        'refuses at all',
    ).toContain('data-refusal');
    expect(
      markup,
      'and it does not invite anyone to write into a folder it cannot read',
    ).not.toContain('data-create');
  });
});
