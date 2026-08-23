/* AC-5 dla T-94: obok „How many agents at once?" stoi „Spend at most $", a wpisana kwota
 * naprawdę dojeżdża do Rusta.
 *
 * ZMIERZONE, PO CO. 96-minutowy bieg właściciela kosztował ~$40 i nikt nie mógł powiedzieć
 * „stop po $20": jedynym limitem biegu były minuty, a minuty nie są ceną.
 *
 * SŁABA WERSJA TEGO KRYTERIUM renderuje kontrolkę i sprawdza, że jest w markupie. Przechodzi ją
 * pole, które przyjmuje liczbę i nigdzie jej nie wysyła — czyli dokładnie ta rodzina wad, dla
 * której powstało to repo: kontrolka obiecuje sterowanie i nic nie robi (niezmiennik 16).
 * Rozstrzyga więc wywołanie: co dostał `invoke`, pod jakim kluczem, przy obu drogach startu.
 *
 * Bez jsdom: `renderToStaticMarkup` z `react-dom/server` na markup, atrapa `@tauri-apps/api/core`
 * na drut. Kliknięcia w tym repo nie da się odpalić, więc kryterium woła to, co woła przycisk.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { invoked, release } = vi.hoisted(() => {
  const waiting: Array<() => void> = [];
  return {
    invoked: vi.fn(
      (..._sent: unknown[]) =>
        new Promise<undefined>((done) => {
          waiting.push(() => {
            done(undefined);
          });
        }),
    ),
    release: (): void => {
      while (waiting.length > 0) waiting.pop()?.();
    },
  };
});

/* Atrapa transportu. `Channel` jest tu, bo obie krawędzie zakładają go w oknie i podają jako
 * jeden z argumentów — atrapa musi umieć go oddać, inaczej mierzylibyśmy brak atrapy. */
vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { ask, start } = await import('../io');
const { Budget, BUDGET_HELP, BUDGET_LABEL } = await import('./budget');
const { budgetUsd, setBudgetUsd } = await import('./chosen');
const { Start, LIMIT_LOCKED } = await import('../start');
const { spendFor } = await import('../strip/model');
const { useRun } = await import('../../../state/run');

/** Ile człowiek pozwolił wydać na ten bieg. */
const CEILING = 20;

/** Plik, który udaje otwarty workflow. */
const OPEN = 'ship-a-feature.json';

/** Ile naraz — nie trójka: domyślną łatwo wpisać i nie zauważyć. */
const AT_ONCE = 5;

function noop(): void {
  // Handler jest wymagany, ale ta asercja nie pyta, co robi.
}

/** Sam znacznik pola z kwotą, wyjęty z markupu paska. */
function amountField(markup: string): string {
  return /<input[^>]*\bdata-budget\b[^>]*>/i.exec(markup)?.[0] ?? '';
}

/** Argumenty pierwszego wywołania, jako zwykła mapa. */
function carried(): Record<string, unknown> {
  const sent = invoked.mock.calls.at(0)?.at(1);
  return typeof sent === 'object' && sent !== null ? (sent as Record<string, unknown>) : {};
}

/** Wiersz „koniec tury" o zadanej cenie — dokładnie taki, jaki przychodzi z drutu. */
function turnCosting(costUsd: number | null): Parameters<typeof spendFor>[0][number] {
  return {
    kind: 'done',
    agent: 'Hand',
    text: '',
    turns: 1,
    durationMs: 252_000,
    costUsd,
    ended: 'well',
    /* 2026-08-24, przy scalaniu T-97: pola tokenowe doszły do wiersza „koniec tury" i są
     * wymagane, bo `Tokens` po stronie Rusta niesie trzy liczby, nie opcje — zero znaczy
     * „nic nie zgłoszono". Ten literał ma być tym, co NAPRAWDĘ przychodzi z drutu, więc
     * idzie za jego kształtem. Sam test mierzy koszt i nie pyta o tokeny. */
    inputTokens: 0,
    outputTokens: 0,
    cachedTokens: 0,
    id: 1,
    at: 0,
  };
}

describe('the ceiling a person puts on one run', () => {
  beforeEach(() => {
    invoked.mockClear();
    release();
    setBudgetUsd(null);
    useRun.getState().nowRunning('', []);
  });

  it('stands in the same row as the question about how many at once', () => {
    const markup = renderToStaticMarkup(<Start onSaid={noop} />);

    expect(
      markup,
      'the two limits are one decision taken twice, in two currencies: how much of the machine ' +
        'and how much money. A ceiling hidden in Settings is a decision taken once for runs ' +
        'that are never alike',
    ).toContain(BUDGET_LABEL);
    expect(markup, 'and the other half of that decision has to stay where it was').toContain(
      'How many agents at once?',
    );
  });

  it('opens empty, and empty means there is no ceiling', () => {
    expect(
      budgetUsd(),
      'nobody has typed anything, so this run is not capped. Zero would be a run that may ' +
        'never start, which is a state the control has no business being able to reach',
    ).toBeNull();
    expect(
      renderToStaticMarkup(<Budget onChange={noop} />),
      'an empty field shows nothing, not a number somebody has to notice and clear',
    ).toContain('value=""');
  });

  it('says out loud that Codex steps count as nothing', () => {
    expect(
      renderToStaticMarkup(<Budget value={CEILING} onChange={noop} />),
      'Codex does not report what a turn cost, so those steps add zero to the total. A person ' +
        'who is not told that reads the ceiling as a promise Loadout cannot keep',
    ).toContain(BUDGET_HELP);
  });

  it('sends what the person typed with a run from a file', async () => {
    setBudgetUsd(CEILING);
    const going = start(OPEN, AT_ONCE);

    expect(
      carried()['budgetUsd'],
      'the amount has to reach Rust under this exact name. Tauri matches arguments by name and ' +
        'reads them before the command body runs, so a value the window keeps to itself is a ' +
        'control that promises to stop the spending and does not',
    ).toBe(CEILING);

    release();
    await Promise.allSettled([going]);
  });

  it('sends the same ceiling when a single agent is asked', async () => {
    setBudgetUsd(CEILING);
    const going = ask({ id: 'agent-1', name: 'Hand' }, 'do the thing', AT_ONCE);

    expect(
      carried()['budgetUsd'],
      'one agent asked from the input row is an ordinary run, so it is capped by the same ' +
        'ceiling. A door that skips the ceiling is a door through which the money leaves',
    ).toBe(CEILING);

    release();
    await Promise.allSettled([going]);
  });

  it('sends nothing at all when the field was left empty', async () => {
    const going = start(OPEN, AT_ONCE);

    expect(
      carried()['budgetUsd'],
      'nobody capped this run, and the key still has to travel: Tauri reads arguments by name ' +
        'before the command body runs, so a missing key is not a smaller call — it is a ' +
        'rejected one',
    ).toBeNull();

    release();
    await Promise.allSettled([going]);
  });

  it('shows what is spent out of what was allowed', () => {
    const lines = [turnCosting(3.41)];

    expect(
      spendFor(lines, CEILING),
      'a run with a ceiling has to show both numbers together. $3.41 on its own answers "what ' +
        'did this cost" and leaves the only question a person with a ceiling actually asks — ' +
        'how close am I — to be worked out in their head',
    ).toContain('$3.41 of $20');
    expect(
      spendFor(lines),
      'and a run nobody capped shows the amount alone, because there is no second number to ' +
        'show',
    ).toContain('$3.41');
    expect(
      spendFor(lines),
      'a run nobody capped must not invent a ceiling to compare against',
    ).not.toContain(' of ');
  });

  it('cannot be changed while the run is going', () => {
    const idle = amountField(renderToStaticMarkup(<Start onSaid={noop} />));
    expect(
      idle,
      'the amount field has to be on the screen at all, or everything this case asks about is ' +
        'about an element that is not there',
    ).not.toBe('');
    expect(
      idle,
      'with nothing going the amount is the person’s to change; a control that is always ' +
        'dimmed is one they stop reading',
    ).not.toContain('disabled=');

    useRun.getState().nowRunning('Ship a feature', [], null, OPEN);
    const busy = amountField(renderToStaticMarkup(<Start onSaid={noop} />));

    expect(
      busy,
      'this run already started with the ceiling it was given, and no command changes it ' +
        'halfway through. A field that takes a new number and does nothing with it is worse ' +
        'than one that is dimmed',
    ).toContain('disabled=');
    expect(
      busy,
      'and the sentence saying why is the same one the slider beside it gives, because it is ' +
        'the same reason. Two sentences for one cause come apart on the day somebody fixes ' +
        'only one of them',
    ).toContain(LIMIT_LOCKED);
  });
});
