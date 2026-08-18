/* Ekran jednego agenta JEST OSIĄGALNY z listy agentów, i ma drogę powrotną.
 *
 * ZMIERZONA WADA (2026-08-18). `session/{filter,layout,density}.ts` to 354 linie gotowej logiki
 * i trzynaście przypadków testowych z ZEREM wołających produkcyjnych, a kafelek w liście agentów
 * był `<span>` bez ani jednego `onClick`. Mechanizm z testem i bez wołającego przechodzi każdą
 * bramkę, jaką to repo ma — i wygląda w raporcie jak zrobiona robota.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(markup).toContain('<button')`. Przechodzi dla przycisku,
 * którego handler nic nie robi — czyli dla tej samej wady z lepszym markupem (niezmiennik 16).
 * Odróżniają to dwie rzeczy: wołamy DOKŁADNIE to, co woła kafelek (`openAgent`), i pytamy
 * markup, czy zmienił się SKUTEK. To repo nie ma jsdom, więc `onClick` nie odpali się w żadnym
 * teście — dlatego handler musi być funkcją modułową, którą test może zawołać wprost, a nie
 * domknięciem zamkniętym w komponencie.
 *
 * DLACZEGO `Rail`, A NIE CAŁY EKRAN PRACY. Szew, o który pyta to kryterium, mieszka w `rail.tsx`
 * — kafelek i miejsce montowania ekranu agenta stoją w tym samym pliku. Że `Rail` jest naprawdę
 * zamontowany na ekranie Run, sądzi osobne, cudze kryterium (`../rail-shows-agents.test.tsx`),
 * które renderuje cały ekran; powtarzanie tego tutaj dałoby drugie miejsce na jedno pytanie.
 *
 * WARTOŚCI OCZEKIWANE POCHODZĄ Z DANYCH ZASIANYCH W MAGAZYNIE, nie z markupu: metrykę zmiany
 * składamy z tych samych liczb, które weszły linią `edit`. Wpisana z palca przechodziłaby także
 * wtedy, gdy ekran karmi blok „co wyprodukował" czymkolwiek innym.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { FeedLine, Step } from '../../../state/run';
import { useRun } from '../../../state/run';
import { line } from '../feed/fixtures/lines';
import { runFeed } from '../feed/live';
import { roster } from '../rail/roster';
import { Rail } from '../rail/rail';
import { closeAgent, openAgent } from './open';

const BUILD = 'Build';
const CHECK = 'Check';

const STEPS: readonly Step[] = [
  { id: 's_1', name: BUILD, state: 'running' },
  { id: 's_2', name: CHECK, state: 'failed' },
];

const SAID = 'Rewrote the field splitter as a three-state machine.';
const OTHERS = 'Ran the checks — they did not work';
const PATH = 'src/parser.rs';
const ADDED = 42;
const REMOVED = 8;

/* Podpis agenta w strumieniu JEST nazwą kroku (`commands/run.rs`: `forward(…, step.name)`), więc
 * plan i strumień spotykają się na tym jednym polu. */
const LINES: readonly FeedLine[] = [
  line.note(1, 0, BUILD, SAID),
  line.edit(2, 400, BUILD, PATH, ADDED, REMOVED),
  line.note(3, 800, CHECK, OTHERS),
];

useRun.setState({ steps: STEPS, lines: [...LINES] });
runFeed.appendLines(LINES);

const cards = roster({
  view: runFeed.view,
  agents: STEPS.map((step) => ({ id: step.name, name: step.name, role: '', step: step.state })),
});

const closed = renderToStaticMarkup(<Rail cards={cards} />);
openAgent(BUILD);
const open = renderToStaticMarkup(<Rail cards={cards} />);
closeAgent();
const backAgain = renderToStaticMarkup(<Rail cards={cards} />);

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

describe('one agent has a screen of its own, reachable from the agents list', () => {
  it('runs on a list that really has tiles on it', () => {
    expect(
      cards.map((card) => card.id),
      'the agents list came out empty, so every question below would be about a screen nobody ' +
        'could open and would pass on nothing.',
    ).toEqual([BUILD, CHECK]);
  });

  it('draws each tile as a control, not as a label', () => {
    expect(
      closed,
      'the tile is not a button, so there is nothing to press. This is where the whole defect ' +
        'started: the mockup draws it as a control because pressing it opens the agent.',
    ).toContain('<button');
    expect(
      closed.indexOf('<button'),
      'and the control is inside the tile, not somewhere else on the column',
    ).toBeGreaterThan(closed.indexOf('data-agent="' + BUILD + '"'));
  });

  it('shows no agent screen until someone opens one', () => {
    expect(
      screenIn(closed),
      'the screen of a single agent stood open with nobody having asked for it, so "opening it ' +
        'works" below would be true of a screen that is simply always there.',
    ).toBe('');
  });

  it('opens the screen of the agent whose tile was pressed', () => {
    const screen = screenIn(open);

    expect(
      screen,
      'pressing a tile changed nothing. A control whose handler has no effect is worse than no ' +
        'control at all (invariant 16) — and it is exactly what this column shipped with.',
    ).not.toBe('');
    expect(screen, 'and it is the screen of THAT agent').toContain('data-agent-screen="' + BUILD);
    expect(
      visible(screen),
      'headed with the two questions a person opens an agent to answer',
    ).toContain('What ' + BUILD + ' produced');
  });

  it('reads its facts off the disk, and the other agent’s work off nothing at all', () => {
    const words = visible(screenIn(open));

    expect(
      words,
      'the change this agent really made is missing. The block is fed from the edit lines — ' +
        'facts — and that is the entire reason it is not fed the last thing the agent said.',
    ).toContain(PATH + ' · +' + String(ADDED) + ' −' + String(REMOVED));
    expect(words, 'and what this agent said is on the screen too, as what it said').toContain(SAID);
    expect(
      words,
      'the other agent’s line reached this screen. The view of one agent is the same stream ' +
        'with a filter; a line of somebody else in it makes the screen answer a question it was ' +
        'not asked.',
    ).not.toContain(OTHERS);
  });

  it('says which step it stands on without inventing the brief it was given', () => {
    const words = visible(screenIn(open));

    expect(words, 'the step is a fact the plan carries, so it is on the screen').toContain(BUILD);
    expect(
      words,
      'a row ending in a dash with nothing after it. The prompt of the step is read from disk ' +
        'and dropped on the way into the run store, so there is no brief to show — and a ' +
        'stand-in row in the same grid and the same face as a real one cannot be told apart ' +
        'from it (invariant 17).',
    ).not.toContain('—  ');
    expect(
      words,
      'and no stand-in for a value we do not have. poprzedni prototyp rendered exactly this and it read ' +
        'as data.',
    ).not.toContain('not reported');
  });

  it('has a way back, and taking it leaves the run view as it was', () => {
    expect(
      visible(screenIn(open)),
      'an agent screen with no way out is a trap, not a screen',
    ).toContain('←');
    expect(
      screenIn(backAgain),
      'going back left the agent screen on top of the run view. The way back has to have the ' +
        'same effect as the tile, in reverse — one field, one truth.',
    ).toBe('');
    expect(
      backAgain,
      'and the agents list is still there underneath, unchanged by the trip',
    ).toContain('data-agent="' + BUILD + '"');
  });
});
