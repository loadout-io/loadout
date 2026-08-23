/* „Run this step again" JEST na ekranie i naprawdę powtarza ten krok (niezmiennik 29).
 *
 * ZMIERZONA WADA (2026-08-23). Cała droga istniała i nie miała ani jednego wołającego spoza
 * testów: `commands::rerun` po stronie Rusta, `rerunStep` w `../io.ts`, `runStepAgain`
 * w `../rail/again.ts`, przycisk w `./session.tsx`. Brakowało jednej rzeczy — `AgentScreen`
 * rysuje ten przycisk wyłącznie wtedy, gdy dostanie `onSaid`, a jedyne miejsce montażu podawało
 * mu sam `cards`. Mechanizm z testem i bez wołającego przechodzi każdą bramkę, jaką to repo ma,
 * i wygląda w raporcie jak zrobiona robota.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: wyrenderować `Session` wprost z `onRunAgain`. Przechodzi DZIŚ,
 * bo `session.tsx` od zawsze umie ten przycisk narysować — zepsuty był szew nad nim. Dlatego
 * renderujemy DOKŁADNIE tę drogę, którą składa okno: kolumnę z listą agentów, w której ekran
 * jednego agenta jest montowany.
 *
 * DRUGA SŁABA WERSJA: „przycisk jest". Przechodzi ją przycisk nie wiedzący, którego kroku
 * dotyczy — a powtórzenie nie tego kroku kosztuje dokładnie te minuty, dla których cała ta
 * ścieżka powstała. Markup musi więc nieść klucz kroku, a wywołanie polityki — komplet
 * argumentów, bez których tamta strona nie wie, co uruchomić.
 *
 * TRZECIA: sam fakt zawołania. Rust odpowiada ZDANIEM, kiedy dzisiejszy plik różni się od tego,
 * który wtedy biegł („to samo z twoją poprawką" nie może wyglądać jak „to samo jeszcze raz").
 * Zdanie, które nie ma gdzie wylądować, jest ciszą — więc pytamy też o strumień.
 *
 * CZEGO TO KRYTERIUM NIE MIERZY, powiedziane wprost: samego `onClick`. To repo nie ma jsdom,
 * więc kliknięcia nie da się tu wywołać. Markup mówi, czy kontrolka JEST i czy wie o swoim
 * kroku; wywołanie polityki mówi, dokąd sięga. Sam gest sądzi `e2e/`, w przeglądarce.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { FeedLine, Step } from '../../../state/run';

const BUILD = 'Build';
const CHECK = 'Check';
/* Pod-agent rozpuszczony w trakcie biegu: nadaje, więc ma kafelek, ale nie stoi na żadnym
 * kroku — nie ma czego powtórzyć i nie dostaje przycisku (niezmiennik 16). */
const SCOUT = 'Scout';

const HERE = '/Users/x/ledger-ui';
const FILE = 'ship-a-feature.json';
const STEP = 's_build';

/** Zdanie, którym Rust mówi, że plik workflow zmienił się od tamtego biegu. */
const SAID = 'The workflow file changed since that run, so this step ran with today’s version.';

const STEPS: readonly Step[] = [
  { id: STEP, name: BUILD, state: 'failed' },
  { id: 's_check', name: CHECK, state: 'pending' },
];

/* Atrapą jest wyłącznie transport. Krawędź (`../io.ts`) jest prawdziwa, więc literówka w nazwie
 * komendy albo w kluczu argumentu przewraca ten plik. */
const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((_command: string, _args?: unknown): Promise<unknown> => Promise.resolve(null)),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { Rail, sayAfterRunningAgain } = await import('../rail/rail');
const { runStepAgain } = await import('../rail/again');
const { closeAgent, openAgent } = await import('./open');
const { roster } = await import('../rail/roster');
const { runFeed } = await import('../feed/live');
const { useRun } = await import('../../../state/run');
const { atOnce, setAtOnce } = await import('../limits/chosen');
const { line } = await import('../feed/fixtures/lines');

const LINES: readonly FeedLine[] = [
  line.note(1, 0, BUILD, 'The parser still drops the quoted comma.'),
  line.note(2, 400, SCOUT, 'Looked through the parser for the quoted-comma case.'),
];

useRun.setState({ steps: STEPS, lines: [...LINES], folder: HERE, fileName: FILE });
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

/** Ile ma biec naraz — wybór człowieka, więc jawnie NIE domyślny. */
setAtOnce(4);

/** Argumenty ostatniego wywołania tej komendy — albo `undefined`, gdy nikt jej nie zawołał. */
function lastCallOf(command: string): Record<string, unknown> | undefined {
  const hit = invoked.mock.calls.filter((call) => call[0] === command).at(-1);
  return hit?.[1] as Record<string, unknown> | undefined;
}

/** Nazwa znacznika, w którym stoi ten napis — pytanie „czy to jest kontrolka". */
function tagAround(markup: string, text: string): string {
  const at = markup.indexOf(text);
  if (at < 0) return '';
  const opens = markup.lastIndexOf('<', at);
  return /^<([a-z]+)/.exec(markup.slice(opens))?.[1] ?? '';
}

/** Wszystko, co okno rysuje, kiedy otwarty jest ten agent. */
function screenOf(agent: string): string {
  openAgent(agent);
  return renderToStaticMarkup(<Rail cards={cards} />);
}

const AGAIN = 'Run this step again';

const onAStep = screenOf(BUILD);
const onNoStep = screenOf(SCOUT);
closeAgent();

invoked.mockImplementation((command: string): Promise<unknown> => {
  if (command === 'rerun_step') return Promise.resolve(SAID);
  return Promise.resolve(null);
});

/* To samo, co robi przycisk: polityka powtórzenia i ten sam kanał na odpowiedź, który okno
 * samo podaje ekranowi agenta. Wszystko pomiędzy jest kodem produkcyjnym. */
runStepAgain(STEP, sayAfterRunningAgain);
await new Promise<void>((done) => {
  setTimeout(done, 0);
});

const asked = lastCallOf('rerun_step');
const inTheStream = runFeed.view.history.filter((row) => row.label.includes(SAID));

describe('running one step again is something a person can reach', () => {
  it('runs on a list that really has tiles on it', () => {
    expect(
      cards.map((card) => card.id),
      'the agents list came out empty, so every question below would be about a screen nobody ' +
        'could open and would pass on nothing.',
    ).toEqual([BUILD, SCOUT]);
  });

  it('draws the control on the screen the window really mounts', () => {
    expect(
      onAStep,
      'the control is not on the screen. Every piece behind it works when called — the command, ' +
        'the edge, the policy, the markup — and nothing calls them, which is the whole defect.',
    ).toContain(AGAIN);
    expect(
      tagAround(onAStep, AGAIN),
      'it has to be a real button. A styled span looks identical and does nothing when pressed.',
    ).toBe('button');
  });

  it('carries the key of the step it would repeat', () => {
    expect(
      onAStep,
      'the control does not say which step it repeats. Without the key the markup cannot tell ' +
        'one from another, and repeating the wrong one costs the forty-eight minutes this ' +
        'whole path exists to save.',
    ).toContain('data-run-again="' + STEP + '"');
  });

  it('leaves it off an agent that stands on no step', () => {
    expect(
      onNoStep.includes(AGAIN),
      'an agent dissolved inside the run is in no workflow file, so there is no key to repeat ' +
        'it by. A control that cannot work is worse than no control at all.',
    ).toBe(false);
  });

  it('reaches the other side with everything it needs to run that one step', () => {
    expect(
      asked,
      'pressing it reached nothing at all. A control with no handler is the defect invariant ' +
        '16 names by hand.',
    ).toBeDefined();
    expect(
      asked?.['fileName'],
      'the workflow file this run came from — the other side looks the newest run of THAT ' +
        'workflow up by it',
    ).toBe(FILE);
    expect(
      asked?.['step'],
      'and the key of the one step to repeat. An empty one would start the whole graph over.',
    ).toBe(STEP);
    expect(
      asked?.['howManyAtOnce'],
      'and how many are allowed to work at once, as the person set it. A constant on the other ' +
        'side looks identical here and quietly ignores the choice.',
    ).toBe(atOnce());
    expect(
      asked?.['folder'],
      'and the folder this run works in, so the repeat happens where the work is',
    ).toBe(HERE);
  });

  it('puts the answer where the person is already reading', () => {
    expect(
      inTheStream.length,
      'the other side answered that the file had changed since that run, and the answer went ' +
        'nowhere. "The same thing again" and "the same thing with your fix" cannot look alike, ' +
        'and an answer with no place to land is silence.',
    ).toBe(1);
  });
});
