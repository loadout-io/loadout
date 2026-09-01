/* Pozycje kafelków i strzałki DOCHODZĄ z pliku workflow do magazynu biegu.
 *
 * PO CO TO ISTNIEJE. Reguła 17 mówi, że UI nie rysuje relacji, których nie ma w danych. Rysunek
 * biegu jest więc legalny wyłącznie wtedy, kiedy pozycja kafelka i strzałka „po" przyjechały
 * z pliku workflow — a do 2026-08-31 nie przyjeżdżały wcale: `planOf` przepisywało z pliku
 * `id`, `name`, `state` i `kind`, gubiąc `at`, a `WhatIsRunning` nie miało pola na strzałki
 * w ogóle. Każdy rysunek zbudowany na tym stanie musiałby współrzędne WYMYŚLIĆ, a wymyślona
 * współrzędna jest dokładnie tą ozdobną krzywą, której reguła 17 zakazuje.
 *
 * SŁABĄ WERSJĄ tego kryterium jest zawołanie `planOf` i obejrzenie zwróconej tablicy. Taka
 * wersja przechodzi nad wadą, dla której to repo powstało (niezmiennik 29): funkcja umie
 * przepisać pole, a do magazynu biegu ono nie dociera, bo droga urywa się jedno wywołanie dalej.
 * Dlatego niżej biegnie CAŁA produkcyjna droga, ta sama, którą idzie przycisk Run:
 * plik workflow → `toChoices` → `launchRun` → `start` → `nowRunning` → magazyn biegu.
 * Czytamy magazyn, nie wartość zwróconą przez którąkolwiek z tych funkcji.
 *
 * DRUGI PRZYPADEK JEST RÓWNIE WAŻNY. Plan jednego kroku, który okno składa samo dla `/ask`,
 * nie ma ani pozycji, ani strzałek i mieć ich nie może — klucz kroku rodzi się po tamtej
 * stronie granicy. „Nie wiemy" musi być odróżnialne od „ten workflow nie ma ani jednej
 * strzałki", bo pierwsze każe rysunkowi MILCZEĆ, a drugie jest faktem o pliku. Implementacja,
 * która oba przypadki oddaje jako pustą listę, zamienia brak wiedzy w twierdzenie.
 *
 * `@tauri-apps/api/core` jest podmieniony atrapą, tak samo jak w `start-invokes.test.tsx`:
 * atrapa nie rozwiązuje się sama, więc magazyn stoi na stanie „bieg trwa" dokładnie tak długo,
 * ile trzeba, żeby go przeczytać. Po `release()` wraca stan sprzed startu (`whatWasRunning`).
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { runFor } from '../../state/run';
import type { Link, WorkflowFile } from '../../state/workflows';
import { useWorkspaces } from '../../state/workspaces';
import { freshStep } from '../workflows/canvas/connect';
import { toChoices } from './choices';
import { ask } from './io';
import { launchRun } from './launch';
import { runTabs } from './tabs/store';

const { invoked, release } = vi.hoisted(() => {
  const waiting: Array<() => void> = [];
  return {
    invoked: vi.fn(
      (..._sent: unknown[]) =>
        new Promise<undefined>((resolve) => {
          waiting.push(() => {
            resolve(undefined);
          });
        }),
    ),
    release: (): void => {
      while (waiting.length > 0) {
        waiting.pop()?.();
      }
    },
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

/** Zakres, w którym pracujemy. `id === folder` — kontrakt granicy z 2026-08-18. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };

/* Dwie pozycje, obie różne i żadna nie jest domyślną z płótna: implementacja wpisująca cokolwiek
 * na stałe wygląda identycznie, dopóki obie liczby nie są rozpoznawalne. */
const WRITE_AT = { x: 48, y: 24 };
const REVIEW_AT = { x: 240, y: 168 };

/* Zwykłe „po" i POWRÓT z sufitem tur — dwa różne rodzaje strzałki, żeby przewóz gubiący
 * `max_turns` nie przeszedł. Klucz nazywa się `max_turns`, nie `maxTurns`: `workflow::Link`
 * po stronie Rusta nie ma `rename_all` (`src/state/workflows.ts`). */
const RUNS_AFTER: readonly Link[] = [
  { from: 'write', to: 'review' },
  { from: 'review', to: 'write', max_turns: 3 },
];

/** Plik workflow o dwóch kafelkach na znanych pozycjach i o podanych strzałkach. */
function fileWith(links: readonly Link[]): WorkflowFile {
  return {
    format: 1,
    id: 'ship',
    name: 'Ship it',
    /* `freshStep` — ta sama fabryka, którą płótno stawia kafelki. Krok napisany tu ręcznie
     * obchodziłby dokładnie tę krawędź, którą to kryterium sądzi. */
    steps: [freshStep('agent', 'write', WRITE_AT), freshStep('agent', 'review', REVIEW_AT)],
    links: [...links],
  };
}

/** Naciśnięcie Run na tym pliku — produkcyjną drogą, bez ani jednego skrótu. */
function pressRun(file: WorkflowFile): Promise<string | null> {
  const [choice] = toChoices([{ path: 'ship.json', workflow: file }]);
  if (choice === undefined) throw new Error('the list of workflows came back empty');
  return launchRun(choice, 2);
}

/** Magazyn biegu tego zakresu, w chwili, w której bieg jeszcze trwa. */
function whileItRuns(): ReturnType<ReturnType<typeof runFor>['getState']> {
  return runFor(HERE.folder).getState();
}

beforeEach(() => {
  invoked.mockClear();
  release();
  useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });
  runTabs.setState({ tabs: [], activeId: null, pendingClose: null });
  runFor(HERE.folder).getState().nowRunning('', []);
});

describe('positions and runs-after lines reach the run from the workflow file', () => {
  it('carries every tile position and every runs-after line into the run', async () => {
    const going = pressRun(fileWith(RUNS_AFTER));
    const now = whileItRuns();

    expect(
      now.steps.map((step) => step.at),
      'the run started without the tile positions the workflow file wrote down. The plan ' +
        'reached the run with names and states only, so anything that draws this run has to ' +
        'invent where each step belongs — and an invented coordinate is a picture of ' +
        'something the data never said (invariant 17). What arrived was ' +
        JSON.stringify(now.steps),
    ).toEqual([WRITE_AT, REVIEW_AT]);

    expect(
      now.links,
      'the run started with no runs-after lines at all. The file says which step follows ' +
        'which, and that answer stopped at the boundary: the run knows the list of steps and ' +
        'nothing about the order between them, so a drawing of it can only lay them in a row ' +
        'and call that the shape of the work. It carried ' +
        JSON.stringify(now.links),
    ).toEqual(RUNS_AFTER);

    release();
    await Promise.allSettled([going]);
  });

  it('keeps we-do-not-know apart from this-file-has-none', async () => {
    const asking = ask({ id: 'reviewer', name: 'Reviewer' }, 'look at it', 1, HERE.folder);
    const madeHere = whileItRuns().links;

    release();
    await Promise.allSettled([asking]);

    const going = pressRun(fileWith([]));
    const fromTheFile = whileItRuns().links;

    release();
    await Promise.allSettled([going]);

    expect(
      madeHere,
      'a plan the window built itself for one agent came out claiming this run HAS no ' +
        'runs-after lines. Nobody wrote any down, which is a different answer: the window ' +
        'never read a file for it, so the honest reply is that we do not know. It said ' +
        JSON.stringify(madeHere),
    ).toBe(null);

    expect(
      fromTheFile,
      'a workflow file whose list of runs-after lines is empty came out as we-do-not-know. ' +
        'That file DID say something — it says there are none — and a run that cannot tell ' +
        'the two apart reports one answer for two different truths, which is how a drawing ' +
        'ends up stating as fact something nobody ever wrote. It said ' +
        JSON.stringify(fromTheFile),
    ).toEqual([]);
  });
});
