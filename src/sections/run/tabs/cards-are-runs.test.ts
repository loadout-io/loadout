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

/* ATRAPA BIERZE ARGUMENT, i to nie jest kosmetyka typu: bez niego `mock.calls` jest listą pustych
 * krotek, a `addressless()` niżej nie miałby czego przeczytać — czyli asercja o adresie
 * przechodziłaby dla każdego wywołania, także dla `stop()` bez niczego. */
const { stopped } = vi.hoisted(() => ({
  stopped: vi.fn((_where?: string | null) => Promise.resolve()),
}));

/* `closeTerminal` dołożone do atrapy 2026-08-20: magazyn kart bierze z `../io` DWA kanały —
 * zatrzymanie biegu i koniec rozmowy z liderem zamykanej karty — a atrapa znała tylko pierwszy,
 * więc vitest przewracał się na kolekcji, przed pierwszą asercją. Ani jedna asercja niżej się
 * o niego nie pyta i żadnej nie ubyło. Oddaje spełnioną obietnicę, bo magazyn wiesza na niej
 * `.catch` (`store.ts`, `endLeadOf`); atrapa bez obietnicy mierzyłaby brak atrapy. */
vi.mock('../io', () => ({
  stop: stopped,
  start: vi.fn(),
  continueRun: vi.fn(),
  closeTerminal: vi.fn(() => Promise.resolve()),
}));

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

/**
 * Zamknięcia karty, które przeszły granicę BEZ adresu — czyli `stop()` albo `stop(null)`.
 *
 * 2026-09 (Z-35, runda naprawcza): `toHaveBeenCalledWith` opisuje JEDNO wywołanie i milczy
 * o pozostałych, więc droga wołająca dodatkowo `stop(null)` przechodziła je bez słowa. Ta lista
 * sądzi wszystkie naraz. Brak adresu należy WYŁĄCZNIE do drogi zamykania okna
 * (`AppState::stop_every_live_run_before_closing`), a ta nie idzie przez `×`.
 */
function addressless(): unknown[] {
  return stopped.mock.calls.filter(([where]) => typeof where !== 'string' || where === '');
}

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
    /* 2026-09 (Z-35) — I MA TO ZROBIĆ Z ADRESEM. Do tego dnia stało tu `stop()` bez argumentu,
     * a tamta strona kończyła wtedy bieg w KAŻDYM żywym folderze: `×` na jednej karcie ubijał
     * pracę w drugim projekcie. Sama liczba wywołań tego nie odróżnia — jedno wywołanie bez
     * folderu i jedno z folderem wyglądają w niej identycznie. */
    expect(
      stopped,
      'the × of this card reached the boundary without saying WHICH folder to stop. A stop with ' +
        'no address ends every live run in the application, so closing one card kills the work ' +
        'going on in another project (invariant 6).',
    ).toHaveBeenCalledWith(HERE);
    expect(
      addressless(),
      'a close crossed the boundary without an address: ' + JSON.stringify(addressless()),
    ).toEqual([]);
  });

  /* 2026-09 (Z-35, runda naprawcza) — GAŁĄŹ, KTÓREJ NIE POKRYWAŁ ŻADEN PRZYPADEK WYŻEJ.
   *
   * `stopRunOf` miało drugą gałąź: kiedy własna sesja karty milczała, pytało sesję BEZ ZAKRESU
   * i wołało `stop(null)`. Póki `stop_run` nie brał folderu, `null` znaczyło „każdy żywy bieg"
   * i była to świadoma cena. Od tego zadania `null` znaczy katalog, pod którym wstała
   * aplikacja — czyli bieg, który do tej karty nie należy. Zamknięcie karty `atlas` kończyło
   * przez to pracę, o której ta karta nie wie: dokładnie ta wada, tylko o warstwę dalej.
   *
   * Przypadek wyżej tego nie łapie, bo tam sesja bez zakresu jest pusta i obie implementacje
   * milczą tak samo. */
  it('closing a quiet card says nothing, even when a run with no scope is going', async () => {
    cardForRun('Ship it', HERE);
    runTabs.getState().setAgents(HERE, 2);
    /* Bieg BEZ ZAKRESU: tak wygląda ten, który okno ZASTAJE w katalogu swojego startu
     * (`../index.tsx`, efekt przy `listRuns`). Nie należy do żadnej karty — karty nie ma. */
    runFor(null).getState().nowRunning('Left over', [], null);

    runTabs.getState().requestClose(HERE);
    await runTabs.getState().confirmClose();

    expect(
      stopped,
      'closing a card where nothing runs reached the boundary because a run with NO scope was ' +
        'going. That run lives in the folder the application started in, so this × stops work ' +
        'the card knows nothing about — the same defect as before, one screen further out.',
    ).not.toHaveBeenCalled();
    expect(
      runTabs.getState().tabs,
      'the card still has to come off the bar: nothing was running on it',
    ).toEqual([]);
  });
});
