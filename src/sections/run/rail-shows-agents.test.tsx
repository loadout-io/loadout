/* AC-2 dla T-39: lista agentów jest ZAMONTOWANA i pokazuje dokładnie to, co oddał `roster()`.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: zaimportować `Rail` i wyrenderować go wprost. Przechodzi ona
 * dzisiaj — bo komponent istnieje — i przechodziłaby także wtedy, gdyby nikt nigdy nie
 * zamontował go na ekranie. Dokładnie tak wyglądało `rail/{roster,card,colour,say}.ts` przez
 * trzydzieści zadań: cztery pliki z kompletem testów i ani jednego wołającego spoza nich.
 * Dlatego renderujemy CAŁY ekran Run i szukamy kafelków w JEGO markupie.
 *
 * WARTOŚCI OCZEKIWANE LICZY `roster()`, nie autor testu. Nazwa, zdanie, stan i token koloru są
 * tu wynikiem tej samej funkcji, którą woła ekran — wpisane z palca przechodziłyby także
 * wtedy, gdy ekran karmi listę czymś zupełnie innym (na przykład planem zamiast strumienia,
 * czyli cichą porażką numer trzy z T-09). Fakty o agentach składa test SAM, z tego samego
 * planu, który zasiał w magazynie: gdyby ekran ich nie brał pod uwagę, agent `failed`
 * pokazałby się jako `working` i ta różnica jest tu całą asercją (c).
 *
 * AGENT `failed` JEST TU CELOWO, bo tam pomyłka jest najłatwiejsza: kwadrat MUSI zostać
 * przygaszonym kolorem tożsamości, a stan MUSI być słowem w kolorze nasyconym [DESIGN §3].
 * Referencyjny poprzedni prototyp dawał agentowi Forge ten sam hex, co „wymaga uwagi" — i to jest
 * jedyny sposób, w jaki ta reguła się psuje: nie przez brak koloru, tylko przez ten sam.
 *
 * KONTROLA PRZECIW PUSTEJ ASERCJI: zanim cokolwiek porównamy, sprawdzamy, że `roster()` w ogóle
 * coś policzył i że parser wyciął z markupu prawdziwe kafelki. Porównanie dwóch pustych list
 * przechodzi na niczym.
 *
 * PIERWSZY RENDER POWSTAJE PRZED ZASIEWEM I TO JEST ISTOTNE: magazyn i model widoku żyją na
 * poziomie modułu (bieg trwa dłużej niż ekran), więc „pusty magazyn" da się zobaczyć tylko
 * raz i tylko na początku.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { useRun } from '../../state/run';
import type { FeedLine, Step } from '../../state/run';
import { runFeed } from './feed/live';
import { IDENTITY, STATUS, statusToken } from './rail/colour';
import type { AgentFacts } from './rail/roster';
import { roster } from './rail/roster';
import Run from './index';

/** Ekran z magazynem, do którego nikt nic nie włożył. Musi powstać PRZED zasiewem. */
const emptyMarkup = renderToStaticMarkup(<Run />);

/**
 * Plan biegu. Nazwa kroku JEST podpisem agenta w strumieniu — tak nadaje pompa zdarzeń
 * (`src-tauri/src/commands/run.rs`: `forward(…, self.plan.steps[id].name.clone())`).
 */
const STEPS: readonly Step[] = [
  { id: 'step-build', name: 'Build', state: 'running' },
  { id: 'step-check', name: 'Check', state: 'failed' },
];

/** Strumień: dwaj agenci, po jednej rzeczy każdy. Kafelek bierze się STĄD, nie z planu. */
const LINES: readonly FeedLine[] = [
  {
    kind: 'note',
    agent: 'Build',
    text: 'Rewriting the quote handling as a small state machine.',
    id: 1,
    at: 1_000,
  },
  {
    kind: 'ran',
    agent: 'Check',
    text: 'Ran the tests — they did not work',
    ok: false,
    preview: '3 of 40',
    detail: ['parser_handles_quoted_commas ... FAILED'],
    detailId: null,
    id: 2,
    at: 2_000,
  },
];

/** Te same fakty, które ekran składa z planu: podpis w strumieniu to nazwa kroku. */
const FACTS: readonly AgentFacts[] = STEPS.map((step) => ({
  id: step.name,
  name: step.name,
  role: '',
  step: step.state,
}));

useRun.setState({ steps: STEPS });
runFeed.appendLines(LINES);

const seededMarkup = renderToStaticMarkup(<Run />);
const cards = roster({ view: runFeed.view, agents: FACTS });

/** Sama lista agentów, wycięta z ekranu — reszta markupu nie ma prawa tu odpowiadać. */
function railOf(markup: string): string {
  const opens = markup.indexOf('<aside');
  const closes = markup.indexOf('</aside>');
  return opens < 0 || closes < 0 ? '' : markup.slice(opens, closes);
}

/** Kafelki listy, w kolejności renderowania. Każdy od swojego znacznika do następnego. */
function tilesOf(markup: string): readonly string[] {
  return railOf(markup)
    .split('<span data-agent="')
    .slice(1)
    .map((piece) => '<span data-agent="' + piece);
}

/** Podpis agenta, którego niesie ten kafelek. */
function agentOf(tile: string): string {
  return /data-agent="([^"]*)"/.exec(tile)?.[1] ?? '';
}

/** Tekst kafelka bez znaczników — wszystko, co człowiek na nim przeczyta. */
function textOf(tile: string): string {
  return tile.replace(/<[^>]*>/g, '');
}

/** Nazwa tokenu koloru z pierwszej deklaracji `var(--…)` w tym kawałku markupu. */
function colourNameIn(piece: string): string {
  return /var\((--[a-z0-9-]+)\)/.exec(piece)?.[1] ?? '';
}

/** Znacznik otwierający kwadratu tożsamości — element z inicjałem, `aria-hidden`. */
function squareOf(tile: string): string {
  return /<span aria-hidden[^>]*>/.exec(tile)?.[0] ?? '';
}

/** Znacznik otwierający słowa stanu. */
function statusWordOf(tile: string): string {
  return /<span[^>]*\bdata-status\b[^>]*>/.exec(tile)?.[0] ?? '';
}

describe('the agents list is mounted and shows what roster() handed it', () => {
  it('draws not one tile while the run has said nothing', () => {
    expect(
      railOf(emptyMarkup),
      'the run screen renders no agents list at all, so "it shows nothing" below would be a ' +
        'statement about a region that does not exist.',
    ).not.toBe('');
    expect(
      tilesOf(emptyMarkup),
      'with an empty run the agents list has to be EMPTY. A placeholder agent, a greyed-out ' +
        'row or an agent taken from the plan is a relation the data does not carry ' +
        '(invariant 17) — and it looks better than the correct version, which is exactly why ' +
        'it gets written.',
    ).toEqual([]);
  });

  it('draws one tile per agent that spoke, with the names and sentences roster() computed', () => {
    expect(
      cards.length,
      'roster() computed no tiles from the seeded stream, so every comparison below would run ' +
        'against an empty list and pass on nothing.',
    ).toBe(2);

    const tiles = tilesOf(seededMarkup);
    expect(
      tiles.length,
      'the mounted screen has to carry exactly one tile per agent that appeared in the stream. ' +
        'roster() says ' +
        String(cards.length) +
        '; the screen draws ' +
        String(tiles.length) +
        '.',
    ).toBe(cards.length);

    expect(
      tiles.map(agentOf),
      'the tiles have to stand in the order roster() returned — first appearance in the ' +
        'stream, never the order of the plan. In a parallel run those two orders almost never ' +
        'agree.',
    ).toEqual(cards.map((card) => card.id));

    for (const [index, card] of cards.entries()) {
      const tile = tiles[index] ?? '';
      expect(
        textOf(tile),
        'the tile for ' +
          card.id +
          ' has to carry the sentence roster() chose (' +
          JSON.stringify(card.say.text) +
          '). A tile that shows the latest note instead quotes the checks as if the agent had ' +
          'said them (00-SYNTHESIS §2.2), and the screen cannot tell you it did.',
      ).toContain(card.say.text);
      expect(textOf(tile), 'the tile for ' + card.id + ' has to carry its name').toContain(
        card.name,
      );
    }
  });

  it('keeps the identity colour on the square of a FAILED agent, and the state in the word', () => {
    const failed = cards.find((card) => card.status === 'failed');
    expect(
      failed,
      'the seeded plan has to produce one agent whose step failed, otherwise this case is ' +
        'about nothing. If the screen ignores the plan, every agent comes out as `working` — ' +
        'and that is the failure this assertion is for.',
    ).toBeDefined();
    if (failed === undefined) return;

    const tile = tilesOf(seededMarkup).find((piece) => agentOf(piece) === failed.id) ?? '';
    expect(tile, 'no tile in the markup carries the failed agent ' + failed.id).not.toBe('');

    const square = colourNameIn(squareOf(tile));
    const word = colourNameIn(statusWordOf(tile));

    expect(
      square,
      'the square of ' +
        failed.id +
        ' names no colour at all, so the two assertions below would compare empty ' +
        'strings.',
    ).not.toBe('');
    expect(
      IDENTITY,
      'the square has to carry the IDENTITY colour roster() assigned (' +
        failed.square +
        '), even for a failed agent. This is the whole of DESIGN §3: poprzedni prototyp painted its ' +
        'agent Forge with the exact hex that meant "needs your decision" one tile below.',
    ).toContain(square);
    expect(square, 'the square has to be the colour roster() computed for this agent').toBe(
      failed.square,
    );
    expect(
      STATUS,
      'the state has to be a WORD in a saturated state colour; the tile paints it with ' +
        JSON.stringify(word),
    ).toContain(word);
    expect(word, 'the state word has to carry the colour that colour.ts assigns to `failed`').toBe(
      statusToken(failed.status),
    );
    expect(
      square === word,
      'identity and state came out as the SAME colour (' +
        square +
        '). Two different things in one colour is the exact failure DESIGN §3 was written ' +
        'against, and it is invisible on a screenshot.',
    ).toBe(false);
    expect(
      textOf(tile),
      'the state has to be readable as a WORD, not only as a colour: a person who cannot tell ' +
        'the five muted greens apart still reads `failed`.',
    ).toContain(failed.status);
  });

  it('keeps the tile at four lines of text, never five', () => {
    const tile = tilesOf(seededMarkup)[0] ?? '';
    expect(tile, 'no tile was cut out of the markup, so counting its lines is free').not.toBe('');

    const card = cards[0];
    expect(card, 'roster() produced no first tile to compare against').toBeDefined();
    if (card === undefined) return;

    const lines = [...tile.matchAll(/<span[^>]*\bdata-card-line\b[^>]*>([\s\S]*?)<\/span>/g)].map(
      (hit) => (hit[1] ?? '').replace(/<[^>]*>/g, ''),
    );
    expect(
      lines.length,
      'the tile carries ' +
        String(lines.length) +
        ' text lines. Four is the ceiling [ARCHITECTURE §7, DESIGN §6]: name, role, one ' +
        'sentence, state. The fifth one is always a counter ("12 files · 2m 04s") — it looks ' +
        'right with one agent and breaks the list with four.',
    ).toBeLessThanOrEqual(4);
    expect(
      lines.length,
      'the tile has to carry a line for every text field roster() filled in, and nothing else',
    ).toBe([card.name, card.role, card.say.text, card.status].filter((text) => text !== '').length);

    /* Wszystko, co na kafelku widać, ma pochodzić z tych czterech pól plus inicjał na kwadracie.
     * Piąta linia dopisana BEZ znacznika `data-card-line` przewraca tę asercję, a nie licznik
     * wyżej — dlatego są tu obie. */
    expect(
      textOf(tile),
      'the tile shows text that roster() did not put in it. Everything on the tile comes from ' +
        'the four RailCard fields plus the initial on the square; anything else is a fifth ' +
        'line wearing a different attribute.',
    ).toBe(card.name.slice(0, 1) + [card.name, card.role, card.say.text, card.status].join(''));
  });
});
