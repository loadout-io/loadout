/* Sześć stanów, sześć rozpoznawalnych kafelków — sądzone w MARKUPIE, na prawdziwej drodze.
 *
 * DLACZEGO PRZEZ `RunGraph`, A NIE PRZEZ SAM KAFELEK. Kafelek wyrenderowany wprost przechodzi
 * także wtedy, gdy nic go nigdy nie montuje — to jest ta sama cicha porażka, którą niezmiennik
 * 29 nazywa po imieniu. Plan bez pozycji i bez strzałek renderuje LISTĘ kroków tym samym
 * kafelkiem, więc lista jest drogą, po której człowiek te sześć form naprawdę widzi.
 *
 * WSZYSTKIE SZEŚĆ KROKÓW MAJĄ TĘ SAMĄ NAZWĘ, TEGO SAMEGO WYKONAWCĘ I TO SAMO ZDANIE. Gdyby
 * różniły się nazwami, ich markup różniłby się z powodu nazwy i punkt o rozróżnialności
 * przechodziłby nad kafelkiem, który o stanie nie mówi ani jednym pikselem.
 *
 * ROZŁĄCZNOŚĆ FORM (T-47) jest tu sprawdzana wprost, a nie tylko przez skaner z sąsiedniego
 * pliku: `--live` wolno wystąpić jako podkład, obrys i pulsująca kropka, `--fail` jako glif
 * i lewa krawędź bloku. Obie barwy dzieli 13 stopni odcienia, więc jedyne, co je odróżnia,
 * to kształt.
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

/* Ten sam krok sześć razy, różny WYŁĄCZNIE stanem. */
const PLAN: Plan = {
  steps: SIX.map((status, at): GraphStep => ({
    id: `s${String(at)}`,
    name: 'Build the parser',
    status,
    who: { name: 'Forge', square: '--color-id-3' },
    doing: 'Rewriting the quote handling as a small state machine.',
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

describe('sześć stanów na płótnie biegu', () => {
  it('draws one card per step, so everything below has something to read', () => {
    expect(
      DRAWN.length,
      'the plan carries six steps and the markup has ' +
        String(DRAWN.length) +
        ' cards, so every point below would be reading an empty string and passing on nothing',
    ).toBe(6);
  });

  it('gives each of the six states a card nobody can mistake for another', () => {
    const shapes = new Set(DRAWN);
    expect(
      shapes.size,
      'two of the six states drew exactly the same card. Name, worker and sentence are equal by ' +
        'construction here, so anything the six share is everything they have — and a person ' +
        'watching cannot tell what is happening from what has already stopped',
    ).toBe(6);
  });

  it('leaves the step that waits as a broken outline with nothing inside it', () => {
    expect(of('waiting')).toContain('border-dashed');
    expect(of('waiting'), 'nothing has started here, so nothing may look filled').not.toContain(
      'bg-live-soft',
    );
  });

  it('washes the step that is working, and rings it', () => {
    expect(of('working')).toContain('bg-live-soft');
    expect(of('working')).toContain('border-live-edge');
    /* Trzecia forma z listy `--live`, i jedyna, którą trzeba było napisać samemu: cień w tej
     * barwie nie ma tokenu, więc niesie ją `./graph.css`. Bez niej wypełnienie 16% alfy jest
     * na ciemnym tle ledwo widoczne i „pracuje" różni się od „czeka" jedną kreską obrysu. */
    expect(
      of('working'),
      'the wash and the ring are all this card has to say that the work is happening right now: ' +
        'the movement that would say it louder has nowhere to live yet, and the reason is ' +
        'written at the top of tile.tsx',
    ).toContain('loadout-run-glow');
  });

  it('outlines the step that waits for a person, and fills nothing', () => {
    expect(of('needs you')).toContain('border-attend-edge');
    expect(
      of('needs you'),
      'this step waits for a person, and a wash means the machine is busy — the two must not ' +
        'look alike',
    ).not.toContain('bg-live-soft');
  });

  it('quiets the step that finished and gives it no mark at all', () => {
    expect(of('done')).toContain('opacity-50');
    expect(of('done'), 'a finished step is quiet, never filled').not.toContain('bg-live-soft');
    expect(of('done')).not.toContain('✕');
  });

  it('marks the step that broke with a glyph and an edge down its side', () => {
    expect(of('failed')).toContain('✕');
    expect(of('failed')).toContain('border-l-fail-edge');
    expect(
      of('failed'),
      'the broken colour is a glyph and the left edge of the block, never a wash behind the ' +
        'whole card — a wash is what the happening-now colour means, and the two hues sit 13 ' +
        'degrees apart',
    ).not.toContain('bg-fail-soft');
  });

  it('crosses out the mark on the step somebody stopped', () => {
    expect(of('stopped')).toContain('⊘');
    expect(of('stopped')).toContain('opacity-50');
    expect(
      of('stopped'),
      'nobody broke this step, so the broken colour has no place',
    ).not.toContain('fail');
  });

  it('never lets the happening-now colour and the broken colour meet on one card', () => {
    /* KONTROLA PRZECIW PUSTEJ ZIELENI. Rozłączność dwóch pustych zbiorów zachodzi za darmo,
     * więc bez tych dwóch linii ten punkt przechodzi nad kafelkiem, który nie ma ani jednej
     * barwy — czyli nad komponentem, który nie rysuje niczego. */
    expect(
      DRAWN.some((card) => card.includes('live')),
      'not one of the six cards carries the happening-now colour, so the disjointness below ' +
        'would hold on an empty set and report green about nothing',
    ).toBe(true);
    expect(
      DRAWN.some((card) => card.includes('fail')),
      'not one of the six cards carries the broken colour, so the disjointness below would ' +
        'hold on an empty set and report green about nothing',
    ).toBe(true);

    for (const status of SIX) {
      const card = of(status);
      expect(
        card.includes('live') && card.includes('fail'),
        'the card for a step that is ' +
          status +
          ' carries both the happening-now colour and the broken colour, and a shape that means ' +
          'both means neither',
      ).toBe(false);
    }
  });

  it('keeps every card at four lines of text, never five', () => {
    for (const status of SIX) {
      const lines = [...of(status).matchAll(/\bdata-card-line\b/g)].length;
      expect(
        lines,
        'the card for a step that is ' +
          status +
          ' carries ' +
          String(lines) +
          ' lines of text. Four is the ceiling [ARCHITECTURE §7]: what the step is, who does it, ' +
          'what it is doing now, and the measure. The fifth is always a counter that looks right ' +
          'on one card and breaks the picture on four',
      ).toBe(4);
    }
  });

  it('paints the identity square from the colour the agents list already gave this worker', () => {
    expect(
      of('working'),
      'the square is identity and never state, so it reads the same colour here as it does in ' +
        'the list beside it',
    ).toContain('var(--color-id-3)');
  });
});
