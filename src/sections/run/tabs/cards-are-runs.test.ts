/* KARTY SĄ BIEGAMI, NIE FOLDERAMI — i `×` zatrzymuje TEN bieg, nie cudzy.
 *
 * SŁABE WERSJE:
 *   1. „`cardsIn` oddaje kartę tego zakresu" sprawdzone na JEDNEJ karcie przechodzi na
 *      implementacji, która nie filtruje nic. Dlatego są dwie karty w dwóch zakresach.
 *   2. „`×` woła `stop`" przechodzi na domknięciu `() => stop()`, czyli na tym defekcie, który
 *      ta funkcja zamyka: znak `×` na karcie, w której nic nie chodzi, ubijał wtedy bieg idący
 *      gdzie indziej. Dlatego liczy się wywołania po zamknięciu karty MARTWEJ i po zamknięciu
 *      karty ŻYWEJ, i te dwie liczby muszą się różnić.
 *   3. Sprawdzenie samego `stop` bez sesji — nie odróżnia „nie wołamy, bo tu nic nie idzie" od
 *      „nie wołamy nigdy".
 *
 * `../io` PODSTAWIONE, bo prawdziwy `stop` woła `stop_run` przez granicę Tauri, której w vitest
 * nie ma. Zatrzymanie jest tu jedyną rzeczą, o którą pytamy — więc atrapa jest dokładnie tym
 * miejscem, w którym widać odpowiedź.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { stopped } = vi.hoisted(() => ({ stopped: vi.fn(() => Promise.resolve()) }));

vi.mock('../io', () => ({ stop: stopped, start: vi.fn(), continueRun: vi.fn() }));

const { cardForRun, cardsIn, runTabs } = await import('./store');
const { runFor } = await import('../../../state/run');

const HERE = '/Users/x/ledger-ui';
const THERE = '/Users/x/meetnotes';

beforeEach(() => {
  stopped.mockClear();
  runTabs.setState({ tabs: [], activeId: null, pendingClose: null });
  runFor(HERE).getState().nowRunning('', [], null);
  runFor(THERE).getState().nowRunning('', [], null);
  runFor(null).getState().nowRunning('', [], null);
});

describe('the tab bar shows the runs of the scope a person is standing in', () => {
  it('keeps the cards of this scope and hides the ones from another', () => {
    cardForRun('Ship it', HERE);
    cardForRun('Read the invoices', THERE);

    const all = runTabs.getState().tabs;
    expect(all.length, 'both runs have to be in the store; only the VIEW is scoped').toBe(2);

    expect(
      cardsIn(all, HERE).map((card) => card.name),
      'the bar has to carry the run of the active scope, and only it. An unfiltered bar shows ' +
        'a person the runs of every project they ever opened, with no way to tell which is here.',
    ).toEqual(['Ship it']);
    expect(
      cardsIn(all, THERE).map((card) => card.name),
      'and the other scope has to see its own run, not the first one in the list',
    ).toEqual(['Read the invoices']);
    expect(
      cardsIn(all, null).length,
      'without a scope there is nothing to filter BY, and a hidden card is a run nobody can ' +
        'stop with × (invariant 6). So: no scope, no filter.',
    ).toBe(2);
  });

  it('closes a card with nothing running on it without touching the boundary', async () => {
    cardForRun('Ship it', HERE);
    /* Karta bez agentów zamyka się od razu — pytanie zadaje się tylko wtedy, kiedy jest o co. */
    runTabs.getState().requestClose(HERE);

    expect(
      runTabs.getState().tabs,
      'closing a card with nobody working on it has to take it off the bar at once',
    ).toEqual([]);
    expect(
      stopped,
      'stop_run was called for a card whose run had already finished. The engine runs one run ' +
        'at a time, so that call lands on whatever is going NOW — that is somebody else work, ' +
        'killed without a question (invariant 6).',
    ).not.toHaveBeenCalled();
  });

  it('stops the run of the card being closed, and stays quiet about another scope run', async () => {
    cardForRun('Ship it', HERE);
    cardForRun('Read the invoices', THERE);
    runTabs.getState().setAgents(HERE, 2);
    runTabs.getState().setAgents(THERE, 1);

    /* W THERE NIC NIE IDZIE, a w HERE tak: tylko tam magazyn biegu zna nazwę workflow. */
    runFor(HERE).getState().nowRunning('Ship it', [], HERE);

    runTabs.getState().requestClose(THERE);
    await runTabs.getState().confirmClose();
    expect(
      stopped,
      'closing the card of a scope where nothing is running called stop_run anyway. With one ' +
        'run at a time on the Rust side that call kills the run in the OTHER scope — the exact ' +
        'defect this function exists to close.',
    ).not.toHaveBeenCalled();
    expect(
      runTabs.getState().tabs.map((card) => card.id),
      'the card still has to come off the bar: nothing was running on it, so there is nothing ' +
        'to wait for',
    ).toEqual([HERE]);

    runTabs.getState().requestClose(HERE);
    await runTabs.getState().confirmClose();
    expect(
      stopped,
      'closing the card of the run that IS going has to stop it. A × that only takes the card ' +
        'off the bar leaves an agent running and burning the usage limit — a financial error, ' +
        'not a hygiene one (invariant 6).',
    ).toHaveBeenCalledTimes(1);
  });
});
