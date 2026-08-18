/* AC-7 dla T-39: zamknięcie karty z żywym biegiem PYTA, a potwierdzenie NAPRAWDĘ zatrzymuje.
 *
 * DEFEKT, KTÓRY TO KRYTERIUM ZAMYKA, i został znaleziony dopiero przez sprawdzającego, bo żadne
 * AC go nie dotykało. `WorkspaceTab.agents` było pisane WYŁĄCZNIE przy zakładaniu karty i zawsze
 * zerem — nikt go nigdy nie podnosił. `requestClose` decyduje na tym polu, więc zawsze wchodził
 * w gałąź „nic tu nie chodzi": karta z biegiem znikała **bez pytania i bez `cancel(id)`**.
 * Skutki dwa, oba realne:
 *
 *   1. Bieg zostawał osierocony i dalej palił limit u dostawcy. `src/state/workspaces.ts` nazywa
 *      to wprost słowami niezmiennika 6: **błąd finansowy, nie higieniczny.**
 *   2. `CloseConfirm` — zamontowany, otestowany, z prawdziwym handlerem — był kodem
 *      NIEOSIĄGALNYM. Czyli dokładnie ta klasa, którą całe T-39 miało skończyć: mechanizm
 *      istnieje, nikt go nie woła, a nic tego nie mówi.
 *
 * SŁABA WERSJA, którą świadomie odrzucam: `expect(store.setAgents).toBeTypeOf('function')`
 * albo wywołanie `setAgents` wprost z testu i sprawdzenie, że pole urosło. Obie przechodzą na
 * kodzie, w którym EKRAN nigdy tego settera nie woła — czyli na dokładnie tym defekcie, o który
 * chodzi. Dlatego niżej liczba bierze się z wyrenderowania EKRANU, a nie z wywołania w teście.
 *
 * DLACZEGO `renderToStaticMarkup` NIE WYSTARCZA I CO ROBIĘ ZAMIAST. Render serwerowy nie
 * uruchamia efektów, a synchronizacja licznika mieszka w `useEffect` — więc markup nigdy jej nie
 * zobaczy. Zamiast udawać, że widzi, ten plik sądzi DWIE POŁOWY osobno i mówi to wprost:
 * (a) magazyn kart zachowuje się poprawnie przy każdej liczbie agentów — to jest cała logika
 * decyzji; (b) ekran naprawdę zawiera wywołanie synchronizujące — czytane z jego źródła, bo
 * to jedyny sposób bez jsdom. Druga połowa jest słabsza i jest tu nazwana, zamiast przemilczana.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, vi } from 'vitest';
import { createWorkspacesStore } from '../../state/workspaces';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const SCREEN = resolve(ROOT, 'src/sections/run/index.tsx');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Karta na folderze, z biegiem albo bez. */
function openOne(store: ReturnType<typeof createWorkspacesStore>): string {
  const id = 'ws-1';
  store.getState().open({ id, name: 'meetnotes', path: '/Users/x/Projects/meetnotes', agents: 0 });
  return id;
}

describe('closing a tab with a live run asks first, and stopping really stops', () => {
  it('a tab with nobody working closes without a question', () => {
    const cancel = vi.fn(async () => undefined);
    const store = createWorkspacesStore(cancel);
    const id = openOne(store);

    store.getState().requestClose(id);

    expect(
      store.getState().pendingClose,
      'a folder where nothing is running has nothing to ask about. A confirmation on EVERY ' +
        'close teaches people to click yes without reading, and then it stops protecting the ' +
        'one close that mattered.',
    ).toBeNull();
    expect(store.getState().tabs.map((tab) => tab.id)).not.toContain(id);
    expect(cancel, 'nothing was running, so nothing had to be cancelled').not.toHaveBeenCalled();
  });

  it('a tab with agents at work asks instead of vanishing', () => {
    const cancel = vi.fn(async () => undefined);
    const store = createWorkspacesStore(cancel);
    const id = openOne(store);

    store.getState().setAgents(id, 3);
    store.getState().requestClose(id);

    expect(
      store.getState().pendingClose,
      'the tab carried three working agents and closed without a word. That is the whole ' +
        'defect: the run keeps burning the provider quota with nobody watching it, and ' +
        'src/state/workspaces.ts calls that a financial error, not a hygiene one (invariant 6).',
    ).not.toBeNull();
    expect(
      store.getState().tabs.map((tab) => tab.id),
      'the tab has to stay on screen while the question is open — a tab that is already gone ' +
        'cannot be the subject of the question.',
    ).toContain(id);
  });

  it('confirming the question really cancels the run, and only then drops the tab', async () => {
    const cancelled: string[] = [];
    const cancel = vi.fn(async (id: string) => {
      cancelled.push(id);
    });
    const store = createWorkspacesStore(cancel);
    const id = openOne(store);

    store.getState().setAgents(id, 2);
    store.getState().requestClose(id);
    await store.getState().confirmClose();

    expect(
      cancelled,
      'confirming the close did not cancel the run. The question would then be theatre: it ' +
        'asks about agents at work and then leaves them at work.',
    ).toEqual([id]);
    expect(
      store.getState().tabs.map((tab) => tab.id),
      'the tab has to go once the run is proven stopped',
    ).not.toContain(id);
  });

  it('the run screen actually feeds that count — the store cannot know it alone', () => {
    const source = fileText(SCREEN);

    expect(
      source,
      'src/sections/run/index.tsx could not be read, so the check below would pass on an ' +
        'empty string and prove nothing.',
    ).not.toBe('');
    expect(
      source.includes('setAgents('),
      'the run screen never tells the tab how many agents are at work. Every assertion above ' +
        'passes on a store that is never fed — which is exactly the state this criterion was ' +
        'written to end: the count stayed 0 forever, so the question never came and the run ' +
        'was never cancelled.',
    ).toBe(true);
    expect(
      source.includes('cards.length'),
      'the count has to come from the same list that draws the agent rail. A second source ' +
        'for "how many are working" is a second answer to one question (invariant 13), and the ' +
        'two would drift on the first run that ends.',
    ).toBe(true);
  });
});
