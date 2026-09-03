/* Historia umie zapomnieć CAŁY bieg — jego folder razem z gałęziami.
 *
 * PO CO TO ISTNIEJE. Gałęzie biegu dało się zdjąć od 2026-08-23 (plik obok). Jego folder — ze
 * strumieniami agentów, przekazaniami i kopiami notatek — nie schodził z dysku NICZYM: ani po
 * biegu, ani przy otwarciu folderu, ani przyciskiem. Zmierzone u właściciela 2026-09-02 na jednym
 * monorepo: 87 folderów biegów, 3,8 GB, a jedyną drogą był `rm -rf` z terminala.
 *
 * SŁABA WERSJA: `expect(pastNow().rows).toHaveLength(1)`. Przechodzi ją stan, do którego nie
 * prowadzi ani jeden piksel — magazyn posprzątany, ekran dalej pokazuje bieg, którego nie ma.
 * Dlatego przedmiotem asercji jest MARKUP ekranu pracy.
 *
 * DRUGA SŁABA WERSJA, gorsza, bo wygląda na mocną: zamontować sam panel (`<PastRuns />`).
 * Przechodziłaby na komponencie, którego ekran pracy nigdzie nie montuje. Montowany jest CAŁY
 * ekran sekcji (`<Run />`) i ani razu sam panel.
 *
 * TRZECIA: pokazać przycisk i zostawić go bez skutku (niezmiennik 16). Rozstrzyga to, że
 * kryterium woła dokładnie tę funkcję, którą woła przycisk, i sprawdza, CO POJECHAŁO do Rusta:
 * nazwę komendy i oba jej argumenty.
 *
 * CZWARTA, I TA JEST TU NAJWAŻNIEJSZA: zdjąć wiersz z listy także wtedy, gdy Rust ODMÓWIŁ.
 * Rust odmawia w całości — gałąź wyjęta do pracy w innym folderze zostawia i folder, i gałęzie —
 * więc ekran, który po odmowie kasuje wiersz, kłamie o stanie dysku. Drugi bieg w tej fikstrze
 * przechodzi dokładnie tę drogę.
 *
 * To repo nie ma jsdom, więc kliknięcia nie ma. Granica jest atrapą: żadnego żywego Tauri
 * i żadnej przeglądarki.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { PastRun, PastRunRow } from '../io';

/** Napis na kontrolce. Słowo w słowo z kontraktu — to jest tekst, który człowiek naciska. */
const FORGET = 'Forget this run';

/** Bieg, o którym człowiek każe zapomnieć. */
const GOING_FOLDER = '20260823-011240__0198a1f2-3b4c-7d5e-8f60-000000000009';
const GOING_TITLE = 'Ship a feature';

/** I bieg, którego gałąź ktoś ma w tej chwili otwartą do pracy: nie znika ani jedna rzecz. */
const BUSY_FOLDER = '20260820-090000__0198a1f2-3b4c-7d5e-8f60-000000000003';
const BUSY_TITLE = 'Look around';

/** Zdanie, którym odmawia Rust. Nazywa gałąź, bo bez niej człowiek nie wie, gdzie skończyć. */
const REFUSED =
  '"loadout/0198a1f2-3b4c-7d5e-8f60-000000000003/s_look" is checked out in another folder ' +
  'right now, so Loadout left every branch of this run alone. Finish there, then try again.';

function a_row(folder: string, title: string): PastRunRow {
  return {
    folder,
    when: '2026-08-23 01:12',
    title,
    state: 'succeeded',
    steps: 1,
    costUsd: 1.5,
    said: null,
  };
}

function a_run(folder: string, title: string): PastRun {
  return {
    folder,
    when: '2026-08-23 01:12',
    title,
    state: 'succeeded',
    workflowFile: 'ship-a-feature.json',
    steps: [
      {
        id: '01a02b3c-15f5-7f13-a86f-f2f856e4d771',
        tile: 's_look',
        name: 'Look',
        agent: 'claude',
        state: 'succeeded',
        summary: 'Read the code.',
        error: '',
        costUsd: 1.5,
        lines: [],
      },
    ],
    handoffs: [],
    branches: [],
    said: null,
  };
}

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
const { closeHistory, forgetThisRun } = await import('./store');
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

invoked.mockImplementation((command: string): Promise<unknown> => {
  if (command === 'list_runs') {
    return Promise.resolve([a_row(GOING_FOLDER, GOING_TITLE), a_row(BUSY_FOLDER, BUSY_TITLE)]);
  }
  if (command === 'read_run') return Promise.resolve(a_run(GOING_FOLDER, GOING_TITLE));
  if (command === 'forget_run') return Promise.resolve([]);
  return Promise.resolve(undefined);
});

/* EKRAN, ZANIM KTOKOLWIEK O HISTORIĘ POPROSIŁ. Magazyn panelu żyje na poziomie modułu, więc ten
 * stan da się zobaczyć raz i tylko tutaj. */
const beforeAnything = screen();

await openHistoryFromLine('');
await openOneRun(HERE.folder, GOING_FOLDER);
const withTheRunOpen = screen();

invoked.mockClear();
await forgetThisRun();
const afterForgetting = screen();
const askedRust = invoked.mock.calls.at(0);

/* DRUGI BIEG: Rust ODMAWIA w całości, bo jedna z gałęzi jest wyjęta do pracy. */
invoked.mockImplementation((command: string): Promise<unknown> => {
  if (command === 'read_run') return Promise.resolve(a_run(BUSY_FOLDER, BUSY_TITLE));
  if (command === 'forget_run') return Promise.reject(new Error(REFUSED));
  return Promise.resolve(undefined);
});
await openOneRun(HERE.folder, BUSY_FOLDER);
await forgetThisRun();
const afterTheRefusal = screen();

closeHistory();

describe('history offers one way out of a whole run', () => {
  it('says nothing about forgetting before anybody opens a run', () => {
    expect(
      beforeAnything.includes(FORGET),
      'the work screen may not carry this control until somebody opens a run in history. One ' +
        'standing there always would make every check below pass without the panel doing a thing.',
    ).toBe(false);
  });

  it('offers the control on an opened run, and it is a control, not a caption', () => {
    expect(
      withTheRunOpen,
      'the panel has to offer taking this whole run away. Until this change nothing in the app ' +
        'could: the folder of a run stayed on disk for good, and the owner had 87 of them.',
    ).toContain(FORGET);
    expect(
      tagAround(withTheRunOpen, 'data-forget-run'),
      'and it has to be something a person can press. A caption that looks like a control and ' +
        'does nothing is worse than no control at all.',
    ).toBe('button');
  });
});

describe('pressing it really reaches Rust, and the screen agrees afterwards', () => {
  it('asks Rust to forget THIS run, in THIS folder', () => {
    expect(
      askedRust,
      'nothing reached Rust at all, so the control is a picture of a control. This is the exact ' +
        'defect this file runs the edge instead of reading it.',
    ).toBeDefined();
    expect(
      askedRust?.at(0),
      'and it has to ask for the one command that takes a whole run away',
    ).toBe('forget_run');

    const sent = (askedRust?.at(1) ?? {}) as Record<string, unknown>;
    expect(
      sent.folder,
      'the scope has to travel with the request. Rust looks for the run under the folder it is ' +
        'given, so a request without one deletes in somebody else’s project.',
    ).toBe(HERE.folder);
    expect(
      sent.run,
      'and so does the address of the run, or Rust is asked to forget nothing in particular',
    ).toBe(GOING_FOLDER);
  });

  it('takes the run off the list once Rust has answered', () => {
    expect(
      afterForgetting,
      'the panel still shows the run it just took away. Its streams and handovers are no longer ' +
        'on disk, so every row of it opens on nothing.',
    ).not.toContain(GOING_TITLE);
    expect(
      afterForgetting.includes(FORGET),
      'and it has to leave the description of that run, because there is no run left to describe',
    ).toBe(false);
  });
});

describe('a refusal leaves everything exactly as it was', () => {
  it('keeps the run on screen and says which branch is in the way', () => {
    expect(
      afterTheRefusal,
      'Rust refused as a whole — it took neither the folder nor the branches — and the panel ' +
        'dropped the run anyway. The screen now says the run is gone while every byte of it is ' +
        'still on disk.',
    ).toContain(BUSY_TITLE);
    expect(
      afterTheRefusal,
      'and the refusal has to be on screen where the person pressed, naming the branch somebody ' +
        'is working on. A refusal only the console knows about leaves them pressing again.',
    ).toContain('is checked out in another folder');
  });
});
