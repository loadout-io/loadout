/* Formularz agenta to SIEDEM wierszy widocznych, nie czternaście — i każdy z tych siedmiu
 * wymaga odpowiedzi albo mówi coś, czego bez niego nie widać.
 *
 * # Co się zmieniło 2026-08-31 i dlaczego
 *
 * Ten plik pytał wcześniej o DZIEWIĘĆ wierszy widocznych i pięć pod `More settings`. Liczba
 * była wzięta z makiety i przez to prawdziwa co do rysunku, a fałszywa co do pracy: zmierzone
 * na żywym oknie, formularz stał na 19 elementach interaktywnych w kolumnie 332 px i był
 * WYŻSZY NIŻ OKNO — ~1150 px treści przy 748 px miejsca, więc `Save` i zdanie tłumaczące,
 * czemu jest wygaszony, leżały ~400 px pod krawędzią. Nawet zwinięty zajmował ~720 z 748.
 *
 * Jedenaście z czternastu pól nie wymagało ŻADNEJ decyzji, żeby zapisać działającego agenta:
 * miały działającą wartość domyślną albo pustka była w nich poprawna. Decyzji wymagały trzy —
 * Name, Instructions, What it does — a wszystkie czternaście stały w jednym płaskim stosie,
 * tą samą etykietą, w tej samej randze.
 *
 * # Siedem, i czemu akurat te
 *
 * Trzy niosą TREŚĆ agenta (Name, What it does, Instructions). Trzy niosą UPRAWNIENIA i granicę
 * (pliki, sieć, czas). Jeden niesie to, czym ten agent myśli — i jest jednym wierszem, bo to
 * jest jedno pytanie, a nie trzy: `Runs with`, `Model` i `Thinking` mają każde działającą
 * domyślną i razem czytają się jak jedno zdanie.
 *
 * Czego tu NIE MA i gdzie poszło:
 *   `Colour`        — poza formularz. Token przydziela ekran, a zmienia się go klikiem
 *                     w kwadrat na kafelku (`index.tsx`), bo to pole nie wymaga ani jednej
 *                     decyzji, a stało NAD `Instructions`, czyli nad całą treścią agenta.
 *   `Tools`         — pod `More settings`, tam gdzie stało. Przy Codeksie jest niedostępne,
 *                     przy Claude'ie nie ma pickera ani sprawdzenia wpisu, a jedyna rzecz,
 *                     po którą po nie sięgano — sieć — ma własny wiersz od 2026-08-23.
 *   `Extra options` — pod osobne, jawne `Advanced`. Surowe argv to nie jest „więcej ustawień",
 *                     tylko inna ranga decyzji.
 *
 * # Słaba wersja tego kryterium
 *
 * Siedem osobnych `expect(html).toContain('Instructions')`. Ósme pole przechodzi wtedy bez
 * mrugnięcia — a dokładnie o to tu pytamy. Przechodzi też samo słowo w zdaniu pomocniczym pod
 * kontrolką, czyli tekst, który niczego nie zapisuje. Dlatego niżej stoi równość CAŁEJ tablicy
 * etykiet, z kolejnością i długością, dla każdego z czterech stanów rozwinięcia.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Agent } from '../../state/agents';
import { AgentForm } from './agent-form';

/** Siedem wierszy widocznych od razu, w tej kolejności. */
const SEVEN = [
  'Name',
  'What it does',
  'Instructions',
  'Runs with',
  'Can it change files',
  'Can it reach the web',
  'Give up after',
];

/** Co odsłania sam wiersz `Runs with` — jedno pytanie rozłożone z powrotem na trzy kontrolki. */
const BRAIN = ['Model', 'Thinking'];

/** Co odsłania `More settings`. */
const MORE = ['Tools', 'Skills', 'Connections'];

/** Co odsłania `Advanced`. Nazwa wiersza mówi, do której aplikacji te wiersze pojadą. */
const ADVANCED = ['Extra options for Claude Code'];

const FORGE: Agent = {
  schema: 1,
  id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f77',
  name: 'Forge',
  summary: 'Writes code',
  color: 'clay',
  instructions: 'Write the smallest change that makes the checks pass.',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'work-freely',
  giveUpAfterMinutes: 10,
  tools: 'everything',
  reachesTheWeb: false,
  skills: [],
  connections: [],
  writeResultsTo: 'handoffs/build.md',
};

function noop(): void {
  /* sterowany formularz: w statycznym renderze nic tego nie woła */
}

interface Open {
  readonly more?: boolean;
  readonly brain?: boolean;
  readonly advanced?: boolean;
}

function markupOf(value: Agent, open: Open = {}): string {
  return renderToStaticMarkup(
    <AgentForm
      value={value}
      expanded={open.more ?? false}
      brainOpen={open.brain ?? false}
      advancedOpen={open.advanced ?? false}
      onChange={noop}
      onToggleMore={noop}
      onSave={noop}
    />,
  );
}

/** Tekst bez znaczników i bez encji. React zapisuje apostrof jako `&#x27;`. */
function plain(fragment: string): string {
  return fragment
    .replace(/<[^>]*>/g, ' ')
    .replace(/&#x27;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim();
}

/* NAZWY WIERSZY, A NIE SAME `<label>`.
 *
 * Wiersz `Runs with` w stanie zwiniętym nie ma kontrolki formularza — ma przycisk, który go
 * rozwija — więc `<label for>` byłby tam etykietą wskazującą na coś, co etykiety nie przyjmuje.
 * Nazwę niesie wtedy `<span class="label">, czyli ten sam napis w tej samej randze. Pytanie
 * „ile wierszy widzi człowiek" jest pytaniem o te napisy, nie o element, w który są zawinięte. */
function rowsOf(html: string): string[] {
  return [...html.matchAll(/<(label|span)\b[^>]*class="label"[^>]*>([\s\S]*?)<\/\1>/g)].map((hit) =>
    plain(hit[2] ?? ''),
  );
}

/** Atrybuty przycisku o tym napisie, albo `null`, kiedy takiego przycisku nie ma. */
function buttonAttributes(html: string, label: string): string | null {
  for (const hit of html.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)) {
    if (plain(hit[2] ?? '') === label) return hit[1] ?? '';
  }
  return null;
}

/** Znacznik otwierający kontrolkę o tym `data-field`, albo pusty napis. */
function control(html: string, field: string): string {
  return new RegExp('<[a-zA-Z]+\\b[^>]*\\bdata-field="' + field + '"[^>]*>').exec(html)?.[0] ?? '';
}

describe('the agent form asks seven things, and everything else is behind a name', () => {
  it('reads out seven rows, in order, and no eighth one', () => {
    expect(
      rowsOf(markupOf(FORGE)),
      'these seven, in this order, and nothing else. An eighth row here is the first step ' +
        'towards the settings page nobody fills in, and it is always defensible on its own. ' +
        'Measured before this was written: fourteen rows and nineteen controls in a 332px ' +
        'column, and Save some 400px below the bottom edge of the window',
    ).toEqual(SEVEN);
  });

  it('has no colour row at all, open or closed', () => {
    const everywhere = markupOf(FORGE, { more: true, brain: true, advanced: true });

    expect(
      rowsOf(everywhere),
      'colour is decorative, has a working default and needs no decision — and it stood ABOVE ' +
        'Instructions, which is the whole content of an agent. It is picked by clicking the ' +
        'square on the tile now',
    ).not.toContain('Colour');
    expect(
      control(everywhere, 'color'),
      'and there is no control for it either: a row left in the page while its name is gone is ' +
        'the same height on screen',
    ).toBe('');
  });

  it('folds the three questions about thinking into one row that reads as one sentence', () => {
    const closed = markupOf(FORGE);

    expect(
      plain(closed),
      'closed, the row says what this agent runs on, which model and how deeply it thinks. All ' +
        'three have a working default, so all three are an answer already given — not three ' +
        'questions asked again',
    ).toContain('Claude Code · opus · Balanced');
    for (const field of ['runsWith', 'model', 'thinking']) {
      expect(
        control(closed, field),
        'and closed it carries no control for ' +
          field +
          '. A control hidden by a style sheet is still a row of this form and still takes its ' +
          'height on screen',
      ).toBe('');
    }

    const open = markupOf(FORGE, { brain: true });
    expect(
      rowsOf(open),
      'opened, and only then, the one row becomes the three it stands for — in place, between ' +
        'Instructions and the question about files',
    ).toEqual([...SEVEN.slice(0, 3), 'Runs with', ...BRAIN, ...SEVEN.slice(4)]);
  });

  it('keeps More settings and Advanced apart, and both out of the tree until asked', () => {
    const closed = markupOf(FORGE);
    for (const field of ['tools', 'skills', 'connections', 'vendorOptions']) {
      expect(
        control(closed, field),
        'with everything closed there is no control for ' +
          field +
          ' at all. A control that is in the page but hidden still counts as a row of this ' +
          'form, and it is how seven quietly becomes fourteen',
      ).toBe('');
    }

    expect(
      rowsOf(markupOf(FORGE, { more: true })),
      'More settings adds exactly Tools, Skills and Connections — and not the extra options, ' +
        'which are raw arguments and a different rank of decision',
    ).toEqual([...SEVEN, ...MORE]);
    expect(
      rowsOf(markupOf(FORGE, { advanced: true })),
      'and Advanced adds exactly the extra options of the app this agent runs with, named for ' +
        'that app',
    ).toEqual([...SEVEN, ...ADVANCED]);

    expect(
      buttonAttributes(markupOf(FORGE), 'Advanced'),
      'Advanced is its own named control. Hiding raw arguments under the same button as skills ' +
        'and connections is how a person opens one and finds the other',
    ).not.toBeNull();
  });

  it('leaves nothing hidden by a style sheet', () => {
    const html = markupOf(FORGE);

    expect(
      / hidden(?:=""|>|\s)/.test(html),
      'nothing in this form may carry the hidden attribute: hiding is how a control stays in ' +
        'the page while the count above still passes',
    ).toBe(false);
    expect(
      /display\s*:\s*none/i.test(html),
      'and nothing may set display:none, for the same reason. What is open is decided in ' +
        'TypeScript, not in a style sheet',
    ).toBe(false);
  });

  it('gives the instructions the room they are worth', () => {
    const html = markupOf(FORGE);
    const area = control(html, 'instructions');

    expect(area, 'the form has to render the instructions area; there is none').not.toBe('');
    expect(
      area.startsWith('<textarea'),
      'instructions are the whole content of an agent, so they get a box you can write a page ' +
        'in — not a single line',
    ).toBe(true);

    const rows = Number(/\brows="(\d+)"/.exec(area)?.[1] ?? 0);
    expect(
      rows,
      'measured before this was written: a 64px window in a 12px monospace face, which is ' +
        'about 120 characters of the whole role prompt visible at once',
    ).toBeGreaterThan(6);
    expect(
      area,
      'and the house height has to be handed back, or the rows above are a number nothing ' +
        'reads. The one field the house owns is 64px tall for every textarea in the app',
    ).toContain('h-auto');

    expect(
      buttonAttributes(html, 'Taller'),
      'and a person who writes more than that can say so. The drag handle in the corner is ' +
        'there, and nobody finds it',
    ).not.toBeNull();
  });

  it('offers the models it knows and says when the one typed is your own', () => {
    const known = markupOf(FORGE, { brain: true });
    expect(
      known,
      'the models this app names are offered, so nobody has to remember the spelling',
    ).toContain('<datalist');
    expect(
      plain(known),
      'and a model the app names gets no note: a note rendered whether or not it applies ' +
        'teaches people to skim past every note',
    ).not.toContain('is your own');

    const typo = markupOf({ ...FORGE, model: 'opus4' }, { brain: true });
    expect(
      plain(typo),
      'a model nobody named is passed through exactly as typed, and the form says so. Until ' +
        'this line "opus4" saved without a murmur and fell over in the middle of a run',
    ).toContain('opus4 is your own');
  });

  it('makes giving up a choice, because ten minutes was a decision wearing a default', () => {
    const html = markupOf(FORGE);
    const dial = control(html, 'giveUpAfterMinutes');

    expect(
      dial.startsWith('<select'),
      'a number box says "any number is fine here" and then ten minutes arrives as if nobody ' +
        'chose it. For an agent that writes code ten minutes is very little',
    ).toBe(true);
    for (const said of ['10 minutes', '30 minutes', 'No limit']) {
      expect(plain(html), 'the choice reads ' + said).toContain(said);
    }

    expect(
      plain(markupOf({ ...FORGE, giveUpAfterMinutes: 45 })),
      'and an agent already saved with its own number keeps it on screen. A list that quietly ' +
        'drops the value it was given shows one thing and saves another',
    ).toContain('45 minutes');
  });

  it('will not let you save an agent with no name', () => {
    const attributes = buttonAttributes(markupOf({ ...FORGE, name: '' }), 'Save');

    expect(attributes, 'the form has to render a Save button; there is none').not.toBeNull();
    expect(
      /\bdisabled\b/.test(attributes ?? ''),
      'an agent with no name is not saveable: the name is how every other screen refers to it',
    ).toBe(true);
  });

  it('will not let you save an agent with no instructions', () => {
    const attributes = buttonAttributes(markupOf({ ...FORGE, instructions: '' }), 'Save');

    expect(attributes, 'the form has to render a Save button; there is none').not.toBeNull();
    expect(
      /\bdisabled\b/.test(attributes ?? ''),
      'instructions are 80% of what makes an agent an agent; an agent without them is a name',
    ).toBe(true);
  });

  it('lets you save as soon as the name and the instructions are filled in', () => {
    const attributes = buttonAttributes(markupOf(FORGE), 'Save');

    expect(attributes, 'the form has to render a Save button; there is none').not.toBeNull();
    expect(
      /\bdisabled\b/.test(attributes ?? ''),
      'everything except the name and the instructions has a default, so Save has to come ' +
        'alive as soon as those two are there',
    ).toBe(false);
  });
});

/* TRZECIA WADA TEJ SAMEJ RODZINY, i dlatego dostaje wlasne kryterium.
 *
 * Ekran Agents padl w tym repo trzy razy z tego samego powodu: pole przyszlo Z PLIKU NA DYSKU
 * w ksztalcie, ktorego typ nie dopuszcza, a kod czytal je bez oslony. Najpierw `instructions`
 * (`.replace` na `undefined`), potem `model` (`.trim`), a 2026-09-01 przy robieniu zrzutow do
 * README — `runsWith`. Za kazdym razem granica bledu robila z sekcji pusty prostokat, czyli
 * ekran, ktory dla czlowieka wyglada na „nic tu nie ma", a naprawde sie wywrocil.
 *
 * `runsWith` jest inny niz tamte dwa i dlatego nie lapie go naprawa granicy w `io.ts`: klucz JEST
 * obecny, tylko jego WARTOSC nie nalezy do zbioru, ktory ta wersja zna. `MODELS` jest
 * `Record<Vendor, …>`, wiec TypeScript uwaza odczyt za pewny — a plik zapisany przez starsza
 * wersje albo poprawiony recznie daje `undefined`.
 *
 * Kryterium nie pyta o `?? []`, tylko o to, co widzi czlowiek: czy formularz w ogole sie narysowal
 * i czy dalej nazywa role, o ktora chodzi. Naprawa przez inne wyrazenie ma je spelniac tak samo. */
describe('an agent whose vendor this version does not know still reaches the screen', () => {
  it('draws the form instead of taking the section down with it', () => {
    const fromAnOlderFile = { ...FORGE, runsWith: 'some-other-cli' } as unknown as Agent;

    const markup = markupOf(fromAnOlderFile);

    expect(
      markup,
      'the form rendered nothing for an agent whose vendor this version does not know. On disk ' +
        'that is one saved file; on screen it took the whole Agents section down behind the ' +
        'error boundary, and a person reads that as an empty library rather than a crash.',
    ).not.toBe('');
    expect(
      markup,
      'the form drew something, but not this role: an agent that survives the render and loses ' +
        'its own name is the same loss, one step later.',
    ).toContain(FORGE.name);
  });
});
