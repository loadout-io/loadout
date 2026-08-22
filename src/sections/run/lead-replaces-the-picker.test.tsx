/* AC-4 dla T-60: pasek niesie lidera, a zielony `Run` z edytora dalej ma odbiorcę.
 *
 * DWA PYTANIA, DWIE DROGI, I TO NIE JEST PRZYPADEK. Markup mówi, czy kontrolka JEST; krawędź
 * mówi, dokąd sięga. Ten sam podział, co w `stop-becomes-reachable.test.tsx`, i z tego samego
 * powodu: to repo NIE MA jsdom, więc kliknięcia nie da się odpalić, a dopisanie
 * `@testing-library/react` byłoby zmianą `package.json`, czyli momentem na zatrzymanie się
 * i zapytanie człowieka (AGENTS.md §7). Klikamy więc to, co klika kontrolka — funkcję.
 *
 * SŁABA WERSJA TEGO KRYTERIUM jest jedna i jest kusząca: sprawdzić tylko, że lista
 * `aria-label="Workflow to run"` zniknęła. Przechodzi dla zmiany, która kasuje listę i nie stawia
 * w jej miejsce niczego — a wtedy zielony `Run` w edytorze przestaje cokolwiek robić (jego jedynym
 * konsumentem był `useEffect` w tej właśnie kontrolce), ekran pracy traci jedyną mysią drogę do
 * biegu, i dowiaduje się o tym człowiek. Dlatego punkty (b) i (c) stoją obok (a), a (e) pilnuje,
 * żeby (a) nie przeszło na pustym markupie: „nie ma tam listy" jest prawdą o każdym pustym ekranie.
 *
 * `./launch` PODSTAWIAMY CAŁE, bo prawdziwy `launchRun` sięga po zakres, zakłada kanał Tauri
 * i nie wraca do końca biegu. Sprawdzamy to, co ten moduł NAPRAWDĘ decyduje: czy zawołać, z czym
 * zawołać i czy nie zawołać drugi raz.
 *
 * WARTOŚCI OCZEKIWANE SĄ CZYTANE, NIE WPISANE: etykieta kontrolki lidera przychodzi z
 * `./lead.ts`. Wpisana z palca po obu stronach byłaby zielona także wtedy, gdyby kontrolka i ten
 * plik mówiły o dwóch różnych rzeczach.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Choice } from './choices';

/* Atrapa polityki startu. Rozwiązuje się natychmiast: pytanie „czy zawołano drugi raz" dotyczy
 * zapadki w module, a nie tego, jak długo trwa bieg — tę drugą rzecz sądzi cudze kryterium
 * (`start-invokes.test.tsx`). */
const { launched } = vi.hoisted(() => ({
  launched: vi.fn((..._sent: unknown[]) => Promise.resolve<string | null>(null)),
}));

vi.mock('./launch', () => ({ launchRun: launched }));

/* Transport. Kontrolka biegu i tak go nie dotknie w renderze statycznym — `useEffect` się nie
 * uruchamia — ale moduły po drodze importują go przy wczytaniu, a prawdziwy woła okno, którego
 * tu nie ma. `Channel` musi umieć powstać, bo krawędź startu zakłada go w konstruktorze. */
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => new Promise(() => undefined)),
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { Start, WorkflowRunButton } = await import('./start');
const { LEAD_LABEL } = await import('./lead');
const { launchRequested } = await import('./requested-launch');
const { requestRun, requestedRun, takeRequestedRun } = await import('./requested');

/** Etykieta listy wyboru workflow — ta, która ma z paska zniknąć. */
const PICKER = 'Workflow to run';

/** Nazwa pliku, o którą prosi edytor. To ona jedzie do Rusta. */
const OPEN = 'ship-a-feature.json';

/** „Ile naraz". Nie trójka: domyślną łatwo wpisać na sztywno i nie zauważyć. */
const AT_ONCE = 5;

/**
 * Co leży w katalogu workflow. DWIE pozycje, i żądana jest drugą — implementacja biorąca
 * „pierwszą z listy" wygląda identycznie, dopóki lista ma jedną pozycję.
 */
const CHOICES: readonly Choice[] = [
  {
    path: 'other.json',
    name: 'Something else',
    steps: [{ id: 'a', name: 'Look around', state: 'pending' as const }],
  },
  {
    path: OPEN,
    name: 'Ship a feature',
    steps: [{ id: 's1', name: 'Write the code', state: 'pending' as const }],
  },
];

/** Kontrolki biegu wyrenderowane statycznie. */
function markup(): string {
  return renderToStaticMarkup(
    <Start
      onSaid={() => {
        /* To kryterium nie pyta o zdanie odmowy — pyta o kontrolki. Kanał raportowania musi
           jednak istnieć, bo typ go wymaga, i to jest celowe: prop wymagany jest sposobem, w jaki
           system typów pilnuje, żeby zdanie miało gdzie stanąć. */
      }}
    />,
  );
}

/**
 * Wszystkie dostępne nazwy w markupie, w kolejności wystąpienia.
 *
 * Nazwy, a nie dowolne wystąpienie napisu: `rendered.includes('Lead agent')` byłoby zielone od
 * podpowiedzi, od nazwy w tekście i od komunikatu — a pytanie dotyczy KONTROLKI, którą da się
 * znaleźć bez patrzenia na ekran.
 */
function accessibleNames(rendered: string): readonly string[] {
  return [...rendered.matchAll(/aria-label="([^"]*)"/g)].map((hit) => hit[1] ?? '');
}

/** Napisy na wszystkich przyciskach markupu — dowód, że grupa kontrolek w ogóle się narysowała. */
function buttonLabels(rendered: string): readonly string[] {
  return [...rendered.matchAll(/<button\b[^>]*>([^<]*)<\/button>/g)].map((hit) =>
    (hit[1] ?? '').trim(),
  );
}

/** Nazwa pliku workflow wyjęta z tego, co dostała atrapa polityki. `''` = nie dostała pozycji. */
function pathOf(value: unknown): string {
  if (typeof value !== 'object' || value === null || !('path' in value)) return '';
  const path: unknown = value.path;
  return typeof path === 'string' ? path : '';
}

/** Jedno odebranie żądania. Odmowa wraca słowami, zamiast wywracać przypadek. */
async function receive(): Promise<{ said: string | null; refusal: string }> {
  try {
    return { said: await launchRequested(CHOICES, AT_ONCE), refusal: '' };
  } catch (error) {
    return { said: null, refusal: error instanceof Error ? error.message : String(error) };
  }
}

/** Odmowa, której ten szkielet nie ma prawa oddać, kiedy zadanie jest skończone. */
function notRefused(refusal: string): void {
  expect(
    refusal.includes('not implemented'),
    'src/sections/run/requested-launch.ts turned the editor request down with "not implemented". ' +
      'That is what the skeleton does today, and it is why this file RUNS that module instead of ' +
      'reading it: ' +
      refusal,
  ).toBe(false);
}

beforeEach(() => {
  launched.mockClear();
  /* Żądanie jest stanem MODUŁU i przeżywa przypadek testowy — dokładnie dlatego, że przeżywa
   * odmontowanie ekranu w aplikacji. Zostawione tu byłoby odebrane przez następny przypadek. */
  takeRequestedRun();
});

describe('the run controls carry the lead, and the green Run in the editor still reaches a run', () => {
  it('draws its group of run controls at all, so the absence check below means something', () => {
    const rendered = markup();

    expect(
      rendered,
      'the run controls rendered nothing. Every "this is no longer there" sentence below would ' +
        'then be true of an empty screen, which is how a criterion goes green on emptiness.',
    ).not.toBe('');
    expect(
      buttonLabels(rendered).length,
      'the run controls rendered no buttons at all, so the strip this criterion is about is not ' +
        'on screen and nothing below is a statement about it.',
    ).toBeGreaterThan(0);
  });

  it('has no workflow list any more, and does have a lead control that names itself', () => {
    const named = accessibleNames(markup());

    expect(
      LEAD_LABEL,
      'src/sections/run/lead.ts carries an empty label for the lead control, so the check below ' +
        'would compare the markup against nothing. A choice with no name is a riddle.',
    ).not.toBe('');

    // ── (a) LISTA WORKFLOW ODDAJE SWOJE MIEJSCE ─────────────────────────────────────────────
    //
    // Nie kosmetyka: ta lista jest slabsza z dwoch drog do jednej czynnosci (nie umie przyjac
    // zadania, ktore `/run <workflow> <co zbudowac>` przyjmuje), a zajmuje miejsce w pasku na
    // stale, przy sufficie chrome 96 px z docs/ARCHITECTURE.md §7.
    expect(
      named,
      'the workflow list is still in the run controls. It is the weaker of two roads to one ' +
        'action and it holds its place in the strip for good.',
    ).not.toContain(PICKER);

    // ── (b) W JEJ MIEJSCU STOI LIDER, I DA SIĘ GO NAZWAĆ ────────────────────────────────────
    expect(
      named.filter((one) => one === LEAD_LABEL).length,
      'the run controls have to carry exactly one control for the lead, and it has to have an ' +
        'accessible name: who you are talking to is not something to guess from the layout. ' +
        'The named controls were: ' +
        JSON.stringify(named),
    ).toBe(1);
  });

  it('names the workflow run separately from the lead conversation', () => {
    const rendered = markup();
    const labels = buttonLabels(rendered);

    expect(
      labels,
      'the real run strip still carries the bare `Start` action next to the lead selector. That ' +
        'makes a complete new workflow run look like the next turn of the selected lead.',
    ).toContain('Run workflow');
    expect(
      labels,
      'the ambiguous bare `Start` label is still visible in the real strip',
    ).not.toContain('Start');

    expect(
      rendered,
      'the true Start path does not mount the workflow action component whose loaded state is ' +
        'judged below.',
    ).toContain('data-workflow-run="manual"');

    const loaded = renderToStaticMarkup(
      <WorkflowRunButton choice={CHOICES[1] ?? null} disabled={false} onRun={() => undefined} />,
    );
    expect(
      buttonLabels(loaded),
      'the actual workflow button mounted by Start does not name the loaded workflow.',
    ).toEqual(['Run Ship a feature']);
    expect(
      loaded,
      'the explanation has to say this is a new complete run from the beginning, not a resumed ' +
        'lead conversation.',
    ).toContain(
      'title="Starts a new run of the complete Ship a feature workflow from the beginning."',
    );
    expect(
      loaded,
      'the visible workflow and the file sent on click have to come from the same choice.',
    ).toContain('data-workflow="ship-a-feature.json"');
  });

  it('launches the workflow the editor asked for, with the limit it was given', async () => {
    requestRun(OPEN);
    expect(
      requestedRun(),
      'the editor request never landed in the module, so the two checks below would be about ' +
        'nothing at all.',
    ).not.toBe(null);

    const first = await receive();
    notRefused(first.refusal);

    // ── (c) ZIELONY `Run` Z EDYTORA NAPRAWDĘ URUCHAMIA BIEG ─────────────────────────────────
    //
    // Zielony `Run` ma dzis JEDNEGO konsumenta i jest nim znikajaca kontrolka wyboru. Bez nowego
    // odbiorcy staje sie martwym przyciskiem — i zlapie to e2e/tests/no-dead-controls.spec.ts,
    // tylko o jeden bieg za pozno (niezmiennik 16).
    expect(
      launched.mock.calls.length,
      'the editor asked for a run and nothing reached the start policy. This is the state the ' +
        'green Run was in before: the screen jumped, nothing began, and nothing said so.',
    ).toBe(1);

    const sent = launched.mock.calls.at(0);
    expect(
      pathOf(sent?.at(0)),
      'the run that began is not the workflow the editor asked for. Picking the first entry on ' +
        'the list looks identical until two files exist — and the list arrives sorted by bytes, ' +
        'so the first one is often a fresh draft with no steps in it. It was handed: ' +
        JSON.stringify(sent),
    ).toBe(OPEN);
    expect(
      sent?.at(1),
      'the limit chosen in the window did not travel with the run, so the scheduler gets a ' +
        'number nobody picked (invariant 11).',
    ).toBe(AT_ONCE);
  });

  it('does not begin that same request a second time on the next render', async () => {
    requestRun(OPEN);

    const first = await receive();
    notRefused(first.refusal);
    expect(launched.mock.calls.length, 'the first receive has to reach the start policy').toBe(1);

    // ── (d) ZAPADKA JEST CAŁĄ OCHRONĄ PRZED DWOMA BIEGAMI Z JEDNEGO KLIKNIĘCIA ──────────────
    const second = await receive();
    notRefused(second.refusal);

    expect(
      launched.mock.calls.length,
      'the same editor request began a run twice. A request left in the module begins one again ' +
        'on every return to the work screen — and two runs of one workflow are two sets of ' +
        'agents writing over the same files, which is what the validator refuses at save time ' +
        '(invariant 12), except here nobody refuses.',
    ).toBe(1);
    expect(
      requestedRun(),
      'the request is still waiting after it was acted on. The latch is what makes "once" true, ' +
        'and this is the class of mistake that costs money rather than a render.',
    ).toBe(null);
  });
});
