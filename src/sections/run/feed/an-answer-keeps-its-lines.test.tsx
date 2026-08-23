/* To, co agent napisał w kilku wierszach, ma się czytać w kilku wierszach.
 *
 * # Skarga
 *
 * Właściciel, 2026-08-23, o strumieniu: „ten tekst niech też będzie jakoś fajnie i ładnie
 * formatowany aby było to przyjemniejsze".
 *
 * # Co było zepsute
 *
 * Model widoku przepuszcza tekst agenta NIETKNIĘTY (`feed/model.ts`, `sentence`), więc jego
 * przełamania dojeżdżały aż do DOM-u — i ginęły tam, bo domyślne `white-space` zamienia każdy
 * przełam w spację. Agent, który odpowiadał listą albo akapitami, dostawał na ekranie jeden
 * zbity blok. To nie był brak renderera markdown: to była utrata rzeczy, którą model naprawdę
 * napisał, już po tym, jak dojechała na miejsce.
 *
 * # Czego to kryterium NIE mówi
 *
 * Nie mówi „renderuj markdown". Renderer to nowa zależność, a `src/ui/shell/permissions.test.ts`
 * zapisuje wprost, czym to grozi w oknie z dostępem do powłoki — i taka decyzja należy do
 * człowieka (AGENTS.md §7). To kryterium pilnuje wyłącznie tego, żeby NIE GUBIĆ tego, co już
 * przyszło.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { createFeed } from './model';
import { Line } from './line';

const FORGE = 'Forge';

/** Odpowiedź w trzech wierszach — dokładnie to, co agenci piszą naprawdę. */
const ANSWER = 'Three districts came out ahead:\n- Wrzeszcz Gorny\n- Oliwa';

function markup(): string {
  const feed = createFeed(sealedScroller());
  feed.appendLines([line.step(1, 0, FORGE, ANSWER)]);
  const row = feed.view.history[0];
  if (row === undefined) return '';
  return renderToStaticMarkup(
    <Line
      row={row}
      onToggle={() => {
        /* To kryterium pyta o markup, nie o skutek kliknięcia. */
      }}
    />,
  );
}

/** Element otaczający zdanie agenta, razem z jego klasami. */
function wrapper(html: string): string {
  return /<span class="([^"]*)">Three districts/.exec(html)?.[1] ?? '';
}

describe('an answer keeps the shape the agent gave it', () => {
  const html = markup();

  it('put the answer on the screen at all, or the rest is about nothing', () => {
    expect(
      html.includes('Three districts came out ahead:'),
      'the answer never reached the markup, so every point below would be true of an empty ' +
        'string. It rendered: ' +
        JSON.stringify(html.slice(0, 200)),
    ).toBe(true);
  });

  it('keeps the line breaks the agent typed', () => {
    expect(
      /whitespace-pre-line/.test(wrapper(html)),
      'the answer is drawn with the default whitespace rule, which turns every line break into ' +
        'a space. An agent that answered with a list gets one solid block of prose, and the ' +
        'shape it chose to explain itself is gone. It carried: ' +
        JSON.stringify(wrapper(html)),
    ).toBe(true);
    /* PO CAŁYM TOKENIE, nie regexem z `\b`: granica słowa wypada także między `pre` a `-line`,
     * więc `/whitespace-pre\b/` trafiałaby w tę samą klasę, której ten punkt broni. Pierwsza
     * wersja tej asercji miała dokładnie ten błąd i była czerwona nad poprawnym kodem. */
    expect(
      wrapper(html).split(/\s+/).includes('whitespace-pre'),
      'but not the rule that also stops wrapping: a long answer would then run off the side ' +
        'of the column instead of folding, and the agents list gets pushed out of the window',
    ).toBe(false);
  });

  it('lets a long unbroken word fold instead of widening the column', () => {
    expect(
      /break-words|break-all/.test(wrapper(html)),
      'one long path or address with no spaces in it widens the stream column, and the column ' +
        'of agents beside it is what gets pushed off the screen',
    ).toBe(true);
  });
});
