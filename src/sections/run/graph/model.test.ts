/* Czym jest ten plik: cała polityka płótna biegu, sądzona bez okna.
 *
 * Reguła 17 mieszka tutaj, nie w komponencie. Płótno wolno narysować WYŁĄCZNIE wtedy, gdy plan
 * niesie pozycje i strzałki — a kiedy ich nie niesie (plan jednego kroku, który okno składa
 * samo dla `/ask`), brak pola znaczy „nie wiemy", nigdy „nie ma". Zgadywana pozycja i ozdobna
 * krzywa między dwoma zmyślonymi punktami wyglądają identycznie do chwili, w której ktoś na
 * nich oprze decyzję.
 */
import { describe, expect, it } from 'vitest';
import type { Link } from '../../../state/workflows';
import type { GraphStep, Plan } from './model';
import { LIVE_ARROW, arrowsOf, hasLayout, measureOf, tilesOf } from './model';

/** Krok planu. Domyślnie stoi w miejscu, które podał plik. */
function step(id: string, extra: Partial<GraphStep> = {}): GraphStep {
  return { id, name: id, status: 'waiting', at: { x: 0, y: 0 }, ...extra };
}

/** Plan, którego kroki stoją w pliku i są połączone strzałką. */
const LAID_OUT: Plan = {
  steps: [step('one'), step('two', { at: { x: 240, y: 0 } })],
  links: [{ from: 'one', to: 'two' }],
};

/** Plan jednego kroku, jaki okno składa dla `/ask`: ani pozycji, ani strzałek. */
const ASKED: Plan = {
  steps: [{ id: 'only', name: 'Answer', status: 'working' }],
  links: [],
};

describe('kiedy wolno narysować płótno', () => {
  it('draws when the plan carries a place for every step and at least one arrow', () => {
    expect(
      hasLayout(LAID_OUT),
      'the plan carries a place for every step and one arrow between them, which is everything ' +
        'the picture is made of — refusing here would leave the drawing unreachable',
    ).toBe(true);
  });

  it('stays silent when the plan carries neither places nor arrows', () => {
    expect(
      hasLayout(ASKED),
      'this plan is the one the window builds for a single typed question: it has no place and ' +
        'no arrow. Drawing it means inventing both, and an invented picture reads exactly like a ' +
        'measured one',
    ).toBe(false);
  });

  it('stays silent when one step alone has no place', () => {
    const half: Plan = {
      steps: [step('one'), { id: 'two', name: 'two', status: 'waiting' }],
      links: [{ from: 'one', to: 'two' }],
    };
    expect(
      hasLayout(half),
      'one step has no place of its own, so the picture would have to make one up for it',
    ).toBe(false);
  });

  it('stays silent when the steps stand somewhere but nothing joins them', () => {
    expect(
      hasLayout({ steps: LAID_OUT.steps, links: [] }),
      'the steps have places and nothing joins them, so the picture would show relations that ' +
        'the file does not state',
    ).toBe(false);
  });
});

describe('kafelki', () => {
  it('puts every step exactly where the file says', () => {
    expect(
      tilesOf(LAID_OUT).map((tile) => tile.position),
      'the places must come from the file, one for one',
    ).toEqual([
      { x: 0, y: 0 },
      { x: 240, y: 0 },
    ]);
  });

  it('leaves out a step whose place nobody wrote down', () => {
    const half: Plan = {
      steps: [step('one'), { id: 'two', name: 'two', status: 'waiting' }],
      links: [],
    };
    expect(
      tilesOf(half).map((tile) => tile.id),
      'a step without a place must not be drawn at a made-up one',
    ).toEqual(['one']);
  });

  it('hands the step itself to the card, so the card reads no second copy', () => {
    expect(tilesOf(LAID_OUT)[0]?.data.step.id).toBe('one');
  });
});

describe('strzałka, którą idzie praca', () => {
  const running: Plan = {
    steps: [
      step('plan', { status: 'done' }),
      step('build', { status: 'working', at: { x: 240, y: 0 } }),
      step('ship', { status: 'waiting', at: { x: 480, y: 0 } }),
    ],
    links: [
      { from: 'plan', to: 'build' },
      { from: 'build', to: 'ship' },
    ],
  };

  /* 2026-08-31 — TE DWA PUNKTY PYTAŁY O `animated`, CZYLI O RUCH. Strzałka przestała płynąć
   * i została przy samej barwie: rodzajów ruszających się rzeczy było trzy przy suficie dwóch
   * z ARCHITECTURE §7, a ta kreska i kropka na kafelku odpowiadały na to samo pytanie
   * (niezmiennik 13). Pytanie punktu jest to samo — KTÓRA strzałka niesie pracę — zmienił się
   * nośnik odpowiedzi, więc oba dalej padają, gdy model przestanie jej udzielać. */
  it('marks the arrow whose far end is working right now', () => {
    const marked = arrowsOf(running).filter((arrow) =>
      (arrow.className ?? '').includes(LIVE_ARROW),
    );
    expect(
      marked.map((arrow) => arrow.id),
      'work has arrived at the step that is working, and the arrow it came along is the one ' +
        'path on this picture that carries the happening-now colour',
    ).toEqual(['plan->build']);
  });

  it('leaves every other arrow in the plain colour', () => {
    expect(
      arrowsOf(running).find((arrow) => arrow.id === 'build->ship')?.className,
      'nothing has travelled along this arrow yet, so colouring it would show work where there ' +
        'is none',
    ).toBeUndefined();
  });

  it('draws an arrow only when the file states both of its ends', () => {
    const dangling: Plan = { steps: LAID_OUT.steps, links: [{ from: 'one', to: 'gone' }] };
    expect(
      arrowsOf(dangling),
      'an arrow into nothing is a relation the file does not have',
    ).toEqual([]);
  });
});

describe('powrót', () => {
  const back: Link = { from: 'check', to: 'build', max_turns: 3 };
  const looping: Plan = {
    steps: [step('build'), step('check', { at: { x: 240, y: 0 } })],
    links: [{ from: 'build', to: 'check' }, back],
  };

  it('says out loud how many times the work may come back', () => {
    expect(
      arrowsOf(looping).find((arrow) => arrow.id === 'check->build')?.label,
      'two arrows between the same pair of cards read as a mistake in the drawing unless the one ' +
        'that goes back says what it is',
    ).toBe('up to 3 tries');
  });

  it('draws the way back as a broken line', () => {
    expect(arrowsOf(looping).find((arrow) => arrow.id === 'check->build')?.style).toEqual({
      strokeDasharray: '6 4',
    });
  });

  it('leaves the ordinary arrow unbroken and unlabelled', () => {
    const plain = arrowsOf(looping).find((arrow) => arrow.id === 'build->check');
    expect(plain?.style).toBeUndefined();
    expect(plain?.label).toBeUndefined();
  });
});

describe('miara na czwartej linii', () => {
  const many: Plan = {
    steps: [step('a'), step('b'), step('c'), step('sum', { name: 'Summary' })],
    links: [
      { from: 'a', to: 'sum' },
      { from: 'b', to: 'sum' },
      { from: 'c', to: 'sum' },
      { from: 'a', to: 'b' },
    ],
  };

  it('counts the handovers when more than one step feeds this one', () => {
    expect(measureOf(step('sum'), many).waits).toBe('reads 3 handoffs');
  });

  it('names the single step before this one, never its key', () => {
    expect(
      measureOf(step('b'), many).waits,
      'the key from the file is not something a person ever sees',
    ).toBe('after a');
  });

  it('says so when nothing comes before', () => {
    expect(measureOf(step('a'), many).waits).toBe('first step');
  });

  it('knows whether anything comes after', () => {
    expect(measureOf(step('a'), many).handsOn).toBe(true);
    expect(measureOf(step('sum'), many).handsOn).toBe(false);
  });
});
