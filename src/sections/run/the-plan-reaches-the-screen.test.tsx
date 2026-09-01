/* Ekran pracy pokazuje PLAN, a nie trzy kopie jednego zdania.
 *
 * ZMIERZONA WADA (2026-08-31, zgłoszenie właściciela). Ten sam fakt — „Check skończył i nie
 * wyszło" — stał na ekranie Run trzy razy naraz: raz w strumieniu, raz w bloku TERAZ pod nim,
 * raz na kafelku w prawej kolumnie. Limit żywych regionów na fakt wynosi 1 (niezmiennik 13).
 * Blok TERAZ miał do tego dwie własne wady: rósł z każdym agentem, choć DESIGN §1 żąda stałej
 * wysokości („nie rośnie, mutuje"), a po końcu biegu zdejmował nagłówek i zostawiał wiersze —
 * więc ostatni stan sprzed zakończenia wyglądał jak stan bieżący, tylko bez etykiety.
 *
 * DLACZEGO CAŁY EKRAN, A NIE SAM RYSUNEK. Rysunek wyrenderowany wprost przechodzi także
 * wtedy, gdy nikt go nigdy nie zamontował — to jest ta sama cicha porażka, którą niezmiennik 29
 * nazywa po imieniu, i dokładnie to, co przydarzyło się `rail/{roster,card,colour,say}.ts`
 * przez trzydzieści zadań. Renderujemy więc `<Run />` i pytamy JEGO markup.
 *
 * DLACZEGO SĄ TU DWA PLANY, JEDEN Z POZYCJAMI I JEDEN BEZ. 2026-08-31 płótno React Flow zeszło
 * z ekranu biegu w całości (`./graph/graph.tsx`, nagłówek): kroki rysują się od dziś jako jedna
 * pionowa ścieżka, niezależnie od tego, czy plik mówi, gdzie stoi który krok. Do tego dnia
 * ścieżkę dostawał WYŁĄCZNIE plan bez zapisanych pozycji — czyli plan, który okno składa samo
 * dla wpisanego pytania — a każdy prawdziwy workflow pozycje ma, więc w produkcie zawsze wypadało
 * płótno: kafelki wysokie na 40 px i karta pytania szeroka na ~120 px (zmierzone na zrzucie okna
 * 1512×950). Oba plany stoją tu więc po to, żeby powiedzieć, że obraz jest JEDEN, i po to, żeby
 * regresja przywracająca warunek padła natychmiast.
 *
 * WARTOŚCI OCZEKIWANE LICZY `roster()` I MODEL STRUMIENIA, nie autor testu. Zdanie kafelka
 * i barwa kwadratu są tu wynikiem tych samych funkcji, które woła ekran — wpisane z palca
 * przechodziłyby także wtedy, gdy ekran karmi rysunek czymś zupełnie innym.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { FeedLine, Step } from '../../state/run';
import { useRun } from '../../state/run';
import type { Link } from '../../state/workflows';
import { line } from './feed/fixtures/lines';
import { runFeed } from './feed/live';
import { identityToken } from './rail/colour';
import Run from './index';

const BUILD = 'Build';
const CHECK = 'Check';

/** Plan z pliku workflow: obie pozycje i jedna strzałka między nimi. */
const LAID_OUT: readonly Step[] = [
  { id: 's_build', name: BUILD, state: 'running', at: { x: 0, y: 0 } },
  { id: 's_check', name: CHECK, state: 'failed', at: { x: 264, y: 0 } },
];

/** Ten sam bieg, o którego kształcie okno nic nie wie — tak wygląda plan wpisanego pytania. */
const FLAT: readonly Step[] = LAID_OUT.map(({ at: _at, ...rest }) => rest);

const LINKS: readonly Link[] = [{ from: 's_build', to: 's_check' }];

const SAID = 'Rewriting the quote handling as a small state machine.';

/* Podpis agenta w strumieniu JEST nazwą kroku — tak nadaje pompa zdarzeń
 * (`src-tauri/src/commands/run.rs`: `forward(…, step.name)`). Na tym jednym polu spotykają
 * się plan i strumień, i to jest jedyne prawdziwe połączenie, jakie w tych danych istnieje. */
const LINES: readonly FeedLine[] = [
  line.note(1, 0, BUILD, SAID),
  line.ran(2, 400, CHECK, 'Ran the checks — they did not work', false, ['3 of 40']),
];

useRun.setState({ workflow: 'Fix the CSV parser', steps: FLAT, links: null });
runFeed.appendLines(LINES);
const listed = renderToStaticMarkup(<Run />);

useRun.setState({ steps: LAID_OUT, links: LINKS });
const drawn = renderToStaticMarkup(<Run />);

/** To, co React Flow zawsze stawia wokół płótna. Nie ma go — nie ma płótna. */
const CANVAS = 'react-flow__pane';

/** Klucze kroków w kolejności, w jakiej stoją w markupie. */
function stepsIn(markup: string): readonly string[] {
  return [...markup.matchAll(/data-step="([^"]*)"/g)].map((hit) => hit[1] ?? '');
}

/** Markup jednego kafelka: od jego znacznika do znacznika następnego. */
function tileOf(markup: string, id: string): string {
  const opens = markup.indexOf('data-step="' + id + '"');
  if (opens < 0) return '';
  const rest = markup.slice(opens);
  const next = rest.indexOf('data-step="', 1);
  return next < 0 ? rest : rest.slice(0, next);
}

/** Tekst, który człowiek na tym kafelku przeczyta — bez znaczników. */
function textOf(piece: string): string {
  return piece.replace(/<[^>]*>/g, '');
}

/**
 * To samo, ale ze ZNACZNIKIEM ZAMIENIONYM NA ODSTĘP — i to jest różnica mierzalna, nie gust.
 *
 * `textOf` skleja sąsiednie węzły tekstowe w jeden wyraz („Build" + „working" = „Buildworking"),
 * więc szukanie pojedynczego SŁOWA z granicą wyrazu nie znajduje w nim niczego. Punkt o stanie
 * kroku pyta właśnie o jedno słowo, więc czyta kartę tak, jak czyta ją człowiek: z przerwą tam,
 * gdzie na ekranie jest przerwa.
 */
function wordsOf(piece: string): string {
  return piece.replace(/<[^>]*>/g, ' ');
}

/** Nazwa tokenu barwy z pierwszej deklaracji `var(--…)` w tym kawałku markupu. */
function colourIn(piece: string): string {
  return /var\((--[a-z0-9-]+)\)/.exec(piece)?.[1] ?? '';
}

describe('the plan is on the run screen', () => {
  it('carries one card per step of the run, whatever the file says about places', () => {
    expect(
      LAID_OUT.length,
      'the seeded run has to have steps, otherwise every comparison below runs on nothing',
    ).toBe(2);
    for (const markup of [listed, drawn]) {
      for (const step of LAID_OUT) {
        expect(
          markup,
          'the run screen shows nothing for step ' +
            step.name +
            '. The plan is the one thing this screen is about, and a drawing nobody mounts is ' +
            'the failure this repository exists for.',
        ).toContain('data-step="' + step.id + '"');
      }
    }
  });

  it('says on that card what the stream says the worker is doing now', () => {
    const wanted = runFeed.view.now.rows.find((row) => row.agent === BUILD)?.text ?? '';
    expect(
      wanted,
      'the stream carries no live sentence for ' +
        BUILD +
        ', so the comparison below would pass on two empty strings',
    ).not.toBe('');

    expect(
      textOf(tileOf(listed, 's_build')),
      'the card for ' +
        BUILD +
        ' has to carry the sentence the run model already holds (' +
        JSON.stringify(wanted) +
        '). Standing it in a second place under the stream was the duplicate this change ' +
        'removes; standing it nowhere loses it.',
    ).toContain(wanted);
  });

  it('paints the identity square of a failed step with its identity colour', () => {
    const tile = tileOf(listed, 's_check');
    expect(tile, 'no card in the markup belongs to the step that failed').not.toBe('');

    expect(
      colourIn(tile),
      'the square is who does the work and never how it went [DESIGN §3]. poprzedni prototyp painted ' +
        'its worker Forge with the exact colour that meant "needs your decision" one card ' +
        'below, and that is the only way this rule ever breaks: not by losing the colour, ' +
        'but by reusing it.',
    ).toBe(identityToken(CHECK));
  });

  it('states each fact once: no second copy under the stream, beside it, or above it', () => {
    expect(
      listed,
      'the live block under the stream repeated, word for word, what the stream had just ' +
        'said, and it grew with every worker while DESIGN §1 asks for a fixed height',
    ).not.toContain('data-now');
    expect(
      listed,
      'the agents list said the same thing a third time, in its own column',
    ).not.toContain('data-agent=');
    expect(
      listed,
      'the row of step blocks in the loadout bar was a second drawing of the plan; the ' +
        'picture above IS that row now',
    ).not.toContain('data-blocks');
  });

  it('draws one path, in the order of the file, for the plan that carries places too', () => {
    expect(
      stepsIn(drawn),
      'the two steps carry a place and the file joins them, and the screen answers with a ' +
        'different picture than it gives the same run without places. There is one run and one ' +
        'answer to "what is this work": the steps top to bottom, in the order the file lists ' +
        'them. Reordering them states, in the only language this picture has, an order nobody ' +
        'wrote down (rule 17).',
    ).toEqual(LAID_OUT.map((step) => step.id));
    expect(
      drawn,
      'the file says where every step stands and the run screen answers with the canvas. ' +
        'Measured 2026-08-31 in a 1512x950 window: tiles 40 px tall, unreadable, and a question ' +
        'card ~120 px wide. Every real workflow carries places, so this was the picture a person ' +
        'actually got — while the point about the path of steps stayed green over a plan the ' +
        'product never draws. The canvas belongs to the workflow editor.',
    ).not.toContain(CANVAS);
    expect(
      listed,
      'the same run without places is drawn on a canvas, which would have to invent them (rule 17)',
    ).not.toContain(CANVAS);
  });

  it('says on each card which step it runs after, because the picture draws no arrows', () => {
    expect(
      textOf(tileOf(drawn, 's_check')),
      'the file joins the two steps and the card of the second one never says what it comes ' +
        'after. This picture draws no arrows at all, so the relation the file states has to ' +
        'arrive in words on the card or it does not arrive on the screen at all.',
    ).toContain('after ' + BUILD);
    expect(
      textOf(tileOf(drawn, 's_build')),
      'the step the run starts with says nothing about what it waits for, so a person reading ' +
        'the top of the picture cannot tell it is the top',
    ).toContain('first step');
  });

  it('tells the state of one step apart from another on the screen, not only in the data', () => {
    const WORD = /\b(?:working|waiting|needs you|done|failed|stopped)\b/;
    const building = WORD.exec(wordsOf(tileOf(drawn, 's_build')))?.[0] ?? '';
    const checking = WORD.exec(wordsOf(tileOf(drawn, 's_check')))?.[0] ?? '';

    expect(
      building,
      'the card of the step that is running says nowhere, in words a person can read, what ' +
        'state it is in. Shape and colour alone leave out everybody who does not separate two ' +
        'dimmed hues, and a name for the screen reader answers blindness, not colour vision.',
    ).not.toBe('');
    expect(checking, 'the card of the step that failed carries no state in words').not.toBe('');
    expect(
      checking,
      'the step that is running and the step that failed read the same word ("' +
        building +
        '"), so the one question a person brings to this screen — which of these is happening ' +
        'now and which went wrong — has no answer on it.',
    ).not.toBe(building);
  });
});
