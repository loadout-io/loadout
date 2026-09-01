/* Reguła 17 na drodze, po której człowiek ją widzi.
 *
 * Płótno jest legalne WYŁĄCZNIE dlatego, że kroki, strzałki i pozycje stoją w pliku workflow.
 * Plan, który ich nie niesie — a taki okno składa samo dla wpisanego pytania — nie dostaje
 * płótna: dostaje listę kroków. Zgadywana pozycja i ozdobna krzywa wyglądają dokładnie tak
 * samo jak zmierzone, a różnica wychodzi dopiero wtedy, gdy ktoś na nich oprze decyzję.
 *
 * Punkt o liście i punkt o płótnie stoją tu OBA z premedytacją. Sama odmowa rysowania
 * przechodzi na komponencie, który nie rysuje niczego — czyli na pustym szkielecie.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Plan } from './model';
import { RunGraph, edgesFor, nodesFor } from './graph';
import { LIVE_ARROW } from './model';

/** To, co React Flow zawsze stawia wokół płótna. Nie ma go — nie ma płótna. */
const CANVAS = 'react-flow__pane';

const LAID_OUT: Plan = {
  steps: [
    { id: 'plan', name: 'Plan the work', status: 'done', at: { x: 0, y: 0 } },
    { id: 'build', name: 'Build the parser', status: 'working', at: { x: 264, y: 0 } },
  ],
  links: [{ from: 'plan', to: 'build' }],
};

/** Plan jednego kroku, jaki okno składa dla wpisanego pytania: bez pozycji i bez strzałek. */
const ASKED: Plan = {
  steps: [{ id: 'ask', name: 'Answer the question', status: 'working' }],
  links: [],
};

describe('plan bez układu', () => {
  it('draws the picture when the file says where every step stands', () => {
    expect(
      renderToStaticMarkup(<RunGraph plan={LAID_OUT} />),
      'both steps carry a place and the file joins them, so there is a real shape to show and ' +
        'refusing it would leave the drawing unreachable in the product',
    ).toContain(CANVAS);
  });

  it('shows a plain list instead, and names every step in it', () => {
    const markup = renderToStaticMarkup(<RunGraph plan={ASKED} />);
    expect(
      markup,
      'nothing in this plan says where the step stands or what it runs after, so a picture ' +
        'would have to invent both',
    ).not.toContain(CANVAS);
    expect(
      markup,
      'the step still has to be on screen — silence about the shape is not silence about the work',
    ).toContain('Answer the question');
  });

  it('lists every step of a plan that has arrows but nowhere to put them', () => {
    const markup = renderToStaticMarkup(
      <RunGraph
        plan={{ steps: LAID_OUT.steps.map(({ at: _at, ...rest }) => rest), links: LAID_OUT.links }}
      />,
    );
    expect(markup).not.toContain(CANVAS);
    expect(markup).toContain('Plan the work');
    expect(markup).toContain('Build the parser');
  });
});

describe('co dostaje rysujący', () => {
  const running: Plan = {
    steps: [
      ...LAID_OUT.steps,
      { id: 'ship', name: 'Ship it', status: 'waiting', at: { x: 528, y: 0 } },
    ],
    links: [...LAID_OUT.links, { from: 'build', to: 'ship' }],
  };

  it('hands the drawing one card per step, each carrying its own step', () => {
    expect(nodesFor(running).map((tile) => tile.data.step.id)).toEqual(['plan', 'build', 'ship']);
  });

  /* 2026-08-31 — TEN PUNKT PYTAŁ O `animated`, CZYLI O RUCH. Strzałka przestała płynąć
   * i została przy samej barwie; powód w całości stoi przy `LIVE_ARROW` w `./model.ts`.
   * Pytanie jest to samo — czy odpowiedź modelu DOCHODZI do rysującego — zmienił się nośnik. */
  it('hands the drawing exactly one marked arrow, and it is the one work came along', () => {
    const marked = edgesFor(running).filter((arrow) =>
      (arrow.className ?? '').includes(LIVE_ARROW),
    );
    expect(
      marked.map((arrow) => arrow.id),
      'the model knows which arrow the work travelled, and this is the only place that answer ' +
        'reaches the drawing — a version that keeps the answer to itself stays green while ' +
        'the picture says nothing',
    ).toEqual(['plan->build']);
  });

  it('gives every arrow a head, so a person can read which way the work goes', () => {
    expect(
      edgesFor(running).every((arrow) => arrow.markerEnd !== undefined),
      'a line without a head states that two steps are related and not which comes first',
    ).toBe(true);
  });
});
