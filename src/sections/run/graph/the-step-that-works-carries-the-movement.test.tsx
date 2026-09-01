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
 *
 * 2026-08-31, DRUGA ZMIANA TEGO DNIA: DWA OSTATNIE PUNKTY PYTAŁY `edgesFor`, CZYLI STRZAŁKI
 * PŁÓTNA. Płótno zeszło z ekranu biegu w całości (`./graph.tsx`, nagłówek) i `edgesFor` zeszło
 * razem z nim, bo nie miało już ani jednego wołającego. Pytanie „czy praca ma na obrazie jeden
 * nośnik, a nie dwa" zostaje i jest dziś MOCNIEJSZE: zamiast pytać funkcję o pole `animated`,
 * które nikt nie renderował, liczy ruszające się rzeczy w markupie, który widzi człowiek
 * (niezmiennik 29). Drugie pytanie tamtych punktów — którędy praca przyszła — odpowiada dziś
 * zdanie na karcie („after Reproduce") i jest sądzone tam, gdzie stoi
 * (`./a-plan-without-a-layout-shows-a-list.test.tsx`).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { AgentStatus } from '../rail/card';
import type { GraphStep, Plan } from './model';
import { RunGraph } from './graph';

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

  it('lets one thing move on the whole picture, and it is the card that is working', () => {
    const moving = [...MARKUP.matchAll(/animate-[a-z0-9-]+/g)].map((hit) => hit[0]);

    expect(
      moving,
      'the picture of this run has six steps and exactly one of them is working, so exactly one ' +
        'thing on it may move. It carries ' +
        String(moving.length) +
        ': ' +
        JSON.stringify([...new Set(moving)]) +
        '. ARCHITECTURE §7 allows two kinds of moving region in the whole application, and the ' +
        'live card in the background already spends one. Movement everywhere is movement ' +
        'nowhere — the eye chases it instead of reading.',
    ).toHaveLength(1);

    expect(
      of('working'),
      'the one moving thing on the picture stands somewhere other than the card of the step ' +
        'that is working. A run screen answers one question at a glance — which of these is ' +
        'happening NOW — and the answer has to stand on the thing it talks about.',
    ).toContain(moving[0] ?? '');
  });
});
