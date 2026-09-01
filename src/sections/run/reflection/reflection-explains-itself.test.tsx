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
 * CZWARTA, I TA JEDNA PRZESZŁA — ZMIERZONE 2026-09-01, TRZY DNI PO NAPISANIU TEGO PLIKU.
 * Zdanie stało w markupie jako węzeł tekstowy, więc punkt niżej był ZIELONY. Na ekranie miało
 * ZERO PIKSELI SZEROKOŚCI: kontrolka stała w rzędzie paska loadoutu, ten rząd dostawał 1108 px
 * przy 1562 chcianych, a `truncate` zjadł całe zdanie (400 px) i połowę jego własnej nazwy
 * (57 ze 112). `renderToStaticMarkup` nie ma szerokości, więc napis ucięty do zera jest dla
 * niego nieodróżnialny od czytelnego — i to jest GRANICA tego przyrządu, nie wada tego pytania.
 *
 * CO SIĘ Z TYM STAŁO. Kontrolka zeszła z paska do stopy kolumny planu, gdzie zdanie się ZAWIJA
 * i mieści w całości (`./toggle.tsx`, cały rachunek). Ten plik pyta więc od dziś o dwie rzeczy
 * naraz: że zdanie stoi jako TEKST i że stoi TAM, gdzie ma miejsce — czyli nie w rzędzie, który
 * je zjadł. O prawdziwą szerokość w chromium pyta
 * `e2e/tests/what-this-run-keeps-is-readable.spec.ts`; bez niego to niżej dalej nie odróżnia
 * zdania czytelnego od skróconego do zera i tak ma zostać, bo w tym repo nie ma jsdom.
 *
 * Granica jest atrapą: żadnego żywego Tauri i żadnej przeglądarki. Że pole naprawdę wychodzi
 * z `run.json` na tę granicę, sądzi `src-tauri/tests/it/reflection_receipt_reaches_the_history.rs`
 * — bez niego wszystko poniżej stoi na fiksturze.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { Choice } from '../choices';
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
const { forgetWhatIsReady, rememberAgents, rememberRuns, rememberWorkflows } =
  await import('../whats-ready');

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

/**
 * Ekran pracy, na którym setup jest gotowy i NIC nie biegnie.
 *
 * TRZY ODPOWIEDZI Z DYSKU, każda tą samą drogą, którą wpisuje je produkcja (`../whats-ready`)
 * — bez nich cały obszar pracy należy do przewodnika pierwszego uruchomienia i kolumny planu
 * nie ma na ekranie wcale. Efekty nie biegną pod `renderToStaticMarkup`, więc jedyną drogą do
 * tych faktów jest ta, którą chodzi produkt.
 */
const READY: Choice = {
  path: 'ship-a-feature.json',
  name: 'Ship a feature',
  steps: [{ id: 's_build', name: 'Build', state: 'pending', kind: 'agent', at: { x: 40, y: 40 } }],
  links: [],
};

/* Ten sam ekran ZANIM setup się skończy — cały obszar pracy należy wtedy do przewodnika
 * pierwszego uruchomienia i kolumny planu nie ma na nim wcale. Trzeci punkt niżej pyta, czy to
 * jest granica, którą wolno mieć. */
const beforeSetupIsDone = screen();

rememberWorkflows([READY]);
rememberAgents(1);
rememberRuns(HERE.folder, []);
const workScreen = screen();
forgetWhatIsReady();

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

/**
 * Wycinek markupu należący do kolumny planu — od jej znacznika do kolumny obok.
 *
 * WYCINEK, NIE CAŁY EKRAN, i to jest ZAWĘŻENIE pytania, nie jego poluzowanie: napis
 * „Learn from this run" stał do 2026-09-01 w rzędzie kontrolek paska i punkt szukający go
 * w całym markupie był zielony w OBU miejscach. Pytanie brzmi dziś także „gdzie", więc
 * odpowiadać ma tylko ta jedna kolumna.
 */
function planColumn(markup: string): string {
  const opens = markup.indexOf('data-plan-column');
  if (opens < 0) return '';
  const rest = markup.slice(opens);
  const closes = rest.indexOf('data-stream-column');
  return closes < 0 ? rest : rest.slice(0, closes);
}

/** Wycinek markupu należący do rzędu kontrolek paska — od jego znacznika do końca paska. */
function stripRow(markup: string): string {
  const opens = markup.indexOf('data-workflow-controls');
  if (opens < 0) return '';
  const rest = markup.slice(opens);
  const closes = rest.indexOf('data-work=');
  return closes < 0 ? rest : rest.slice(0, closes);
}

/**
 * Sam znacznik kontrolki ręcznego biegu — od `<button` do klamry zamykającej otwarcie.
 *
 * PO ZNACZNIKU, NIE PO CAŁYM NAPISIE Z ATRYBUTAMI: kolejność atrybutów jest sprawą tego,
 * w jakiej napisano propsy, i punkt o nią pytający byłby czerwony od przestawienia dwóch linii,
 * które niczego nie zmieniają.
 */
function runControl(markup: string): string {
  const at = markup.indexOf('data-workflow-run="manual"');
  if (at < 0) return '';
  const opens = markup.lastIndexOf('<', at);
  const closes = markup.indexOf('>', at);
  return opens < 0 || closes < 0 ? '' : markup.slice(opens, closes + 1);
}

describe('the control says what it will do before anybody turns it on', () => {
  it('stands a sentence next to "Learn from this run", in the markup the work screen draws', () => {
    expect(
      planColumn(workScreen),
      'the work screen this point reads has no plan column at all, so every question below ' +
        'would be asked of an empty string and would pass on nothing',
    ).not.toBe('');
    expect(
      planColumn(workScreen),
      'the control itself has to be on the work screen, or there is nothing for the sentence to ' +
        'stand next to',
    ).toContain(REFLECTION_LABEL);
    expect(
      readsAsText(planColumn(workScreen), REFLECTION_EXPLAINED),
      'a person reading "Learn from this run" cannot tell what Loadout will do with it: whether ' +
        'anything is written down, how much of it, or whether it starts being used without them ' +
        'saying so. The sentence has to be TEXT on the screen — a title attribute answers only ' +
        'the person who already stopped the mouse there, and this is the one control on this ' +
        'screen that spends money after the run is over. Missing as text: ' +
        JSON.stringify(REFLECTION_EXPLAINED),
    ).toBe(true);
  });

  /* TRZECI PUNKT, DOPISANY 2026-09-01 RAZEM Z PRZENIESIENIEM, I PYTAJĄCY O JEGO GRANICĘ.
   *
   * Kontrolka stała do dziś w pasku, który rysuje się ZAWSZE; stoi w kolumnie planu, której na
   * niedokończonym setupie nie ma. To jest realna różnica i albo się ją nazwie i sprawdzi, albo
   * się o niej dowie człowiek. Granica brzmi: ekran, na którym tej kontrolki nie ma, nie ma też
   * czym zacząć biegu — więc nie ma wyboru, którego dałoby się nie dosięgnąć.
   *
   * Sprawdzalne mechanicznie, bo `welcomeIsTheWholeScreen` wymaga, żeby BRAKOWAŁO folderu,
   * agenta albo workflow — a bez workflow kontrolka startu nie ma czego puścić i jest wyłączona
   * u źródła (`../start.tsx`, `disabled={chosen === ''}`). */
  it('is missing only from a screen that cannot start a run in the first place', () => {
    expect(
      beforeSetupIsDone.includes('data-plan-column'),
      'this point is about the screen a person sees before the setup is finished, and that ' +
        'screen already has a plan column here — so it would be asking its question of the ' +
        'wrong screen and passing on nothing',
    ).toBe(false);
    expect(
      beforeSetupIsDone.includes(REFLECTION_LABEL),
      'the choice is somewhere on the unfinished-setup screen after all, which makes the rest ' +
        'of this point moot: say where it stands instead of leaving this stale',
    ).toBe(false);
    const control = runControl(beforeSetupIsDone);
    expect(
      control,
      'there is no manual run control on that screen at all, so this point would pass on an ' +
        'empty string rather than on the control it is about',
    ).not.toBe('');
    expect(
      control.includes('disabled'),
      'the screen without the choice on it still offers a live way to start a run: ' +
        JSON.stringify(control) +
        '. Then a person can spend money on a private turn after that run without ever being ' +
        'shown the choice that turns it off, and without a way to change it',
    ).toBe(true);
  });

  /* DRUGI PUNKT, DOPISANY 2026-09-01. Pierwszy pyta o zdanie i o miejsce, w którym ma stać;
   * ten pyta o miejsce, w którym stać NIE MOŻE — i te dwa pytania nie są tym samym pytaniem
   * odwróconym. Zdanie wolno postawić w obu miejscach naraz i pierwszy punkt byłby wtedy
   * zielony, a człowiek dostałby z powrotem dokładnie to, co zmierzono: rząd wysoki na 52 px,
   * któremu przy oknie z makiety brakuje 454 px, i napis skrócony w nim do zera. */
  it('never puts a sentence back into the row that has no width for one', () => {
    const row = stripRow(workScreen);
    expect(
      row,
      'the strip carries no row of controls on this screen, so this point would pass on an ' +
        'empty string instead of on the row it is about',
    ).not.toBe('');
    expect(
      row.includes(REFLECTION_EXPLAINED),
      'the sentence is back in the row of controls in the loadout strip. That row is one line ' +
        '52 pixels tall; measured in Chromium at 1512x950 it is given 1108 pixels and wants ' +
        '1562, so a sentence 400 pixels long standing in it is cut to nothing — and this point ' +
        'cannot see that, because static markup has no widths. It can see that the sentence is ' +
        'somewhere it will be squeezed, which is the same defect one step earlier',
    ).toBe(false);
    expect(
      row.includes(REFLECTION_LABEL),
      'the choice itself is back in the row of controls in the loadout strip, and its own name ' +
        'was cut there too — 57 pixels of the 112 it needs, read by a person as "Learn f…"',
    ).toBe(false);
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
