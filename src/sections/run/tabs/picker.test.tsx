/* Kryterium 6 dla T-24: `×` na karcie z żywym biegiem pyta, a po potwierdzeniu anuluje czysto.
 *
 * SŁABA WERSJA: „po kliknięciu × karta znika". Przechodzi na implementacji, która zdejmuje
 * kartę natychmiast i anuluje w tle — czyli na tej, która zostawia osieroconego agenta palącego
 * limit u dostawcy, dokładnie wbrew niezmiennikowi 6. Na ekranie obie wyglądają identycznie
 * i różnią się wyłącznie rachunkiem na koniec miesiąca.
 *
 * Rozróżnia KOLEJNOŚĆ, mierzona jedynym sposobem, jakim się da: anulowanie wstrzyknięte do
 * fabryki zwraca obietnicę, której ten plik **nie rozwiązuje**, i w tej chwili — po anulowaniu,
 * przed jego zakończeniem — karta musi wciąż stać na pasku. Dopiero po rozwiązaniu znika.
 * Implementacja odwrócona pada tutaj i nigdzie indziej.
 *
 * Trzy pozostałe zdania kryterium są w tym samym pliku, bo opisują ten sam przycisk:
 * odrzucenie nie zmienia niczego, anulowanie woła się DOKŁADNIE RAZ, a karta bez pracujących
 * agentów zamyka się od razu i bez pytania.
 *
 * DLACZEGO PYTANIE JEST MIERZONE NA MAGAZYNIE, A NIE PRZEZ KLIKNIĘCIE. W tym repo nie ma jsdom
 * ani `@testing-library/react`, a dołożenie ich to zmiana `package.json`, czyli moment na
 * zatrzymanie się i zapytanie człowieka (AGENTS.md §7). Kolejność żyje w magazynie kart i tam
 * jest sprawdzalna; komponent zostaje czystą funkcją stanu na markup i jego jedno zdanie —
 * liczba pracujących agentów — jest sprawdzone przez `renderToStaticMarkup`.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { PendingClose, WorkspaceTab, WorkspacesStore } from '../../../state/run-tabs';
import { createWorkspacesStore } from '../../../state/run-tabs';
import { CloseConfirm } from './picker';

/** Karta z trzema pracującymi agentami — ta, o której zamknięcie trzeba zapytać. */
const MEETNOTES: WorkspaceTab = {
  id: 'ws-meetnotes',
  name: 'meetnotes',
  path: '/Users/you/Projects/meetnotes',
  agents: 3,
};

/** Karta na wierzchu. Zamykamy nie ją, żeby „nic się nie zmieniło" dało się w ogóle zmierzyć. */
const SPREADSHEET: WorkspaceTab = {
  id: 'ws-spreadsheet',
  name: 'spreadsheet',
  path: '/Users/you/Projects/spreadsheet',
  agents: 1,
};

/** Karta, w której nic nie chodzi. */
const LOADOUT: WorkspaceTab = {
  id: 'ws-loadout',
  name: 'Loadout',
  path: '/Users/you/Projects/Loadout',
  agents: 0,
};

interface Harness {
  /** Magazyn zasiany przez prawdziwą fabrykę — trzy karty, `spreadsheet` na wierzchu. */
  readonly store: WorkspacesStore;
  /** Foldery, dla których zawołano anulowanie, w kolejności wywołań. */
  readonly cancelled: readonly string[];
  /** Kończy trwające anulowanie. Do tej chwili obietnica wisi i to jest cały pomiar. */
  readonly finishCancelling: () => void;
}

function harness(): Harness {
  const cancelled: string[] = [];
  let release = (): void => {
    // Podmieniane przy pierwszym wywołaniu anulowania; do tego czasu nie ma czego kończyć.
  };

  const store = createWorkspacesStore((id: string) => {
    cancelled.push(id);
    return new Promise<void>((resolve) => {
      release = resolve;
    });
  });

  store.getState().open(MEETNOTES);
  store.getState().open(SPREADSHEET);
  store.getState().open(LOADOUT);
  store.getState().activate(SPREADSHEET.id);

  return {
    store,
    cancelled,
    finishCancelling: () => {
      release();
    },
  };
}

/** Przepuszcza wszystko, co czeka w kolejce zadań, bez rozwiązywania anulowania. */
function settle(): Promise<void> {
  return new Promise<void>((resolve) => {
    setTimeout(resolve, 0);
  });
}

function names(store: WorkspacesStore): readonly string[] {
  return store.getState().tabs.map((tab) => tab.name);
}

function noop(): void {
  // Handlery są wymagane, ale ten przypadek pyta wyłącznie o treść zdania.
}

function confirmText(pending: PendingClose): string {
  const markup = renderToStaticMarkup(
    <CloseConfirm pending={pending} onConfirm={noop} onDismiss={noop} />,
  );
  const hit = /<([a-z]+)[^>]*\bdata-close-confirm\b[^>]*>([\s\S]*?)<\/\1>/i.exec(markup);
  return (hit?.[2] ?? '')
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('closing a tab that has agents working in it', () => {
  it('asks before it does anything', () => {
    const { store, cancelled } = harness();
    store.getState().requestClose(MEETNOTES.id);

    expect(
      store.getState().pendingClose,
      'the × on a tab with three agents at work has to raise a question naming that tab and ' +
        'how much work stands behind the click',
    ).toEqual({ id: MEETNOTES.id, name: MEETNOTES.name, agents: MEETNOTES.agents });
    expect(names(store), 'and nothing may be removed while the question is open').toEqual([
      MEETNOTES.name,
      SPREADSHEET.name,
      LOADOUT.name,
    ]);
    expect(cancelled, 'and nothing may be stopped either').toEqual([]);
  });

  it('changes nothing at all when the answer is no', async () => {
    const { store, cancelled } = harness();
    store.getState().requestClose(MEETNOTES.id);
    store.getState().dismissClose();
    await settle();

    expect(store.getState().pendingClose, 'the question is gone').toBe(null);
    expect(names(store), 'every tab is where it was').toEqual([
      MEETNOTES.name,
      SPREADSHEET.name,
      LOADOUT.name,
    ]);
    expect(store.getState().activeId, 'and so is the tab on top').toBe(SPREADSHEET.id);
    expect(
      cancelled,
      'turning the question down is not a quiet yes: nothing may be stopped here',
    ).toEqual([]);
  });

  it('removes the tab only after cancelling has come back', async () => {
    const { store, cancelled, finishCancelling } = harness();
    store.getState().requestClose(MEETNOTES.id);

    const closing = store.getState().confirmClose();
    await settle();

    expect(
      cancelled,
      'saying yes has to stop the run in that folder, and name that folder when it does',
    ).toEqual([MEETNOTES.id]);
    expect(
      names(store),
      'and the tab has to be STILL THERE at this moment. Cancelling has not come back yet — ' +
        'the run is being wound down. A tab that vanishes now looks finished on screen while ' +
        'the agent it started is still alive and still burning the limit at the vendor, which ' +
        'is a money mistake rather than a tidiness one (invariant 6)',
    ).toEqual([MEETNOTES.name, SPREADSHEET.name, LOADOUT.name]);

    finishCancelling();
    await closing;

    expect(names(store), 'now that it is over, the tab goes').toEqual([
      SPREADSHEET.name,
      LOADOUT.name,
    ]);
    expect(store.getState().pendingClose, 'and the question with it').toBe(null);
    expect(
      cancelled,
      'exactly once, from beginning to end. Two stop signals for one run is two escalations ' +
        'racing each other over the same agent',
    ).toEqual([MEETNOTES.id]);
    expect(store.getState().activeId, 'closing another tab does not move the view').toBe(
      SPREADSHEET.id,
    );
  });

  it('closes a folder with nobody working in it at once, without asking', async () => {
    const { store, cancelled } = harness();
    store.getState().requestClose(LOADOUT.id);
    await settle();

    expect(
      store.getState().pendingClose,
      'there is nothing to ask about: a confirmation on every close teaches people to click ' +
        'yes without reading, and then it protects nobody',
    ).toBe(null);
    expect(names(store), 'the tab is gone right away').toEqual([MEETNOTES.name, SPREADSHEET.name]);
    expect(cancelled, 'and no run was stopped, because none was running').toEqual([]);
  });
});

describe('the question itself', () => {
  it('says how many agents it is about', () => {
    const three = confirmText({ id: MEETNOTES.id, name: MEETNOTES.name, agents: 3 });
    expect(
      three,
      'the marked element has to hold the words, otherwise there is nothing to read',
    ).not.toBe('');
    expect(
      three,
      'three agents are at work, so the question says three. "Are you sure?" tells a person ' +
        'nothing about what the click costs, so the only possible answer to it is a reflex',
    ).toContain('3 agents');
    expect(three, 'and it names the folder, not "the current tab"').toContain(MEETNOTES.name);

    const one = confirmText({ id: SPREADSHEET.id, name: SPREADSHEET.name, agents: 1 });
    expect(
      one,
      'a different folder with a different amount of work has to read differently. One sentence ' +
        'typed in by hand reads the same at one agent and at eight',
    ).not.toBe(three);
  });
});
