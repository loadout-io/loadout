/* Historia pokazuje gałęzie, które bieg zostawił — i umie je zdjąć.
 *
 * PO CO TO ISTNIEJE. Po sprzątaniu z pierwszego kryterium tego zadania po biegu zostaje sama
 * gałąź: praca jest z niej osiągalna w całości, a katalog roboczy znika. Gałęzie zostają jednak
 * na zawsze — nic ich nie listuje i nic nie umie ich zdjąć poza ręcznym `git branch -D`. Po
 * tygodniu pracy `git branch` przestaje być do przeczytania, a gałęzie niosące coś ważnego giną
 * wśród kilkudziesięciu, o których nikt już nie pamięta.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(pastNow().opened?.branches).toHaveLength(2)`. Przechodzi
 * ją stan, do którego nie prowadzi ani jeden piksel — magazyn pełny, ekran milczy. To jest ta
 * sama klasa, dla której powstał sąsiedni plik `history-reaches-the-screen.test.tsx`, więc
 * przedmiotem asercji jest tutaj MARKUP EKRANU PRACY, a nie wartość w magazynie.
 *
 * DRUGA SŁABA WERSJA, GORSZA, BO WYGLĄDA NA MOCNĄ: zamontować sam panel (`<PastRuns />`).
 * Przechodziłaby na komponencie, którego ekran pracy nigdzie nie montuje. Dlatego montowany jest
 * CAŁY ekran sekcji (`<Run />`) i ani razu sam panel.
 *
 * TRZECIA SŁABA WERSJA: pokazać listę i przycisk, a zdejmowanie zostawić bez wołającego. Wtedy
 * kontrolka wygląda, mówi i nic nie robi (niezmiennik 16). Rozstrzyga to, że kryterium woła
 * dokładnie tę funkcję, którą woła przycisk, i sprawdza, CO POJECHAŁO do Rusta: nazwę komendy
 * i oba jej argumenty.
 *
 * To repo nie ma jsdom, więc kliknięcia nie ma. Granica jest atrapą: żadnego żywego Tauri
 * i żadnej przeglądarki.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { PastRun, PastRunRow } from '../io';

/** Napis na kontrolce. Słowo w słowo z kontraktu — to jest tekst, który człowiek naciska. */
const FORGET = 'Forget the branches';

/** Co stoi tam, gdzie stała lista, kiedy nie ma już czego zdejmować. */
const NONE_LEFT = 'No branches left';

/** Bieg, który zostawił po sobie dwie gałęzie. */
const KEPT_FOLDER = '20260823-011240__0198a1f2-3b4c-7d5e-8f60-000000000009';
const KEPT_ID = '0198a1f2-3b4c-7d5e-8f60-000000000009';

/** I bieg, który nie zostawił żadnej: wszystkie jego kroki niczego nie zmieniły. */
const BARE_FOLDER = '20260820-090000__0198a1f2-3b4c-7d5e-8f60-000000000003';

/** Nazwy gałęzi, składane tak, jak składa je Rust: `loadout/<bieg>/<kafelek>`. */
const BUILD_BRANCH = 'loadout/' + KEPT_ID + '/s_build';
const DOCS_BRANCH = 'loadout/' + KEPT_ID + '/s_docs';

const KEPT_ROW: PastRunRow = {
  folder: KEPT_FOLDER,
  when: '2026-08-23 01:12',
  title: 'Ship a feature',
  state: 'succeeded',
  steps: 2,
  costUsd: 1.5,
  said: null,
};

const BARE_ROW: PastRunRow = {
  folder: BARE_FOLDER,
  when: '2026-08-20 09:00',
  title: 'Look around',
  state: 'succeeded',
  steps: 1,
  costUsd: null,
  said: null,
};

const KEPT: PastRun = {
  folder: KEPT_FOLDER,
  when: KEPT_ROW.when,
  title: KEPT_ROW.title,
  state: KEPT_ROW.state,
  workflowFile: 'ship-a-feature.json',
  steps: [
    {
      id: '01a02b3c-15f5-7f13-a86f-f2f856e4d771',
      tile: 's_build',
      name: 'Build',
      agent: 'claude',
      state: 'succeeded',
      summary: 'Wrote the change.',
      error: '',
      costUsd: 1,
      lines: [],
    },
    {
      id: '01a02b3c-15f5-7f13-a86f-f2f856e4d772',
      tile: 's_docs',
      name: 'Docs',
      agent: 'claude',
      state: 'succeeded',
      summary: 'Wrote it down.',
      error: '',
      costUsd: 0.5,
      lines: [],
    },
  ],
  handoffs: [],
  branches: [
    { name: BUILD_BRANCH, step: 'Build' },
    { name: DOCS_BRANCH, step: 'Docs' },
  ],
  said: null,
};

const BARE: PastRun = {
  folder: BARE_FOLDER,
  when: BARE_ROW.when,
  title: BARE_ROW.title,
  state: BARE_ROW.state,
  workflowFile: 'look-around.json',
  steps: [
    {
      id: '01a02b3c-15f5-7f13-a86f-f2f856e4d773',
      tile: 's_look',
      name: 'Look',
      agent: 'claude',
      state: 'succeeded',
      summary: 'Read the code.',
      error: '',
      costUsd: null,
      lines: [],
    },
  ],
  handoffs: [],
  branches: [],
  said: null,
};

/* Atrapa granicy oddaje `Promise<unknown>` JAWNIE: bez adnotacji `vi.fn` zamraża typ pierwszego
 * ciała, a to niżej podmieniamy na takie, które oddaje bieg z historii. */
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
const { openHistoryFromLine, openOneRun } = await import('../history-command');
const { closeHistory, forgetTheBranches } = await import('./store');
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

/**
 * Nazwa znacznika, na którym wisi ten atrybut — albo pusty napis, gdy atrybutu nie ma.
 *
 * Szukamy WSTECZ do ostatniego `<`, zamiast porównywać całą otwierającą klamrę: kolejność
 * atrybutów w markupie jest sprawą Reacta, a kryterium pytające o nią byłoby czerwone od
 * przestawienia dwóch linii, które niczego nie zmieniają. Pytanie brzmi „czy to jest kontrolka",
 * a na nie odpowiada sama nazwa znacznika.
 */
function tagAround(markup: string, attribute: string): string {
  const at = markup.indexOf(attribute);
  if (at < 0) return '';
  const opens = markup.lastIndexOf('<', at);
  return /^<([a-z]+)/.exec(markup.slice(opens))?.[1] ?? '';
}

invoked.mockImplementation((command: string): Promise<unknown> => {
  if (command === 'list_runs') return Promise.resolve([KEPT_ROW, BARE_ROW]);
  if (command === 'read_run') return Promise.resolve(KEPT);
  if (command === 'forget_run_branches') return Promise.resolve([BUILD_BRANCH, DOCS_BRANCH]);
  return Promise.resolve(undefined);
});

/* EKRAN, ZANIM KTOKOLWIEK O HISTORIĘ POPROSIŁ. Magazyn panelu żyje na poziomie modułu, więc ten
 * stan da się zobaczyć raz i tylko tutaj — a bez niego każda asercja niżej przechodziłaby także
 * dla ekranu, który mówi to samo, cokolwiek się stanie. */
const beforeAnything = screen();

await openHistoryFromLine('');
await openOneRun(HERE.folder, KEPT_FOLDER);
const withBranches = screen();

invoked.mockClear();
await forgetTheBranches();
const afterForgetting = screen();
const askedRust = invoked.mock.calls.at(0);

invoked.mockImplementation((command: string): Promise<unknown> => {
  if (command === 'read_run') return Promise.resolve(BARE);
  return Promise.resolve(undefined);
});
await openOneRun(HERE.folder, BARE_FOLDER);
const withoutBranches = screen();

closeHistory();

describe('history says what a run left behind in git', () => {
  it('says nothing about branches before anybody opens a run', () => {
    expect(
      beforeAnything.includes(FORGET),
      'the work screen may not carry this control until somebody opens a run in history. One ' +
        'standing there always would make every check below pass without the panel doing a thing.',
    ).toBe(false);
    expect(
      beforeAnything.includes(BUILD_BRANCH),
      'and none of the branch names may be on screen beforehand either',
    ).toBe(false);
  });

  it('lists every branch the opened run left, by name and by the step that left it', () => {
    for (const branch of KEPT.branches ?? []) {
      expect(
        withBranches,
        'a branch this run left has to be on screen by name, because that name is the only thing ' +
          'a person can type into git to find the work again. Missing: ' +
          branch.name,
      ).toContain(branch.name);
      expect(
        withBranches,
        'and it has to say WHICH step left it, or the list is a column of near-identical names ' +
          'that differ by one word at the end. Missing: ' +
          branch.step,
      ).toContain(branch.step);
    }
  });

  it('offers one control to take them away, and it is a control, not a caption', () => {
    expect(
      withBranches,
      'the panel has to offer taking these branches away. Without it the only way out is a ' +
        'hand-typed `git branch -D` per name, and after a week of runs there are dozens.',
    ).toContain(FORGET);
    expect(
      tagAround(withBranches, 'data-forget-branches'),
      'and it has to be something a person can press. A caption that looks like a control and ' +
        'does nothing is worse than no control at all.',
    ).toBe('button');
  });
});

describe('pressing it really reaches Rust, and the panel says so afterwards', () => {
  it('asks Rust to forget the branches of THIS run, in THIS folder', () => {
    expect(
      askedRust,
      'nothing reached Rust at all, so the control is a picture of a control. This is the exact ' +
        'defect this file runs the edge instead of reading it.',
    ).toBeDefined();
    expect(askedRust?.at(0), 'and it has to ask for the one command that takes branches away').toBe(
      'forget_run_branches',
    );

    const sent = (askedRust?.at(1) ?? {}) as Record<string, unknown>;
    expect(
      sent.folder,
      'the scope has to travel with the request. Rust looks for the run under the folder it is ' +
        'given, so a request without one names branches in somebody else’s project.',
    ).toBe(HERE.folder);
    expect(
      sent.run,
      'and so does the address of the run, or Rust is asked to forget the branches of nothing ' +
        'in particular',
    ).toBe(KEPT_FOLDER);
  });

  it('says the branches are gone once Rust has answered', () => {
    expect(
      afterForgetting,
      'after Rust answered, the panel still shows the branches it just took away. A screen that ' +
        'contradicts what was done teaches a person not to trust it.',
    ).not.toContain(BUILD_BRANCH);
    expect(
      afterForgetting,
      'and it has to say so in words, because an empty space says nothing about whether the ' +
        'request worked',
    ).toContain(NONE_LEFT);
    expect(
      afterForgetting.includes(FORGET),
      'the control has to go with them: pressing it again would ask Rust to take away a list ' +
        'that is already empty',
    ).toBe(false);
  });
});

describe('a run that left nothing behind offers nothing to take away', () => {
  it('draws no control and says there is nothing left', () => {
    expect(
      withoutBranches,
      'this run is on screen, or the two checks below would pass on an empty panel',
    ).toContain('data-past-run="' + BARE_FOLDER + '"');
    expect(
      withoutBranches.includes(FORGET),
      'every step of this run changed nothing, so it left no branch and there is nothing to ' +
        'take away. A control that can only answer "there was nothing" is a control with no effect.',
    ).toBe(false);
    expect(
      withoutBranches,
      'and the panel still has to say where the list went, instead of leaving a gap that reads ' +
        'like a part that failed to draw',
    ).toContain(NONE_LEFT);
  });
});
