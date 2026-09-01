/* KAŻDY PLAN BIEGU JEST JEDNĄ PIONOWĄ ŚCIEŻKĄ — także ten, który wie, gdzie stoi każdy krok.
 *
 * CO TU STAŁO DO 2026-08-31 I DLACZEGO ZESZŁO. Ten plik pilnował reguły 17 od drugiej strony:
 * plan z pozycjami i strzałkami DOSTAWAŁ płótno, a plan bez nich — listę. Kryterium było
 * zielone i mierzyło prawdę o kodzie, tyle że kod robił rzecz, której na ekranie biegu nie da
 * się przeczytać: zmierzone na zrzucie okna 1512×950 kafelki płótna miały 40 px wysokości,
 * a karta pytania — ~120 px szerokości. Ścieżka kroków, którą rysuje makieta
 * (`docs/mockup/index.html`, `.work` i `.step`), w produkcie NIE POJAWIAŁA SIĘ NIGDY, bo warunek
 * przepuszczał ją wyłącznie dla planu bez zapisanych pozycji — a taki plan ma tylko okno
 * składające krok dla wpisanego pytania. Fikstura podawała właśnie taki plan, więc punkt
 * o ścieżce był zielony nad mechanizmem, którego człowiek nie widział ani razu (niezmiennik 29).
 *
 * NOWA PRAWDA, KTÓREJ TE PUNKTY PILNUJĄ. Płótno należy do EDYTORA workflow i tam zostaje; ekran
 * biegu rysuje ścieżkę ZAWSZE. Reguła 17 nie znika — przesuwa się na nośnik: skoro nie rysujemy
 * ani jednej strzałki, relacja „co po czym" musi dojechać SŁOWAMI, a robi to ostatnia linia
 * karty („after Plan the work"), wyliczona ze strzałek z pliku (`./model.ts`, `measureOf`).
 * Zgadnięta pozycja i ozdobna krzywa dalej są zakazane — po prostu nie ma już czego zgadywać.
 *
 * NAZWA PLIKU ZOSTAJE, bo zostaje pytanie: co robi ekran z planem, który o swoim kształcie nic
 * nie mówi. Odpowiedź się zmieniła — dziś dostaje dokładnie ten sam obraz, co każdy inny.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Plan } from './model';
import { RunGraph } from './graph';

/** To, co React Flow zawsze stawia wokół płótna. Jest w markupie — jest płótno. */
const CANVAS = 'react-flow__pane';

/** Plan z pliku workflow: każdy krok ma miejsce, a strzałki mówią, co po czym idzie. */
const LAID_OUT: Plan = {
  steps: [
    { id: 'plan', name: 'Plan the work', status: 'done', at: { x: 0, y: 0 } },
    { id: 'build', name: 'Build the parser', status: 'working', at: { x: 264, y: 0 } },
    { id: 'ship', name: 'Ship it', status: 'waiting', at: { x: 528, y: 0 } },
  ],
  links: [
    { from: 'plan', to: 'build' },
    { from: 'build', to: 'ship' },
  ],
};

/** Klucze kroków tego planu, w kolejności pliku — wartość oczekiwana każdego punktu niżej. */
const IN_FILE = LAID_OUT.steps.map((step) => step.id);

/** Ten sam plan bez ani jednej pozycji: tyle wie okno, kiedy strzałki są, a miejsc nie ma. */
const FLAT: Plan = {
  steps: LAID_OUT.steps.map(({ at: _at, ...rest }) => rest),
  links: LAID_OUT.links,
};

/** Plan jednego kroku, jaki okno składa dla wpisanego pytania: bez pozycji i bez strzałek. */
const ASKED: Plan = {
  steps: [{ id: 'ask', name: 'Answer the question', status: 'working' }],
  links: [],
};

/** Ten sam plan z inaczej nazwanym poprzednikiem — dowód, że podpis czyta plan, a nie siebie. */
const RENAMED: Plan = {
  ...LAID_OUT,
  steps: LAID_OUT.steps.map((step) =>
    step.id === 'plan' ? { ...step, name: 'Draft the work' } : step,
  ),
};

const DRAWN = renderToStaticMarkup(<RunGraph plan={LAID_OUT} />);
const FLATTENED = renderToStaticMarkup(<RunGraph plan={FLAT} />);
const ONE_STEP = renderToStaticMarkup(<RunGraph plan={ASKED} />);
const RENAMED_DRAWN = renderToStaticMarkup(<RunGraph plan={RENAMED} />);

/** Klucze kroków w kolejności, w jakiej stoją w markupie. */
function stepsIn(markup: string): readonly string[] {
  return [...markup.matchAll(/data-step="([^"]*)"/g)].map((hit) => hit[1] ?? '');
}

/** Markup jednej karty: od jej klucza do klucza karty następnej. */
function cardOf(markup: string, id: string): string {
  const opens = markup.indexOf('data-step="' + id + '"');
  if (opens < 0) return '';
  const rest = markup.slice(opens);
  const next = rest.indexOf('data-step="', 1);
  return next < 0 ? rest : rest.slice(0, next);
}

/** Tekst, który człowiek na tym kawałku przeczyta — bez znaczników. */
function textOf(piece: string): string {
  return piece.replace(/<[^>]*>/g, '');
}

describe('plan biegu jest ścieżką, zawsze', () => {
  it('draws a step of the path for every step the file lists', () => {
    expect(
      LAID_OUT.steps.length,
      'the plan under test has to have steps, otherwise every point below runs on nothing',
    ).toBe(3);
    for (const step of LAID_OUT.steps) {
      expect(
        cardOf(DRAWN, step.id),
        'the step "' +
          step.name +
          '" is in the file and nothing on screen belongs to it. This picture is the one thing ' +
          'the run screen is about: a person watching four agents work reads it to find out ' +
          'what is left, and a step missing from it is work happening with nothing to see.',
      ).not.toBe('');
    }
  });

  it('keeps the steps in the order the file lists them', () => {
    expect(
      stepsIn(DRAWN),
      'the steps stand on screen in a different order than the file lists them. Order is the ' +
        'whole relation this picture carries — it draws no arrows — so a picture that reorders ' +
        'them states, in the only language it has, that the work happens in an order nobody ' +
        'wrote down (rule 17).',
    ).toEqual(IN_FILE);
  });

  it('says on every card which step it runs after, because the picture draws no arrows', () => {
    expect(
      textOf(cardOf(DRAWN, 'build')),
      'the card of the second step never says which step it comes after. The file joins the ' +
        'three steps and this picture has no arrow to show it with, so the relation has to ' +
        'arrive in words or it does not arrive at all.',
    ).toContain('after Plan the work');
    expect(
      textOf(cardOf(DRAWN, 'plan')),
      'the first step of the run says nothing about what it waits for. "Nothing comes before ' +
        'this one" is an answer a person needs on the step a run starts with.',
    ).toContain('first step');
    expect(
      textOf(cardOf(RENAMED_DRAWN, 'build')),
      'the same plan with the first step renamed still says the old name, so the sentence is ' +
        'written into the screen rather than read off the file. A picture that names a step ' +
        'this run does not have is worse than one that names none (rule 17).',
    ).toContain('after Draft the work');
  });

  it('never falls back to a canvas, whatever the file carries', () => {
    expect(
      DRAWN,
      'the file says where every step stands and the screen answers with the canvas again. ' +
        'Measured on 2026-08-31 in a 1512x950 window: tiles 40 px tall and a question card ' +
        '~120 px wide, which is the whole reason the path of steps exists. The canvas belongs ' +
        'to the workflow editor, where a person arranges it; a run is read top to bottom.',
    ).not.toContain(CANVAS);
    expect(FLATTENED, 'a plan with arrows but no places is drawn on a canvas').not.toContain(
      CANVAS,
    );
    expect(ONE_STEP, 'a plan of one step with no shape at all is drawn on a canvas').not.toContain(
      CANVAS,
    );
  });

  it('names the step of a plan that says nothing about its shape', () => {
    expect(
      textOf(ONE_STEP),
      'the step still has to be on screen — silence about the shape is not silence about the work',
    ).toContain('Answer the question');
  });

  it('draws every step of a plan that has arrows but nowhere to put them', () => {
    expect(
      stepsIn(FLATTENED),
      'the window builds this plan itself for a typed question, so it carries what the run ' +
        'knows and nothing more. Every step of it still belongs on screen, in the order it ' +
        'came in.',
    ).toEqual(IN_FILE);
  });
});
