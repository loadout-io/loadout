/* `/history` naprawdę POKAZUJE historię — w markupie ekranu pracy, nie tylko w wartości zwróconej
 * z funkcji (niezmiennik 29).
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(await openHistoryFromLine('')).toBe(null)` plus
 * `expect(pastNow().rows).toHaveLength(3)`. Przechodzi ją stan, do którego nie prowadzi ani jeden
 * piksel: magazyn pełny, panel niezamontowany, ekran milczy. To jest dokładnie ta klasa, dla której
 * to repo powstało — kryterium zielone, funkcja martwa — i cztery razy złapał ją recenzent na
 * ZIELONEJ bramce (AGENTS.md, niezmiennik 29). Tamten stan mierzy `../history-command.test.ts`;
 * tutaj mierzymy MARKUP: ekran pracy przed poleceniem nie ma o historii ani słowa, po nim ma wiersz
 * na każdy bieg z dysku, a każdy z tych wierszy jest PRZYCISKIEM, nie napisem.
 *
 * DRUGA SŁABA WERSJA, GORSZA, BO WYGLĄDA NA MOCNĄ: zamontować sam panel (`<PastRuns />`).
 * Przechodziłaby na komponencie, którego ekran pracy nigdzie nie montuje — czyli na tej samej
 * wadzie, dla której istnieje `checks/quick-wired.sh`, tylko po stronie Reacta. Dlatego montowany
 * jest CAŁY ekran sekcji (`<Run />`) i ani razu sam panel.
 *
 * CO ZNACZY TU „WPISANIE /history". To repo nie ma jsdom, więc Enter jest dla kryterium
 * nieosiągalny — wołamy dokładnie to, co woła wiersz wejścia: `openHistoryFromLine`, czyli
 * wartość, którą `../entry/entry.tsx` podaje jako domyślną dla `onOpenHistory`. Że ta komenda
 * w ogóle stoi w wierszu, sprawdzamy osobno i tam, gdzie widzi ją człowiek: w zachęcie pustego
 * pola, którą ten sam ekran rysuje. Naciśnięcie klawisza sądzi `e2e/`, w prawdziwej przeglądarce.
 *
 * Granica jest atrapą: żadnego żywego Tauri i żadnej przeglądarki.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { PastRun, PastRunRow } from '../io';

/** Bieg, który da się przeczytać w całości. */
const SHIP: PastRunRow = {
  folder: '20260816-194804__0198a1f2-3b4c-7d5e-8f60-000000000004',
  when: '2026-08-16 19:48',
  title: 'Ship a feature',
  state: 'succeeded',
  steps: 2,
  costUsd: 1,
  said: null,
};

/** Bieg starszy, zatrzymany ręką. */
const LOOK: PastRunRow = {
  folder: '20260810-081500__0198a1f2-3b4c-7d5e-8f60-000000000001',
  when: '2026-08-10 08:15',
  title: 'Look around',
  state: 'cancelled',
  steps: 1,
  costUsd: null,
  said: null,
};

/** Zdanie, którym Rust mówi o biegu, którego opisu nie dało się przeczytać. */
const HONEST = 'Loadout could not read the record of this one, so all it can say is when it ran.';

/** Bieg, po którym został katalog i nic więcej. Ma być WIERSZEM, nie zniknięciem. */
const TORN: PastRunRow = {
  folder: '20260812-101112__0198a1f2-3b4c-7d5e-8f60-000000000002',
  when: '2026-08-12 10:11',
  title: '',
  state: '',
  steps: 0,
  costUsd: null,
  said: HONEST,
};

/** Zdanie, które krok po sobie zostawił — jedyny powód, dla którego się go otwiera. */
const SUMMARY = 'Stored the greeting in the file.';

/** Co przekazał pierwszy krok drugiemu. */
const HANDED = 'What we are building';

/** Wiersz zapisanego strumienia, w kształcie, który przyjeżdża z Rusta. */
const READ_LINE = {
  kind: 'read' as const,
  agent: 'Build',
  text: 'Read 3 files',
  count: 3,
  paths: ['src/auth.rs', 'src/login.rs', 'src/routes.rs'],
  detailId: null,
};

const OPENED: PastRun = {
  folder: SHIP.folder,
  when: SHIP.when,
  title: SHIP.title,
  state: SHIP.state,
  steps: [
    {
      id: 's_plan',
      name: 'Plan',
      agent: 'claude',
      state: 'succeeded',
      summary: 'Wrote the plan.',
      error: '',
      costUsd: 0.25,
      lines: [],
    },
    {
      id: 's_build',
      name: 'Build',
      agent: 'claude',
      state: 'failed',
      summary: SUMMARY,
      error: 'The check would not run.',
      costUsd: 0.75,
      lines: [READ_LINE],
    },
  ],
  handoffs: [{ from: 'Plan', to: ['Build'], title: HANDED, kind: 'plan' }],
  said: null,
};

/* Atrapa granicy oddaje `Promise<unknown>` JAWNIE, a nie z wnioskowania: bez adnotacji
 * `vi.fn` zamraża typ pierwszego ciała, a to niżej podmieniamy na takie, które oddaje bieg
 * z historii — i wtedy `quick-types` jest czerwone na teście, nie na kodzie. */
const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((_command: string): Promise<unknown> => Promise.resolve(undefined)),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const Run = (await import('../index')).default;
const { openHistoryFromLine, openOneRun } = await import('../history-command');
const { closeHistory } = await import('./store');
const { KNOWN, PROMPT, understand } = await import('../entry/entry');
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
 * Nazwa znacznika, na którym wisi wiersz tego biegu — albo pusty napis, gdy wiersza nie ma.
 *
 * Szukamy WSTECZ do ostatniego `<`, zamiast porównywać całą otwierającą klamrę: kolejność
 * atrybutów w markupie jest sprawą Reacta i tego, w jakiej kolejności napisano propsy, a
 * kryterium pytające o nią byłoby czerwone od przestawienia dwóch linii, które niczego nie
 * zmieniają. Pytanie brzmi „czy to jest kontrolka", a na nie odpowiada sama nazwa znacznika.
 */
function tagAround(markup: string, folder: string): string {
  const at = markup.indexOf('data-history-row="' + folder + '"');
  if (at < 0) return '';
  const opens = markup.lastIndexOf('<', at);
  return /^<([a-z]+)/.exec(markup.slice(opens))?.[1] ?? '';
}

/* EKRAN PRZED POLECENIEM. Magazyn panelu żyje na poziomie modułu, więc stan „nikt o historię nie
 * prosił" da się zobaczyć raz i tylko tutaj. */
const beforeAnything = screen();

invoked.mockImplementation((command: string): Promise<unknown> => {
  if (command === 'list_runs') return Promise.resolve([SHIP, TORN, LOOK]);
  if (command === 'read_run') return Promise.resolve(OPENED);
  return Promise.resolve(undefined);
});

const answered = await openHistoryFromLine('');
const withTheList = screen();

await openOneRun(HERE.folder, SHIP.folder);
const withTheRun = screen();

closeHistory();
const afterClosing = screen();

describe('the line offers /history, and a person can read it in the empty field', () => {
  it('names the command in the prompt the work screen really draws', () => {
    expect(
      KNOWN.some((one) => one.name === '/history'),
      'the line has one closed list of commands, and the prompt plus the "not known here" ' +
        'answer are both built from it. A command missing from that list is a command the line ' +
        'turns down.',
    ).toBe(true);
    expect(
      understand('/history'),
      'and typing it has to be understood, or the line answers a person with the list of ' +
        'commands it does know',
    ).toBe('/history');
    expect(
      PROMPT.includes('/history'),
      'the prompt is the only place a person meets these commands before typing one, so a ' +
        'command missing from it is a command nobody finds',
    ).toBe(true);
    expect(
      beforeAnything,
      'and that prompt has to be in the markup of the work screen itself, not only in a value ' +
        'the module exports. A command line drawn without it offers nothing.',
    ).toContain('/history past runs');
  });
});

describe('typing /history puts what really ran on the screen', () => {
  it('shows nothing about history before anybody asks for it', () => {
    expect(
      beforeAnything.includes('data-history'),
      'the work screen may not carry the history panel until somebody asks for it. A panel ' +
        'standing there always would make every check below pass without the command doing a ' +
        'thing.',
    ).toBe(false);
    expect(
      beforeAnything.includes('Ship a feature'),
      'and none of the runs may be on screen beforehand either, or the checks below would pass ' +
        'on a screen that says the same thing whatever happens',
    ).toBe(false);
  });

  it('answers the line with the panel instead of a sentence', () => {
    expect(
      answered,
      'the list stood up, so there is nothing left to say in the line — a sentence here would be ' +
        'a second answer to a question already answered on screen',
    ).toBe(null);
  });

  it('draws one row per run, each one a control a person can press', () => {
    expect(
      withTheList,
      'the panel that /history opens has to be in the markup of the work screen. A value in a ' +
        'store that nothing renders is the defect this repository exists to catch.',
    ).toContain('data-history');

    for (const run of [SHIP, TORN, LOOK]) {
      expect(
        withTheList,
        'every run Rust handed over has to have its own row, addressed by the folder it lives ' +
          'in — that address is what picking a row sends back. Missing: ' +
          run.folder,
      ).toContain('data-history-row="' + run.folder + '"');
      expect(
        withTheList,
        'and the row has to say when it ran, because that is the one thing every run has, ' +
          'readable or not: ' +
          run.when,
      ).toContain(run.when);
    }

    expect(
      withTheList,
      'the run whose record could not be read has to stand there with the honest sentence Rust ' +
        'wrote. A blank middle column looks exactly like a row that failed to draw, and this run ' +
        'really was here.',
    ).toContain(HONEST);

    const rows = withTheList.split('data-history-row=').length - 1;
    const pressable = [SHIP, TORN, LOOK].filter(
      (run) => tagAround(withTheList, run.folder) === 'button',
    );
    expect(
      pressable.length,
      'each of those rows has to be a real control a person can press: picking one run out of ' +
        'the list is the whole point of showing the list, and a row that only looks pressable ' +
        'is worse than a plain list. Tags found: ' +
        [SHIP, TORN, LOOK].map((run) => tagAround(withTheList, run.folder)).join(', '),
    ).toBe(3);
    expect(rows, 'and there have to be three of them, one per run on disk').toBe(3);
  });

  it('opens the picked run with its steps, what they said and what they passed on', () => {
    expect(
      withTheRun,
      'picking a row has to open that run, addressed by the folder the row carried',
    ).toContain('data-past-run="' + SHIP.folder + '"');
    expect(
      withTheRun,
      'the sentence a step left behind is the reason a person opens a finished run at all',
    ).toContain(SUMMARY);
    expect(
      withTheRun,
      'and the reason a step did not work has to be there too, or the screen shows a step that ' +
        'failed and no reason for it',
    ).toContain('The check would not run.');
    expect(
      withTheRun,
      'what one step handed to the next is the only way a result travels between them, and it ' +
        'is a file on disk. A run opened without them shows half of what happened.',
    ).toContain(HANDED);
    expect(
      withTheRun,
      'the stream that was kept for that step has to be drawn by the same row component the live ' +
        'view uses, counted the way the file counts it — anything else tells a different story ' +
        'than the screen told while it was going',
    ).toContain('Read 3 files');
    expect(
      withTheRun,
      'a step whose stream nobody kept has to say so. An empty space there is indistinguishable ' +
        'from a step that never said anything.',
    ).toContain('Nothing of what this step said was kept on disk.');
  });

  it('gives the screen back when the panel is closed', () => {
    expect(
      afterClosing.includes('data-history'),
      'the way out has to really take the panel down, or a person who opened history is stuck ' +
        'looking at it. The work screen underneath was never touched.',
    ).toBe(false);
    expect(
      afterClosing.includes('data-feed'),
      'and the work screen has to be right there underneath, exactly as it was',
    ).toBe(true);
  });
});
