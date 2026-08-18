/* RUN NIE OTWIERA JUŻ OKNA WYBORU KATALOGU — folder bierze się z aktywnego zakresu.
 *
 * ZMIERZONE 2026-08-18 PRZEZ WŁAŚCICIELA: nacisnął Run i dostał systemowe okno wyboru folderu.
 * Nazwał to „mega chujnia" i rozstrzygnął inaczej: projekt wybiera się RAZ, w bocznym menu.
 * Ten plik pilnuje trzech rzeczy, których nie widać w markupie, bo wszystkie trzy dzieją się
 * między kliknięciem a granicą.
 *
 * SŁABE WERSJE, i każda przechodzi na kodzie, który wymóg łamie:
 *   1. „`launchRun` bez zakresu oddaje zdanie" — przechodzi także wtedy, gdy zdanie przychodzi
 *      PO otwarciu okna wyboru i po anulowaniu go przez człowieka. Dokładnie tak było do dziś:
 *      `NO_FOLDER` istniało i było odpowiedzią na ANULOWANIE tego okna. Dlatego atrapa wtyczki
 *      jest tu podstawiona i asercja mówi, że nie została zawołana ANI RAZU.
 *   2. „`start` dostał czwarty argument" — przechodzi na kodzie, który wysyła cokolwiek
 *      niepustego. Sprawdzamy WARTOŚĆ: folder aktywnego zakresu, nie pierwszego z listy
 *      i nie tego, pod którym wstała aplikacja.
 *   3. Sam odczyt zdania odmowy — nie mówi nic o tym, czy bieg NIE ruszył. Odmowa, po której
 *      `run_workflow` i tak poleciał, jest gorsza niż brak odmowy.
 *
 * `./io` PODSTAWIAMY CAŁE, bo prawdziwy `start` zakłada kanał Tauri i nie wraca do końca biegu.
 * Sprawdzamy to, co ta polityka NAPRAWDĘ decyduje: czy wołać, z czym wołać i co powiedzieć.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { started, chosen } = vi.hoisted(() => ({
  started: vi.fn(() => Promise.resolve()),
  /* Okno wyboru folderu po stronie systemu. Ma tu NIE ZOSTAĆ ZAWOŁANE ani razu. */
  chosen: vi.fn(() => Promise.resolve('/Users/x/somewhere-else')),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: chosen }));
vi.mock('./io', () => ({ start: started, stop: vi.fn(), continueRun: vi.fn() }));

const { NO_FOLDER, launchRun } = await import('./launch');
const { runTabs } = await import('./tabs/store');
const { useWorkspaces } = await import('../../state/workspaces');

/** Zakres, w którym pracujemy. `id === folder` — kontrakt granicy z 2026-08-18. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };
/** Drugi zakres, żeby „bierze aktywny" dało się odróżnić od „bierze pierwszy z listy". */
const THERE = { id: '/Users/x/meetnotes', name: 'Notes', folder: '/Users/x/meetnotes' };

/** Workflow z dwoma krokami — czyli taki, którego da się uruchomić. */
const CHOICE = {
  path: 'ship.json',
  name: 'Ship it',
  steps: [
    { id: 's1', name: 'Build', state: 'pending' as const },
    { id: 's2', name: 'Review', state: 'pending' as const },
  ],
};

beforeEach(() => {
  started.mockClear();
  chosen.mockClear();
  useWorkspaces.setState({ all: [], activeId: null, said: null });
  runTabs.setState({ tabs: [], activeId: null, pendingClose: null });
});

describe('pressing Run takes the folder from the active workspace and never asks for one', () => {
  it('refuses without a workspace, names the way out, and starts nothing', async () => {
    const said = await launchRun(CHOICE, 3);

    expect(
      said,
      'a run with nowhere to work has to answer with a sentence. Silence after Run is the ' +
        'defect this whole wave started from.',
    ).toBe(NO_FOLDER);
    expect(
      chosen,
      'Run opened the system folder chooser. That is the exact thing the owner rejected on ' +
        '2026-08-18: choosing a project is a decision taken once, in the side menu, not a ' +
        'question asked every time somebody presses Run.',
    ).not.toHaveBeenCalled();
    expect(
      started,
      'the run started anyway. A refusal that still calls run_workflow sends agents into the ' +
        'directory the app happened to launch from — which is the worst possible answer to ' +
        '"where do we work".',
    ).not.toHaveBeenCalled();
    expect(
      /workspace/i.test(said ?? ''),
      'the refusal has to name the way out — the thing to add, in the words the side menu uses ' +
        '(DESIGN §8). "No folder is open" leaves a person exactly where they were. It says: ' +
        JSON.stringify(said),
    ).toBe(true);
  });

  it('sends the folder of the ACTIVE workspace, not the first one on the list', async () => {
    useWorkspaces.setState({ all: [THERE, HERE], activeId: HERE.id, said: null });

    const said = await launchRun(CHOICE, 4);

    expect(said, 'a run with a workspace and a workflow with steps has nothing to refuse').toBe(
      null,
    );
    expect(started, 'the run never reached the boundary').toHaveBeenCalledTimes(1);
    expect(
      started.mock.calls[0],
      'the boundary has to get the file name, the limit, the plan, the folder of the ' +
        'workspace a person is standing in, and the task. `THERE` first on the list is the trap: ' +
        'an implementation reading `all[0]` looks identical until two workspaces exist. The task ' +
        'is `null` here because Run was pressed rather than typed — `/run <workflow> <what to ' +
        'build>` is the caller that fills it, and a missing fifth argument would send `undefined` ' +
        'over a wire that matches arguments by name.',
    ).toEqual([CHOICE.path, 4, { name: CHOICE.name, steps: CHOICE.steps }, HERE.folder, null]);
    expect(
      chosen,
      'the folder chooser was opened even though a workspace was set',
    ).not.toHaveBeenCalled();
  });

  it('gives that run a card that names the workflow and the folder it works in', async () => {
    useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });

    await launchRun(CHOICE, 1);

    expect(
      runTabs.getState().tabs.map((tab) => [tab.name, tab.path]),
      'a run without a card cannot be seen and cannot be stopped with ×. The card is what the ' +
        'owner asked for: tabs inside the workspace are the RUNS there, named by their ' +
        'workflow, with the folder in the tooltip.',
    ).toEqual([[CHOICE.name, HERE.folder]]);

    /* Drugi bieg innego workflow w tym samym zakresie PRZEPISUJE nazwę karty, nie dokłada
     * drugiej: dwa biegi w jednym folderze pisałyby po tych samych plikach (niezmiennik 12),
     * więc karta z nazwą poprzedniego workflow byłaby zdaniem nieprawdziwym o żywym biegu. */
    await launchRun({ ...CHOICE, path: 'other.json', name: 'Fix it' }, 1);
    expect(
      runTabs.getState().tabs.map((tab) => tab.name),
      'the second run in the same workspace left the previous workflow name on the card. The ' +
        'card would then describe a run that is no longer going (invariant 17).',
    ).toEqual(['Fix it']);
  });
});
