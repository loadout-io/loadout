/* Z-46: co zostawiły biegi, których Loadout nie zamknął, DOCHODZI NA EKRAN — i ma wyjście.
 *
 * PO CO TO ISTNIEJE. Sprzątanie przy otwarciu folderu domyka wyłącznie te katalogi robocze,
 * o których bieg zostawił notatkę. Bieg sprzed tej notatki nie ma jej wcale, więc jego katalog
 * stoi dalej — i do dziś nic o nim nie mówiło. Zmierzone u właściciela 2026-09-03 na jednym
 * monorepo: dziennik zameldował 75 zamkniętych folderów, a `git worktree list` wymieniał dwanaście
 * stojących, po 264 MB; gałęzi po biegach, których katalogów już nie ma, było tam 99 przy
 * czternastu biegach. Człowiek nie miał ani jednego miejsca, w którym mógłby to zobaczyć.
 *
 * SŁABA WERSJA: `expect(pastNow().could?.workFolders).toBe(2)`. Przechodzi ją stan, do którego
 * nie prowadzi ani jeden piksel — magazyn zna liczby, ekran milczy. To jest dokładnie ta klasa,
 * dla której to repo powstało (niezmiennik 29), więc przedmiotem asercji jest MARKUP.
 *
 * DRUGA SŁABA WERSJA, gorsza, bo wygląda na mocną: zamontować sam panel (`<PastRuns />`).
 * Przechodziłaby na komponencie, którego ekran pracy nigdzie nie montuje. Montowany jest CAŁY
 * ekran sekcji (`<Run />`) i ani razu sam panel.
 *
 * TRZECIA: pokazać kontrolkę i zostawić ją bez skutku (niezmiennik 16). Rozstrzyga to, że
 * kryterium woła dokładnie tę funkcję, którą woła przycisk, i sprawdza, CO POJECHAŁO do Rusta.
 *
 * To repo nie ma jsdom, więc kliknięcia nie ma. Granica jest atrapą: żadnego żywego Tauri.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { CouldForget, Forgotten, PastRunRow } from '../io';

/** Zdanie, którym Rust mówi, ile tego jest. Liczby są jego, nie okna. */
const HOW_MUCH = 'This project has 2 work folders and 5 branches from runs Loadout did not close.';

/** Zdanie o tym, co zejdzie po dacie. */
const WOULD_GO = 'Forgetting them takes 3 runs away, and 4 branches and 1 work folder with them.';

/** I zdanie po naciśnięciu: co zeszło, a co zostało — po imieniu i ze ścieżką. */
const STAYED =
  'Loadout took 1 work folder and 4 branches away. It left the folder ' +
  '/Users/x/ledger-ui/.loadout/runs/20260801-090000__a2/work/s_3 where it is, because it still ' +
  'holds changes nobody saved.';

const COULD: CouldForget = {
  workFolders: 2,
  branches: 5,
  said: HOW_MUCH,
  older: { runs: 3, branches: 4, workFolders: 1, said: WOULD_GO },
};

const SWEPT: Forgotten = { workFolders: 1, branches: 4, runs: 0, said: STAYED };

function a_row(folder: string, title: string): PastRunRow {
  return {
    folder,
    when: '2026-08-23 01:12',
    title,
    workflowFile: 'ship-a-feature.json',
    state: 'succeeded',
    steps: 1,
    costUsd: 1.5,
    said: null,
  };
}

const ROWS = [
  a_row('20260823-011240__0198a1f2-3b4c-7d5e-8f60-000000000009', 'Ship a feature'),
  a_row('20260801-090000__0198a1f2-3b4c-7d5e-8f60-000000000002', 'Look around'),
];

const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((_command: string, _sent?: unknown): Promise<unknown> =>
    Promise.resolve(undefined),
  ),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const Run = (await import('../index')).default;
const { openHistoryFromLine } = await import('../history-command');
const { FORGET_AFTER_DAYS, askAboutRunsOlderThan, closeHistory, forgetTheLeftovers } =
  await import('./store');
const { FORGET_RUNS_OLDER_THAN, FORGET_THESE_RUNS, FORGET_THE_LEFTOVERS } = await import('./panel');
const { useWorkspaces } = await import('../../../state/workspaces');

/** Zakres, w którym pracujemy. `id === folder` — kontrakt granicy z 2026-08-18. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };
useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });

/** Markup tak, jak czyta go człowiek: React zapisuje cudzysłowy i `&` jako encje. */
function readable(markup: string): string {
  return markup
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
}

function screen(): string {
  return readable(renderToStaticMarkup(<Run />));
}

/** Nazwa znacznika, na którym wisi ten atrybut — pusty napis, kiedy atrybutu nie ma. */
function tagAround(markup: string, attribute: string): string {
  const at = markup.indexOf(attribute);
  if (at < 0) return '';
  const opens = markup.lastIndexOf('<', at);
  return /^<([a-z]+)/.exec(markup.slice(opens))?.[1] ?? '';
}

/** Folder z historią i z leżakami — stan, w którym pracuje większość tego pliku. */
function aFolderWithRuns(command: string): Promise<unknown> {
  if (command === 'list_runs') return Promise.resolve(ROWS);
  if (command === 'what_this_folder_could_forget') return Promise.resolve(COULD);
  if (command === 'forget_what_the_old_runs_left') return Promise.resolve(SWEPT);
  return Promise.resolve(undefined);
}

invoked.mockImplementation(aFolderWithRuns);

/* EKRAN, ZANIM KTOKOLWIEK O HISTORIĘ POPROSIŁ. Magazyn panelu żyje na poziomie modułu, więc ten
 * stan da się zobaczyć raz i tylko tutaj. */
const beforeAnything = screen();

await openHistoryFromLine('');
/* Podgląd jedzie osobnym wywołaniem, więc oddajemy pętli zdarzeń jedną turę: bez niej mierzyliśmy
 * ekran sprzed odpowiedzi Rusta, czyli stan, którego człowiek nie zdąży zobaczyć. */
await Promise.resolve();
const withTheList = screen();

invoked.mockClear();
await forgetTheLeftovers();
const afterSweeping = screen();
const askedRust = invoked.mock.calls.at(0);

closeHistory();

/* FOLDER, W KTÓRYM NIE MA ANI JEDNEGO BIEGU, A GAŁĘZIE PO NICH STOJĄ (2026-09, Z-46).
 *
 * To nie jest przypadek z brzegu, tylko stan, do którego prowadzi wszystko inne w tym pliku:
 * „Forget runs older than …" kasuje katalogi biegów, a gałąź biegu, którego katalogu już nie ma,
 * jest DOKŁADNIE tym, co zamiatacz ma sprzątać. Po dwóch takich czyszczeniach folder ma zero
 * wierszy historii i komplet osieroconych gałęzi — i to jest chwila, w której człowiek najbardziej
 * potrzebuje o nich usłyszeć. */
const ONLY_BRANCHES =
  'This project has 0 work folders and 5 branches from runs Loadout did not close.';
const NOTHING_OLDER = 'Nothing here is older than 30 days.';
const ORPHANS: CouldForget = {
  workFolders: 0,
  branches: 5,
  said: ONLY_BRANCHES,
  older: { runs: 0, branches: 0, workFolders: 0, said: NOTHING_OLDER },
};

invoked.mockImplementation((command: string): Promise<unknown> => {
  if (command === 'list_runs') return Promise.resolve([]);
  if (command === 'what_this_folder_could_forget') return Promise.resolve(ORPHANS);
  return Promise.resolve(undefined);
});
await openHistoryFromLine('');
await Promise.resolve();
const withOnlyBranches = screen();
closeHistory();

/* I z powrotem folder z historią: reszta tego pliku otwiera panel jeszcze raz. */
invoked.mockImplementation(aFolderWithRuns);

describe('history says what the runs Loadout did not close left behind', () => {
  it('says nothing about leftovers before anybody opens history', () => {
    expect(
      beforeAnything.includes(HOW_MUCH),
      'the work screen may not carry this sentence until somebody opens history. One standing ' +
        'there always would make every check below pass without the panel doing a thing.',
    ).toBe(false);
  });

  it('puts the real numbers on screen, in the sentence Rust wrote', () => {
    expect(
      withTheList,
      'the panel has to say how much of this there is, with the numbers Rust counted. Until this ' +
        'change nothing said it anywhere: the log said 75 folders were closed while twelve of ' +
        'them were still standing, and a terminal was the only way to find that out.',
    ).toContain(HOW_MUCH);
    expect(
      withTheList,
      'and it has to say what forgetting runs by date would take — folders, branches and work ' +
        'folders — because nothing may be deleted before a sentence says what goes',
    ).toContain(WOULD_GO);
  });

  it('offers both ways out, and both are controls a person can press', () => {
    expect(withTheList, 'the sweep needs a control of its own').toContain(FORGET_THE_LEFTOVERS);
    expect(
      tagAround(withTheList, 'data-forget-leftovers'),
      'and it has to be something a person can press. A caption that looks like a control and ' +
        'does nothing is worse than no control at all.',
    ).toBe('button');
    expect(
      withTheList,
      'and the date control has to name what it does, with the number of days beside it',
    ).toContain(FORGET_RUNS_OLDER_THAN);
    expect(
      tagAround(withTheList, 'data-older-than-days'),
      'the number of days has to be something a person can type into: hidden behind its own ' +
        'answer it could never be changed, so a folder with nothing older than 30 days would ' +
        'never offer any other question',
    ).toBe('input');
    expect(
      withTheList,
      'and the runs older than that need their own control, separate from the sweep: one control ' +
        'for both would mean somebody who wants disk space back loses history as well',
    ).toContain(FORGET_THESE_RUNS);
    expect(
      String(FORGET_AFTER_DAYS),
      'the default the control starts on has to be on screen too',
    ).not.toBe('');
    expect(withTheList).toContain('value="' + String(FORGET_AFTER_DAYS) + '"');
  });
});

describe('pressing the sweep really reaches Rust, and the answer lands where the person pressed', () => {
  it('asks Rust to forget what the old runs left, in THIS folder', () => {
    expect(
      askedRust,
      'nothing reached Rust at all, so the control is a picture of a control. This is the exact ' +
        'defect this file runs the edge instead of reading it.',
    ).toBeDefined();
    expect(askedRust?.at(0), 'and it has to ask for the one command that does this').toBe(
      'forget_what_the_old_runs_left',
    );
    const sent = (askedRust?.at(1) ?? {}) as Record<string, unknown>;
    expect(
      sent.folder,
      'the scope has to travel with the request. Rust sweeps under the folder it is given, so a ' +
        'request without one deletes in somebody else’s project.',
    ).toBe(HERE.folder);
  });

  it('shows what stayed behind, by name and by path', () => {
    expect(
      afterSweeping,
      'Rust leaves a folder holding changes nobody saved, and it says so. A screen that shows ' +
        'only "done" leaves the person pressing again over the same state, never learning why ' +
        'the numbers did not reach zero.',
    ).toContain(STAYED);
  });
});

describe('a folder whose runs are all gone still hears about their branches', () => {
  it('says how many branches are left when there is not one run to list', () => {
    expect(
      withOnlyBranches,
      'the panel hid everything it had to say because the history list was empty. That is the ' +
        'one folder where this sentence matters most: forgetting runs by date takes their ' +
        'folders, and a branch of a run whose folder is gone is exactly what the sweep exists ' +
        'to clear. A person who cleared their history is left with orphaned branches nothing ' +
        'ever mentions again.',
    ).toContain(ONLY_BRANCHES);
    expect(
      tagAround(withOnlyBranches, 'data-forget-leftovers'),
      'and the way out has to be there too, for the same reason: with no runs on the list this ' +
        'is the only control that can take those branches away',
    ).toBe('button');
  });

  it('drops only the control that has nothing to work on', () => {
    expect(
      withOnlyBranches.includes(FORGET_RUNS_OLDER_THAN),
      'forgetting runs by date has to go when there is not one run to forget: a control that can ' +
        'only ever answer "there was nothing" is a control without effect (invariant 16)',
    ).toBe(false);
    expect(
      withOnlyBranches.includes(FORGET_THESE_RUNS),
      'and so does the control that presses it',
    ).toBe(false);
  });
});

describe('changing the number of days asks about that number', () => {
  it('sends the number a person typed, not the one it started on', async () => {
    await openHistoryFromLine('');
    await Promise.resolve();
    invoked.mockClear();
    askAboutRunsOlderThan(7);
    await Promise.resolve();
    await Promise.resolve();

    const asked = invoked.mock.calls.find((call) => call.at(0) === 'what_this_folder_could_forget');
    expect(
      asked,
      'typing another number has to ask Rust again. A field that keeps its own value and never ' +
        'asks anything is a field that answers the first question for ever.',
    ).toBeDefined();
    const sent = (asked?.at(1) ?? {}) as Record<string, unknown>;
    expect(sent.olderThanDays, 'and it has to carry the number that was typed').toBe(7);
    closeHistory();
  });
});
