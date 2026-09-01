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
const { Budget, BUDGET_LABEL } = await import('./budget');
const { budgetOfTheRun, budgetUsd, setBudgetUsd } = await import('./chosen');
const { launchRun } = await import('../launch');
const { defaultBudgetUsd } = await import('../../../state/settings');
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

/** Jeden bieg od początku do końca — tak, jak robi go przycisk, razem z odbiorem odpowiedzi. */
async function oneWholeRun(): Promise<void> {
  const going = start(OPEN, AT_ONCE);
  release();
  await Promise.allSettled([going]);
}

/** Workflow, który dostaje dostawa triggera. Ten sam kształt, co z `toChoices`. */
const DELIVERED = {
  path: 'ship-it.json',
  name: 'Ship it',
  steps: [{ id: 's_ship', name: 'Ship', state: 'pending' as const }],
};

/** Trwała dostawa triggera — z zamrożonym workspace, bo bez niego `launchRun` odmawia. */
const CLAIM = {
  slug: 'linear-mine',
  deliveryId: 'delivery-1',
  workflow: 'ship-it.json',
  runId: '0198a1f2-3b4c-7d5e-8f60-0000000000aa',
  workspace: '/Users/somebody/Projects/loadout-t208',
};

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
    /* 2026-08-29 (T-208) — STAN WEJŚCIOWY TO „NIKT NIC NIE POWIEDZIAŁ", nie „człowiek zdjął
     * sufit". Do tego dnia stało tu `setBudgetUsd(null)` i te dwa zdania były jednym stanem,
     * więc dwa przypadki niżej pinowały zachowanie, którego produkt już nie ma. */
    setBudgetUsd(undefined);
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

  /* 2026-08-29 (T-208) — TEN PRZYPADEK PINOWAŁ ODWROTNE ZDANIE i był na to zdanie zielony:
   * „otwiera się puste, a puste znaczy bez sufitu". Puste pole było wtedy stanem POCZĄTKOWYM,
   * więc bieg, przy którym nikt nie pomyślał o pieniądzach, leciał bez ograniczenia — a „nikt
   * nie pomyślał" jest stanem domyślnym, nie wyjątkiem. Zmierzone koszty prawdziwych biegów
   * właściciela z fazy 8: od $11 do $67,78, a jeden bieg przerwał limit konta, nie aplikacja.
   * Roszczenie jest więc dziś inne, a nie słabsze: puste pole nadal znaczy „bez sufitu", tylko
   * nie da się w nie wpaść przez zapomnienie. */
  it('opens at the ceiling Settings remembers, so nothing starts uncapped by accident', () => {
    expect(
      defaultBudgetUsd(),
      'the window has no ceiling to offer at all, so a strip nobody typed into is a run nobody ' +
        'capped. Zero would be no better: a run allowed to spend nothing may never start',
    ).toBeGreaterThan(0);
    expect(
      budgetUsd(),
      'nobody has typed anything into the strip, and this is the state almost every run starts ' +
        'in. It has to take the amount a person set once in Settings — a ceiling that only ' +
        'exists when somebody remembers to type it is a ceiling most runs do not have',
    ).toBe(defaultBudgetUsd());
    expect(
      renderToStaticMarkup(<Budget value={budgetUsd()} onChange={noop} />),
      'and the field shows that amount instead of standing empty. An empty field makes a run ' +
        'nobody capped look exactly like a run whose amount has not been typed yet',
    ).toContain('value="' + String(defaultBudgetUsd()) + '"');
  });

  /* 2026-08-31 — TEN PRZYPADEK PYTAŁ O DYMEK I BYŁ NA DYMKU ZIELONY. Brzmiał „says out loud
   * that Codex steps count as nothing" i przechodził na `title="…"`, czyli na tekście, który
   * pojawia się po sekundzie trzymania myszy w bezruchu i nie istnieje ani dla klawiatury, ani
   * dla czytnika ekranu, ani na dotyku. Dokładnie ta rodzina, dla której powstał niezmiennik 29:
   * kryterium zielone, zdanie niewidzialne. Zdanie stoi dziś na ekranie Settings, pod kontrolką,
   * którą ono kwalifikuje, i sądzi je `sections/settings/the-spend-limit-says-what-it-misses`.
   *
   * Tutaj zostaje druga połowa tamtej naprawy, bo dotyczy TEGO pola: dymek niesie wyłącznie
   * powód wygaszenia i nic poza nim. Zdanie schowane pod kursorem czynnej kontrolki jest
   * zdaniem, którego połowa ludzi nie zobaczy nigdy. */
  it('hides nothing under the cursor while the amount can still be changed', () => {
    expect(
      renderToStaticMarkup(<Budget value={CEILING} onChange={noop} />),
      'the live amount field carries a tooltip again. Whatever it says is said to nobody using ' +
        'a keyboard, a screen reader or a touch screen, so it cannot be the only place a fact ' +
        'about this ceiling lives',
    ).not.toContain('title=');
  });

  it('keeps the reason it cannot be changed under the cursor, where the same reason already is', () => {
    expect(
      renderToStaticMarkup(<Budget value={CEILING} onChange={noop} disabled={LIMIT_LOCKED} />),
      'a field greyed out for no stated reason is a riddle. This one reason may live in a ' +
        'tooltip because the same sentence already stands beside the control next to it, and ' +
        'one fact gets one place on a screen',
    ).toContain(`title="${LIMIT_LOCKED}"`);
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

  /* 2026-08-29 (T-208) — DRUGI PRZYPADEK Z ODWRÓCONYM ROSZCZENIEM. Stało tu „wysyła nic, kiedy
   * pole zostawiono puste", i przechodziło WYŁĄCZNIE dlatego, że `beforeEach` ustawiał wtedy
   * `setBudgetUsd(null)` — czyli mierzyło stan, w który dziś nie da się wejść inaczej niż jawnym
   * ruchem człowieka. Oba zdania są tu teraz obok siebie, bo dopiero razem mówią, co się
   * zmieniło: bez wpisu jedzie sufit z Settings, a `null` dociera do Rusta dopiero po
   * wyczyszczeniu pola. */
  it('sends the ceiling from Settings with a run nobody typed a number into', async () => {
    const going = start(OPEN, AT_ONCE);

    expect(
      carried()['budgetUsd'],
      'a run started without touching the amount field reached Rust uncapped. That is the ' +
        'silent uncapped run this task removes, and it was the ordinary case: the strip opens ' +
        'this way for every run nobody thought about',
    ).toBe(defaultBudgetUsd());

    release();
    await Promise.allSettled([going]);
  });

  it('sends nothing at all once a person clears the field', async () => {
    setBudgetUsd(null);
    const going = start(OPEN, AT_ONCE);

    expect(
      carried()['budgetUsd'],
      'clearing the field is the one way a person says "do not cap this run", and it has to ' +
        'still work. The key travels even so: Tauri reads arguments by name before the command ' +
        'body runs, so a missing key is not a smaller call — it is a rejected one',
    ).toBeNull();

    release();
    await Promise.allSettled([going]);
  });

  /* 2026-08-29, DRUGA POPRAWKA — TO JEST TA ASERCJA, KTÓREJ TU BRAKOWAŁO. Pierwsza wersja
   * zdejmowała sufit RAZ I NA ZAWSZE: nadpisanie z paska nie było nigdy zdejmowane, więc każdy
   * następny Start też szedł bez ograniczenia i nikt tego nie zamawiał. Przypadki wyżej tego nie
   * widziały, bo każdy z nich puszcza dokładnie jeden bieg — a wada zaczyna się przy drugim. */
  it('gives the next run the ceiling from Settings again, because taking it off was for one run', async () => {
    setBudgetUsd(null);
    await oneWholeRun();

    expect(
      budgetUsd(),
      'the ceiling a person took off one run stayed off. Every later run then goes out uncapped ' +
        'without anybody asking for it, which is the defect this task exists to remove — only ' +
        'moved one run further along',
    ).toBe(defaultBudgetUsd());

    invoked.mockClear();
    await oneWholeRun();

    expect(
      carried()['budgetUsd'],
      'and the run after it reached Rust uncapped. A limit spent once has to come back: the ' +
        'strip says what THIS run may cost, not what every run from now on may cost',
    ).toBe(defaultBudgetUsd());
  });

  it('spends a typed amount on one run and does not charge it to the next', async () => {
    setBudgetUsd(CEILING);
    await oneWholeRun();

    expect(
      budgetOfTheRun(),
      'the run that just went has to keep the amount it was given, or the chip above its own ' +
        'lines reads "$3.41 of $75" over a run that was capped at $20',
    ).toBe(CEILING);
    expect(
      budgetUsd(),
      'and the next run goes back to what Settings remembers. A number typed for one run that ' +
        'silently rules every later one is the same defect as a ceiling taken off for good',
    ).toBe(defaultBudgetUsd());
  });

  /* Droga produkcyjna triggera, wołana dokładnie tak, jak woła ją obserwator w
   * `src/state/triggers.ts`: `launchRun(choice, atOnce, task, claim)`. */
  it('gives a run delivered by a trigger the ceiling from Settings, not what the strip holds', async () => {
    setBudgetUsd(null);
    const going = launchRun(DELIVERED, AT_ONCE, 'LOAD-1: do the work', CLAIM);

    expect(
      carried()['budgetUsd'],
      'a run started by a trigger delivery took the amount sitting in the run strip. Nobody is ' +
        'at the keyboard when an issue arrives at night, so a ceiling a person took off their ' +
        'own next run must not travel to work they never saw',
    ).toBe(defaultBudgetUsd());

    release();
    await Promise.allSettled([going]);

    expect(
      budgetUsd(),
      'and the trigger ate the amount the person had typed for their own next run. The strip is ' +
        'about the run a person is about to start, and a trigger firing in the background is ' +
        'not that run',
    ).toBeNull();
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
