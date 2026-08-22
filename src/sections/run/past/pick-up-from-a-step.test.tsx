/* „Kontynuuj stąd" jest PRZYCISKIEM w otwartym biegu i naprawdę wznawia ten bieg.
 *
 * 2026-08-23, pytanie właściciela nad ekranem historii: „a z history możemy kontynuować?".
 *
 * SŁABA WERSJA: `expect(pickUpFrom).toBeDefined()`. Przechodzi ją funkcja, do której nie prowadzi
 * ani jeden piksel — czyli dokładnie ta klasa wady, dla której w tym repo istnieje
 * `checks/quick-wired.sh`, tylko po stronie Reacta. Dlatego montowany jest CAŁY ekran sekcji
 * (`<Run />`), nigdy sam panel, a asercja pyta o markup.
 *
 * DRUGA SŁABA WERSJA, i ta wygląda na mocną: „granica została zawołana". Przechodzi ją wywołanie
 * z pustym krokiem albo z cudzym zakresem — a wtedy wznowienie ciągnie NIE TEN bieg i nie z tego
 * miejsca. Sądzone są więc ARGUMENTY, po jednym na każdą rzecz, bez której tamta strona nie wie,
 * co uruchomić.
 *
 * TRZECIA: sam fakt zawołania, bez pytania o to, co zostaje na ekranie. Panel jest modalem na
 * całą sekcję — zostawiony nad ruszonym biegiem zasłania jedyne miejsce, w którym widać, że
 * cokolwiek ruszyło.
 *
 * CZEGO TO KRYTERIUM NIE MIERZY, powiedziane wprost: samego `onClick`. To repo nie ma jsdom,
 * więc kliknięcia nie da się tu wywołać — zmierzone: podmiana ciała handlera na puste zostawia
 * ten plik ZIELONY. Łączy obie połowy `data-pick-up`: markup dowodzi, że przy każdym kroku stoi
 * przycisk WIEDZĄCY, którego kroku dotyczy, a wywołanie polityki dowodzi, co się z takim kluczem
 * dzieje. Sam gest sądzi `e2e/`, w prawdziwej przeglądarce — ta sama granica, którą nazywa
 * `./history-reaches-the-screen.test.tsx`.
 *
 * Granica jest atrapą: żadnego żywego Tauri i żadnej przeglądarki.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { PastRun, PastRunRow } from '../io';

const SHIP: PastRunRow = {
  folder: '20260816-194804__0198a1f2-3b4c-7d5e-8f60-000000000004',
  when: '2026-08-16 19:48',
  title: 'Ship a feature',
  state: 'failed',
  steps: 2,
  costUsd: 1,
  said: null,
};

/** Bieg, który PADŁ NA DRUGIM KROKU — czyli ten kształt, o który właściciel zapytał. */
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
      summary: '',
      error: 'The check would not run.',
      costUsd: 0.75,
      lines: [],
    },
  ],
  handoffs: [],
  said: null,
};

const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((_command: string, _args?: unknown): Promise<unknown> => Promise.resolve(null)),
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
const { PICK_UP_HERE, pickUpFrom } = await import('./pick-up');
const { useWorkspaces } = await import('../../../state/workspaces');

/** Zakres, w którym pracujemy. `id === folder` — kontrakt granicy z 2026-08-18. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };
useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });

function screen(): string {
  return renderToStaticMarkup(<Run />);
}

/** Nazwa znacznika, w którym stoi ten napis — pytanie „czy to jest kontrolka". */
function tagAround(markup: string, text: string): string {
  const at = markup.indexOf(text);
  if (at < 0) return '';
  const opens = markup.lastIndexOf('<', at);
  return /^<([a-z]+)/.exec(markup.slice(opens))?.[1] ?? '';
}

/** Argumenty ostatniego wywołania tej komendy — albo `undefined`, gdy nikt jej nie zawołał. */
function lastCallOf(command: string): Record<string, unknown> | undefined {
  const hit = invoked.mock.calls.filter((call) => call[0] === command).at(-1);
  return hit?.[1] as Record<string, unknown> | undefined;
}

// ── NAJPIERW STAN „NIC NIE JEST OTWARTE" ────────────────────────────────────────────────────
// Magazyn panelu żyje na poziomie modułu, więc ten stan da się zobaczyć raz i tylko tutaj.
closeHistory();
invoked.mockClear();
pickUpFrom('s_build');
const calledWithNothingOpen = invoked.mock.calls.length;

invoked.mockImplementation((command: string): Promise<unknown> => {
  if (command === 'list_runs') return Promise.resolve([SHIP]);
  if (command === 'read_run') return Promise.resolve(OPENED);
  return Promise.resolve(null);
});

await openHistoryFromLine('');
await openOneRun(HERE.folder, SHIP.folder);
const withTheRun = screen();

invoked.mockClear();
pickUpFrom('s_build');
const afterPressing = screen();

describe('a run in the history can be carried on from any of its steps', () => {
  it('draws the control on every step of the opened run, as a control and not a label', () => {
    expect(
      withTheRun,
      'the opened run carries no way to carry on. History that can only be read is history the ' +
        'owner asked to be able to continue from.',
    ).toContain(PICK_UP_HERE);
    expect(
      tagAround(withTheRun, PICK_UP_HERE),
      'it has to be a real button. A styled span looks identical and does nothing when pressed.',
    ).toBe('button');
    for (const step of OPENED.steps) {
      expect(
        withTheRun,
        'each button has to carry the key of ITS step. Without that the markup cannot tell one ' +
          'from another, and this criterion would pass for two buttons wired to the same step.',
      ).toContain('data-pick-up="' + step.id + '"');
    }
    expect(
      withTheRun.split(PICK_UP_HERE).length - 1,
      'one per step, because "from where" is a choice about the STEP. A single control over the ' +
        'whole run would have to guess that step — and guessing wrong means either repeating ' +
        'work that succeeded or skipping work that did not.',
    ).toBe(OPENED.steps.length);
  });

  it('starts the run again from that step, in the folder the list came from', () => {
    const args = lastCallOf('resume_run');
    expect(
      args,
      'pressing it called nothing at all. A control with no handler is the defect invariant 16 ' +
        'names by hand.',
    ).toBeDefined();
    expect(
      args?.['run'],
      'the folder of the run being carried on — without it the other side does not know which ' +
        'run to take the handoffs from',
    ).toBe(SHIP.folder);
    expect(
      args?.['step'],
      'and the step to pick up at. An empty one would restart the whole graph, which is the ' +
        'forty-eight minutes nobody wants to pay twice.',
    ).toBe('s_build');
    expect(
      args?.['folder'],
      'the scope comes from the list this run was read from, not from whatever the side menu ' +
        'says now — a person may switch workspaces between opening history and pressing this.',
    ).toBe(HERE.folder);
  });

  it('gets out of the way so the run it just started can be seen', () => {
    expect(
      afterPressing.includes('data-history'),
      'the panel covers the whole section. Left standing over the run it just started, it hides ' +
        'the one place where anything is visible.',
    ).toBe(false);
    expect(
      afterPressing.includes('data-feed'),
      'and the work screen has to be right there underneath',
    ).toBe(true);
  });

  it('reaches nothing at all when there is no open run to carry on from', () => {
    expect(
      calledWithNothingOpen,
      'a run started from nothing would take its input from whatever directory happened to sort ' +
        'last — and the person would be looking at somebody else\u2019s work under this run\u2019s name.',
    ).toBe(0);
  });
});
