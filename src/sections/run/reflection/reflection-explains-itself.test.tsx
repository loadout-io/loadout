/* Refleksja mówi, CO ROBI, zanim ruszy, i CO ZROBIŁA, kiedy bieg zejdzie — w markupie ekranu,
 * nie w wartości zwróconej z funkcji (niezmiennik 29).
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(reflectionText(KEPT)).toContain('2 notes')`. Przechodzi ją
 * dokładnie ten stan, który zastano 2026-08-29: rachunek prywatnej tury leży w `run.json`, funkcja
 * umie z niego złożyć zdanie, ekran historii nie czyta ani jednego z tych pól, a `grep -r reflect
 * src/sections/run/past/` daje ZERO trafień. Kryterium zielone, funkcja martwa — czyli klasa, dla
 * której to repo powstało. Dlatego montowany jest CAŁY ekran sekcji (`<Run />`), a bieg otwiera się
 * tą samą drogą, którą otwiera go człowiek: `/history`, potem wiersz listy.
 *
 * DRUGA SŁABA WERSJA, GORSZA: sprawdzić samą OBECNOŚĆ zdania przy kontrolce. Przechodzi ją
 * `title="…"` — czyli podpowiedź, której nikt nie zobaczy, dopóki nie zatrzyma myszy nad
 * ptaszkiem. Kontrolka ma powiedzieć, co się stanie, PATRZĄCEMU. Dlatego zdanie jest sądzone jako
 * WĘZEŁ TEKSTOWY (`>zdanie<`), a nie jako wystąpienie w markupie.
 *
 * TRZECIA, NAJCICHSZA: sprawdzić tylko bieg, po którym coś zostało. Bieg, po którym nie została
 * ani jedna notatka, milczałby wtedy dalej — a milczenie jest nieodróżnialne od awarii i jest
 * całym powodem, dla którego to zadanie istnieje. Dlatego czytane są CZTERY biegi, po jednym na
 * każdy stan rachunku, i asercje pytają też o to, że ich zdania są RÓŻNE.
 *
 * Granica jest atrapą: żadnego żywego Tauri i żadnej przeglądarki. Że pole naprawdę wychodzi
 * z `run.json` na tę granicę, sądzi `src-tauri/tests/it/reflection_receipt_reaches_the_history.rs`
 * — bez niego wszystko poniżej stoi na fiksturze.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { PastReflection, PastRun, PastRunRow } from '../io';

/** Cztery biegi, po jednym na każdy stan rachunku prywatnej tury. */
const KEPT_TWO = '20260829-101500__0198a1f2-3b4c-7d5e-8f60-000000000101';
const KEPT_ONE = '20260829-101501__0198a1f2-3b4c-7d5e-8f60-000000000102';
const KEPT_NONE = '20260829-101502__0198a1f2-3b4c-7d5e-8f60-000000000103';
const NEVER_ASKED = '20260829-101503__0198a1f2-3b4c-7d5e-8f60-000000000104';
const BEFORE_THE_FIELD = '20260829-101504__0198a1f2-3b4c-7d5e-8f60-000000000105';

function row(folder: string, title: string): PastRunRow {
  return {
    folder,
    when: '2026-08-29 10:15',
    title,
    state: 'succeeded',
    steps: 1,
    costUsd: 1,
    said: null,
  };
}

const ROWS: readonly PastRunRow[] = [
  row(KEPT_TWO, 'Kept two'),
  row(KEPT_ONE, 'Kept one'),
  row(KEPT_NONE, 'Kept nothing'),
  row(NEVER_ASKED, 'Never asked'),
  row(BEFORE_THE_FIELD, 'Older than the field'),
];

/** Otwarty bieg w kształcie, w którym przyjeżdża z Rusta. `undefined` znaczy „opis tego nie ma". */
function opened(folder: string, reflection: PastReflection | null | undefined): PastRun {
  const run = {
    folder,
    when: '2026-08-29 10:15',
    title: ROWS.find((one) => one.folder === folder)?.title ?? '',
    state: 'succeeded',
    workflowFile: 'ship-a-feature.json',
    steps: [
      {
        id: '0198a1f2-3b4c-7d5e-8f60-00000000000b',
        tile: 'build',
        name: 'Build',
        agent: 'claude',
        state: 'succeeded',
        summary: 'Stored the greeting.',
        error: '',
        costUsd: 1,
        lines: [],
      },
    ],
    handoffs: [],
    said: null,
  };
  return reflection === undefined ? run : { ...run, reflection };
}

const KEPT: Readonly<Record<string, PastRun>> = {
  [KEPT_TWO]: opened(KEPT_TWO, { ran: true, kept: 2, discardedAgain: 0, droppedWithoutReason: 0 }),
  [KEPT_ONE]: opened(KEPT_ONE, { ran: true, kept: 1, discardedAgain: 0, droppedWithoutReason: 0 }),
  [KEPT_NONE]: opened(KEPT_NONE, {
    ran: true,
    kept: 0,
    discardedAgain: 1,
    droppedWithoutReason: 2,
  }),
  [NEVER_ASKED]: opened(NEVER_ASKED, {
    ran: false,
    kept: 0,
    discardedAgain: 0,
    droppedWithoutReason: 0,
  }),
  [BEFORE_THE_FIELD]: opened(BEFORE_THE_FIELD, undefined),
};

/* Atrapa granicy oddaje `Promise<unknown>` JAWNIE, a nie z wnioskowania: bez adnotacji `vi.fn`
 * zamraża typ pierwszego ciała, a to niżej podmieniamy na takie, które oddaje bieg z historii. */
const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((_command: string, _args?: unknown): Promise<unknown> =>
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
const { closeHistory } = await import('../past/store');
const { REFLECTION_EXPLAINED, REFLECTION_LABEL } = await import('./toggle');
const { DID_NOT_LOOK_BACK, KEPT_NOTHING, NOT_IN_THE_RECORD, reflectionText } =
  await import('./said');
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
 * Czy to zdanie stoi w markupie jako TEKST, a nie wyłącznie jako wartość atrybutu.
 *
 * Węzeł tekstowy Reacta zawsze przylega do klamer znacznika, więc `>zdanie<` odróżnia go od
 * `title="zdanie"` bez pytania o nazwę znacznika ani o kolejność atrybutów — a ta kolejność jest
 * sprawą tego, w jakiej napisano propsy, i kryterium o nią pytające byłoby czerwone od
 * przestawienia dwóch linii, które niczego nie zmieniają.
 */
function readsAsText(markup: string, sentence: string): boolean {
  return markup.includes('>' + sentence + '<');
}

const workScreen = screen();

invoked.mockImplementation((command: string, args?: unknown): Promise<unknown> => {
  if (command === 'list_runs') return Promise.resolve(ROWS);
  if (command === 'read_run') {
    const asked = (args as { run?: string } | undefined)?.run ?? '';
    return Promise.resolve(KEPT[asked]);
  }
  return Promise.resolve(undefined);
});

await openHistoryFromLine('');

/** Markup panelu z otwartym TYM biegiem. */
async function afterOpening(folder: string): Promise<string> {
  await openOneRun(HERE.folder, folder);
  return screen();
}

const withTwo = await afterOpening(KEPT_TWO);
const withOne = await afterOpening(KEPT_ONE);
const withNothing = await afterOpening(KEPT_NONE);
const neverAsked = await afterOpening(NEVER_ASKED);
const olderThanTheField = await afterOpening(BEFORE_THE_FIELD);

closeHistory();

/** Zdanie, które ten ekran postawił w wierszu refleksji — albo pusty napis, gdy wiersza nie ma. */
function reflectionRow(markup: string): string {
  const at = markup.indexOf('data-reflection');
  if (at < 0) return '';
  const opens = markup.indexOf('>', at);
  const closes = markup.indexOf('<', opens);
  return opens < 0 || closes < 0 ? '' : markup.slice(opens + 1, closes);
}

describe('the control says what it will do before anybody turns it on', () => {
  it('stands a sentence next to "Learn from this run", in the markup the work screen draws', () => {
    expect(
      workScreen,
      'the control itself has to be on the work screen, or there is nothing for the sentence to ' +
        'stand next to',
    ).toContain(REFLECTION_LABEL);
    expect(
      readsAsText(workScreen, REFLECTION_EXPLAINED),
      'a person reading "Learn from this run" cannot tell what Loadout will do with it: whether ' +
        'anything is written down, how much of it, or whether it starts being used without them ' +
        'saying so. The sentence has to be TEXT on the screen — a title attribute answers only ' +
        'the person who already stopped the mouse there, and this is the one control on this ' +
        'strip that spends money after the run is over. Missing as text: ' +
        JSON.stringify(REFLECTION_EXPLAINED),
    ).toBe(true);
  });
});

describe('says what it did with this run, and says it outright when it kept nothing', () => {
  it('says what it did with this run, and says it outright when it kept nothing', () => {
    expect(
      reflectionRow(withTwo),
      'a run whose private turn left notes behind has to say HOW MANY, right there in the run a ' +
        'person opened. Without the number the sentence answers a question nobody asked: the ' +
        'reason to open a finished run is to find out what came out of it.',
    ).toBe(reflectionText(KEPT[KEPT_TWO]?.reflection ?? null));
    expect(withTwo, 'and that count has to be readable as a count, not as a field name').toContain(
      'kept 2 notes',
    );
    expect(
      withOne,
      'one note may never read as "1 notes" — a screen that cannot count to one is a screen ' +
        'nobody trusts with the rest of its numbers',
    ).toContain('kept 1 note for you');

    expect(
      reflectionRow(withNothing),
      'a run whose private turn went and found nothing has to SAY so. An empty space there is ' +
        'indistinguishable from a turn that fell over — and somebody paid for that turn either ' +
        'way. That is the whole reason this row exists.',
    ).toBe(
      KEPT_NOTHING +
        ' It threw out 1 note you had already turned down, and 2 notes that came with no reason under it.',
    );
    expect(
      reflectionRow(withNothing) === reflectionRow(neverAsked),
      'a run that was asked and came back empty-handed is not the same fact as a run nobody ' +
        'asked, and one sentence for both answers the wrong question for one of them',
    ).toBe(false);
    expect(
      reflectionRow(neverAsked),
      'a run whose private turn never went has to say that, instead of leaving the row blank',
    ).toBe(DID_NOT_LOOK_BACK);

    expect(
      reflectionRow(olderThanTheField),
      'a run recorded before this was ever written down knows nothing about it, and saying "did ' +
        'not look back" there would be inventing an answer out of a missing key (invariant 17)',
    ).toBe(NOT_IN_THE_RECORD);
  });
});
