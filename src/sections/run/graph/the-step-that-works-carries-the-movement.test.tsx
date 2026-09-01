/* Ruch na obrazie planu należy do KROKU, który pracuje — i tylko do niego.
 *
 * CO SIĘ ZMIENIŁO 2026-08-31 I DLACZEGO. Sufit z ARCHITECTURE §7 to DWA regiony ruszające się
 * od jednego zdarzenia, liczone przez `../exactly-one-thing-pulses.test.ts` jako RODZAJE
 * ruszającej się rzeczy. Rodzajów było trzy: kropka żywej karty w tle, kreska płynąca po
 * strzałce i — zamówiona — kropka na kroku, który pracuje. Trzeci rodzaj nie mieści się pod
 * sufitem, a wyrocznia licząca literały `animate-*` strzałki nie widzi: jej ruch definiuje
 * arkusz biblioteki, a kod mówi tylko `animated`. Zieleń tamtego punktu przy trzech ruszających
 * się rzeczach byłaby więc zielenią nad przekroczonym sufitem.
 *
 * WYPADŁA STRZAŁKA, i to nie jest wybór najtańszego. Strzałka i kropka odpowiadają na TO SAMO
 * pytanie — „tu dzieje się praca" — a limit żywych regionów na jeden fakt wynosi 1
 * (niezmiennik 13). Z dwóch nośników jednego faktu zostaje ten, który stoi NA rzeczy, o której
 * mówi: pierwszy krok każdego biegu nie ma strzałki wchodzącej, więc strzałka i tak milczała
 * dokładnie wtedy, gdy bieg dopiero rusza.
 *
 * STRZAŁKA NIE TRACI NIC POZA RUCHEM. Barwę „dzieje się teraz" niesie dalej — dziś klasą,
 * nie ruchem — więc ścieżkę, którą przyszła praca, widać także z wyłączonymi animacjami.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { AgentStatus } from '../rail/card';
import type { GraphStep, Plan } from './model';
import { RunGraph, edgesFor } from './graph';
import { LIVE_ARROW } from './model';

const SIX: readonly AgentStatus[] = [
  'waiting',
  'working',
  'needs you',
  'done',
  'failed',
  'stopped',
];

const PLAN: Plan = {
  steps: SIX.map((status, at): GraphStep => ({
    id: `s${String(at)}`,
    name: 'Build the parser',
    status,
    who: { name: 'Forge', square: '--color-id-3' },
    doing: 'Rewriting the quote handling as a small machine with three positions.',
  })),
  links: [],
};

const MARKUP = renderToStaticMarkup(<RunGraph plan={PLAN} />);

/** Markup każdego kafelka z osobna, w kolejności planu, bez jego klucza. */
function cards(markup: string): readonly string[] {
  return markup
    .split('data-step="')
    .slice(1)
    .map((chunk) => chunk.slice(chunk.indexOf('"') + 1));
}

const DRAWN = cards(MARKUP);
const of = (status: AgentStatus): string => DRAWN[SIX.indexOf(status)] ?? '';

/** Plan z układem: praca stoi na środkowym kroku, więc przyszła pierwszą strzałką. */
const RUNNING: Plan = {
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

describe('ruch stoi na kroku, który pracuje', () => {
  it('draws one card per step, so everything below has something to read', () => {
    expect(
      DRAWN.length,
      'the plan carries six steps and the markup has ' +
        String(DRAWN.length) +
        ' cards, so every point below would be reading an empty string',
    ).toBe(6);
  });

  it('moves something on the card of the step that is working right now', () => {
    expect(
      of('working'),
      'the card of a step that is working carries nothing that moves. A wash, a ring and a ' +
        'glow are all still, and stillness is what every other card on this picture says: the ' +
        'one question a person asks a run screen is which of these is happening NOW [DESIGN §7]',
    ).toContain('animate-');
  });

  it('leaves every other card on the picture still', () => {
    const moving = SIX.filter((status) => status !== 'working' && of(status).includes('animate-'));
    expect(
      moving,
      'these states also move: ' +
        JSON.stringify(moving) +
        '. Movement everywhere is movement nowhere — the eye chases it instead of reading, and ' +
        'a step that has already stopped is the last thing that may look busy',
    ).toEqual([]);
  });

  it('marks the arrow the work came along, so the path is readable without motion', () => {
    const marked = edgesFor(RUNNING).filter((arrow) =>
      (arrow.className ?? '').includes(LIVE_ARROW),
    );
    expect(
      marked.map((arrow) => arrow.id),
      'the model knows which arrow the work travelled and this is the only place that answer ' +
        'reaches the drawing. A version that keeps it to itself stays green while the picture ' +
        'says nothing.',
    ).toEqual(['plan->build']);
  });

  it('lets no arrow move, because the step it points at already carries that fact', () => {
    const shifting = edgesFor(RUNNING).filter((arrow) => arrow.animated === true);
    expect(
      shifting.map((arrow) => arrow.id),
      'these arrows still move: ' +
        JSON.stringify(shifting.map((arrow) => arrow.id)) +
        '. Together with the dot on the step and the dot on a card in the background that is a ' +
        'third kind of moving thing, and ARCHITECTURE §7 allows two. The arrow and the dot ' +
        'answer the same question, so the one that stands on the thing it talks about stays.',
    ).toEqual([]);
  });
});
