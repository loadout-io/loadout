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
 * DLACZEGO PIERWSZY PLAN NIE MA POZYCJI. To repo nie ma jsdom, a React Flow mierzy kafelki
 * dopiero w przeglądarce: pod `renderToStaticMarkup` oddaje ramę płótna z PUSTYMI pojemnikami.
 * Droga, po której człowiek widzi kafelki w tym środowisku, to lista — czyli plan bez układu,
 * ten sam, który okno składa dla wpisanego pytania. Że plan Z układem dostaje płótno, a nie
 * listę, sądzi osobno ostatni punkt, na obecności ramy.
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

/** Nazwa tokenu barwy z pierwszej deklaracji `var(--…)` w tym kawałku markupu. */
function colourIn(piece: string): string {
  return /var\((--[a-z0-9-]+)\)/.exec(piece)?.[1] ?? '';
}

describe('the plan is on the run screen', () => {
  it('carries one card per step of the run', () => {
    expect(
      LAID_OUT.length,
      'the seeded run has to have steps, otherwise every comparison below runs on nothing',
    ).toBe(2);
    for (const step of LAID_OUT) {
      expect(
        listed,
        'the run screen shows nothing for step ' +
          step.name +
          '. The plan is the one thing this screen is about, and a drawing nobody mounts is ' +
          'the failure this repository exists for.',
      ).toContain('data-step="' + step.id + '"');
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

  it('draws the picture when the file says where every step stands', () => {
    expect(
      drawn,
      'both steps carry a place and the file joins them, so there is a real shape to show — ' +
        'and a screen that lists them anyway leaves the drawing unreachable in the product',
    ).toContain(CANVAS);
    expect(
      listed,
      'nothing in the first plan says where a step stands, so a picture would have to invent ' +
        'it (rule 17), and the list is the honest answer',
    ).not.toContain(CANVAS);
  });
});
