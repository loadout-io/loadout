/* „What X was given" mówi prawdę: wiersze biorą się z plików, nie z pustych list.
 *
 * ZMIERZONA WADA (2026-08-23). `session/mount.tsx` podawał `handoffs: []` i `notes: []` NA
 * SZTYWNO, a `stepsOf` fabrykował `brief: ''` i `files: []`. Blok mówił więc „Nothing was given
 * to this agent." w biegu, w którym ten agent dostał trzy pliki od poprzedników i dwie notatki
 * w promptcie — i mówił to w jedynym miejscu na ekranie, które odpowiada na pytanie, czy
 * synteza w ogóle widziała to, co zebrał research.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: zasiać wiersze wprost w `sessionSections()` i sprawdzić, że
 * markup je pokazuje. Ona przechodzi DZISIAJ — `layout.ts` umie te wiersze złożyć od zawsze
 * i ma na to własne kryterium (`./layout.test.ts`). Zepsuty był SZEW: nikt tych faktów nie
 * czytał z dysku. Dlatego wszystko poniżej jedzie przez prawdziwą krawędź do Rusta, atrapą
 * jest wyłącznie transport, a spodziewane napisy pochodzą z tych samych obiektów, które
 * atrapa oddała.
 *
 * DRUGA SŁABA WERSJA, i ta wygląda mocno: „są wiersze". Przechodzi ją odczyt, który pokazuje
 * KAŻDY plik z folderu każdemu agentowi. Wtedy blok „co dostał" jest listą wszystkiego, co
 * kiedykolwiek powstało, czyli nie odpowiada na pytanie, które ma nad sobą napisane. Stąd
 * kontrola: plik zaadresowany do INNEGO kroku nie ma prawa stanąć na tym ekranie.
 *
 * CZEGO TO KRYTERIUM NIE MIERZY, powiedziane wprost: efektu Reacta. To repo nie ma jsdom,
 * więc `useEffect` nie odpali się tutaj ani razu — dlatego odczyt jest funkcją modułową,
 * którą test woła dokładnie tak, jak woła ją wejście w ekran agenta.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { Handoff, Note } from '../../../state/memory';
import type { FeedLine, Step } from '../../../state/run';

const BUILD = 'Build';
const CHECK = 'Check';
const PLAN = 'Plan';
/* Pod-agent rozpuszczony w trakcie biegu: nadaje do strumienia, więc ma kafelek, ale nie stoi
 * na żadnym kroku i nikt mu niczego nie zostawił. To jest ten agent, dla którego zdanie
 * „Nothing was given to this agent." jest prawdą — i jedyny, dla którego jest. */
const SCOUT = 'Scout';

const FOLDER = '/Users/x/ledger-ui';

const STEPS: readonly Step[] = [
  { id: 's_build', name: BUILD, state: 'running' },
  { id: 's_check', name: CHECK, state: 'pending' },
];

/** Plik, który poprzednik zostawił TEMU agentowi. 2560 bajtów to równo 2.5 KB. */
const FOR_BUILD: Handoff = {
  id: '0198a1f2-0001',
  run: '20260823-100000__0198a1f2',
  from: PLAN,
  to: [BUILD],
  kind: 'brief',
  title: 'What to build and why',
  status: 'current',
  created: '2026-08-23T10:00:00Z',
  path: '.loadout/runs/20260823-100000__0198a1f2/handoffs/01__plan__brief.md',
  bytes: 2560,
};

/** Plik, który TEN agent zostawił dalej — nie jest tym, co dostał. */
const FROM_BUILD: Handoff = {
  ...FOR_BUILD,
  id: '0198a1f2-0002',
  from: BUILD,
  to: [CHECK],
  kind: 'patch-summary',
  title: 'What changed and what to look at',
  path: '.loadout/runs/20260823-100000__0198a1f2/handoffs/02__build__patch-summary.md',
  bytes: 1024,
};

/** Kontrola: ten sam bieg, ten sam folder, ADRESAT INNY. */
const FOR_CHECK: Handoff = {
  ...FOR_BUILD,
  id: '0198a1f2-0003',
  to: [CHECK],
  title: 'What to look at once the work lands',
  path: '.loadout/runs/20260823-100000__0198a1f2/handoffs/01__plan__check-brief.md',
  bytes: 512,
};

/** Notatka, która jedzie w promptcie tego jednego agenta. */
const MINE: Note = {
  id: 'n-state-machines',
  title: 'Small state machines',
  rule: 'Prefer small state machines over patterns for parsing',
  because: 'The quoted-comma case came back three times.',
  status: 'in-use',
  scope: 'this-agent',
  length: 54,
  occurrences: 3,
  modified: '2026-08-20',
  agent: BUILD,
};

/** Kandydatka, której nikt nie wziął do użytku — nie jedzie nigdzie i nie ma jej na ekranie. */
const NOT_YET: Note = {
  ...MINE,
  id: 'n-generated-files',
  title: 'Generated files',
  rule: 'Never edit a file that a build writes',
  status: 'suggested',
  scope: 'everywhere',
  agent: null,
};

/** Notatka bez właściciela: jedzie w promptcie KAŻDEGO kroku, więc stoi na każdym ekranie. */
const EVERYONES: Note = {
  ...NOT_YET,
  id: 'n-run-the-checks',
  title: 'Run the checks',
  rule: 'Run the checks before you say a thing is done',
  status: 'in-use',
};

/* Atrapą jest wyłącznie transport. Krawędzie (`sections/memory/io.ts`) są prawdziwe, więc
 * literówka w nazwie komendy albo w kluczu argumentu przewraca ten plik. */
const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((_command: string, _args?: unknown): Promise<unknown> => Promise.resolve(null)),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { AgentScreen, readWhatWasGiven } = await import('./mount');
const { closeAgent, openAgent } = await import('./open');
const { roster } = await import('../rail/roster');
const { runFeed } = await import('../feed/live');
const { useRun } = await import('../../../state/run');
const { line } = await import('../feed/fixtures/lines');

/* Podpis agenta w strumieniu JEST nazwą kroku (`commands/run.rs`: `forward(…, step.name)`),
 * więc plan i strumień spotykają się na tym jednym polu. */
const LINES: readonly FeedLine[] = [
  line.note(1, 0, BUILD, 'Rewrote the field splitter as a three-state machine.'),
  line.note(2, 400, SCOUT, 'Looked through the parser for the quoted-comma case.'),
];

useRun.setState({ steps: STEPS, lines: [...LINES], folder: FOLDER });
runFeed.appendLines(LINES);

const cards = roster({
  view: runFeed.view,
  agents: STEPS.map((step) => ({
    id: step.name,
    name: step.name,
    role: '',
    step: step.state,
    stepId: step.id,
  })),
});

/** Co dysk odpowiada na oba pytania tego bloku. */
function onDisk(passed: readonly Handoff[], notes: readonly Note[]): void {
  invoked.mockImplementation((command: string): Promise<unknown> => {
    if (command === 'list_handoffs') return Promise.resolve(passed);
    if (command === 'list_notes') return Promise.resolve(notes);
    return Promise.resolve(null);
  });
}

/** Sam ekran agenta, wycięty z markupu. Pusty, kiedy go nie ma. */
function screenIn(markup: string): string {
  const at = markup.indexOf('data-agent-screen');
  return at < 0 ? '' : markup.slice(at);
}

/** Tekst, który człowiek naprawdę czyta — bez znaczników, więc bez klas i atrybutów `data-*`. */
function visible(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Ekran otwartego agenta, przeczytany tak, jak czyta go człowiek. */
function words(agent: string): string {
  openAgent(agent);
  return visible(screenIn(renderToStaticMarkup(<AgentScreen cards={cards} />)));
}

/** Argumenty ostatniego wywołania tej komendy — albo `undefined`, gdy nikt jej nie zawołał. */
function lastCallOf(command: string): Record<string, unknown> | undefined {
  const hit = invoked.mock.calls.filter((call) => call[0] === command).at(-1);
  return hit?.[1] as Record<string, unknown> | undefined;
}

onDisk([FOR_BUILD, FROM_BUILD, FOR_CHECK], [MINE, NOT_YET]);
await readWhatWasGiven(FOLDER);
const askedFor = lastCallOf('list_handoffs');
const build = words(BUILD);
const stranger = words(SCOUT);

/* Druga scena, jedna różnica: notatka bez właściciela. Jedzie w promptcie każdego kroku, więc
 * jest tym samym faktem dla agenta, który nie dostał ani jednego pliku. */
onDisk([], [EVERYONES]);
await readWhatWasGiven(FOLDER);
const strangerWithANote = words(SCOUT);
closeAgent();

describe('what an agent was given is read off the disk, not left empty', () => {
  it('runs on a list that really has tiles on it', () => {
    expect(
      cards.map((card) => card.id),
      'the agents list came out empty, so every question below would be about a screen nobody ' +
        'could open and would pass on nothing.',
    ).toEqual([BUILD, SCOUT]);
  });

  it('asks the disk about the folder this run is working in', () => {
    expect(
      askedFor,
      'nothing was asked for at all, so the block can only be as empty as it was before.',
    ).toBeDefined();
    expect(
      askedFor?.['folder'],
      'and it has to be the folder of THIS run. Without it the other side falls back to a ' +
        'directory picked when the app started, and the block answers about somebody else’s ' +
        'work under this run’s heading.',
    ).toBe(FOLDER);
  });

  it('lists every file a step before it left for this agent, with its name and how big it is', () => {
    expect(
      build,
      'the file this agent was handed does not name who left it. "From whom" is the first ' +
        'thing a person asks about a file they did not write.',
    ).toContain('From ' + PLAN);
    expect(
      build,
      'and it has to carry the one line saying what that file is. A row that is only a path ' +
        'answers "there is something" and leaves the reading to be done twice.',
    ).toContain(FOR_BUILD.title);
    expect(
      build,
      'and how much of it there is, read as a person reads it. 2560 bytes is exactly 2.5 KB, ' +
        'and the number is the only honest answer to "is this the whole research or a stub".',
    ).toContain('2.5 KB');
  });

  it('leaves out the file that was addressed to another step', () => {
    expect(
      build,
      'a file left for the checking step turned up under what the building step was given. ' +
        'A block that shows everything in the folder is a list of the folder, not an answer ' +
        'to the question written above it.',
    ).not.toContain(FOR_CHECK.title);
    expect(
      stranger,
      'and the same file is not on the screen of an agent that stands on no step at all',
    ).not.toContain(FOR_CHECK.title);
  });

  it('names the note that went into this agent’s prompt, and only that one', () => {
    expect(
      build,
      'the note Loadout puts into this prompt is missing. Whether the model knew a rule is ' +
        'the second half of "what was this agent given" and it is not readable anywhere else.',
    ).toContain(MINE.rule + ' — in use');
    expect(
      build,
      'a note nobody took into use is not given to anybody, so it has no place in a block of ' +
        'facts about what this agent had.',
    ).not.toContain(NOT_YET.rule);
  });

  it('stops saying "nothing" once there is something', () => {
    expect(
      build,
      'the block still says nothing was given while naming things that were. Two answers to ' +
        'one question, and the plain sentence is the one a person believes.',
    ).not.toContain('Nothing was given');
  });

  it('still says "nothing" for an agent that was given nothing', () => {
    expect(
      stranger,
      'an agent dissolved inside the run stands on no step, was left no file and had no rule ' +
        'of its own. The honest sentence is the whole point of the block: a row invented for ' +
        'it would read exactly like a row with data behind it.',
    ).toContain('Nothing was given to this agent.');
  });

  it('gives every agent the note that has no owner', () => {
    expect(
      strangerWithANote,
      'a rule with no owner goes into the prompt of every step, so it is a fact about every ' +
        'agent. Hiding it here would under-report what the model was told.',
    ).toContain(EVERYONES.rule + ' — in use');
  });
});
