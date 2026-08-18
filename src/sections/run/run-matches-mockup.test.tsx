/* AC-1 dla T-39: układ ekranu Run jest układem z makiety, i to MAKIETA jest wyrocznią.
 *
 * DLACZEGO WARTOŚCI OCZEKIWANE SĄ CZYTANE, A NIE WPISANE. Słabą wersją tego kryterium jest
 * `expect(markup).toContain('268px')`. Przechodzi ona w dwóch przypadkach, w których układ jest
 * zepsuty: gdy `268px` stoi gdziekolwiek w markupie — także jako wysokość czegoś zupełnie
 * innego — i gdy makieta zmieni się na 300, a ekran nie. Odróżnia je to, że oczekiwana wartość
 * jest **czytana z `docs/mockup/index.html` w tym samym biegu testu**: kiedy pliki się rozjadą,
 * test pada, i to jest jego jedyne zadanie. Ten sam zabieg stoi w
 * `src/ui/shell/shell-matches-mockup.test.tsx` na regule `.app`.
 *
 * PORÓWNUJEMY CAŁĄ DEKLARACJĘ, NIE SAMĄ LICZBĘ. `.work` mówi `minmax(0,1fr) 268px`, i to
 * `minmax(0,1fr)` jest połową sensu: bez niego szeroki wiersz strumienia rozpycha kolumnę
 * zamiast się przewijać, a lista agentów zjeżdża z ekranu. Asercja na samej liczbie
 * przepuściłaby `1fr 268px`. Tak samo `.feedcol`: `minmax(0,1fr) auto auto` znaczy „historia
 * bierze resztę, TERAZ i wiersz wejścia mają wysokość swojej treści" — `auto auto auto`
 * wygląda identycznie przy pustym biegu i rozjeżdża się przy pierwszej setce linii.
 *
 * KONTROLA PRZECIW PUSTEMU PORÓWNANIU. Parser, który cicho nic nie dopasował, dałby dwa puste
 * napisy i porównanie przeszłoby na niczym. Dlatego każdy odczyt z makiety ma osobną asercję
 * na to, że coś realnie znalazł, i na to, że ma spodziewaną liczbę członów.
 *
 * KOLEJNOŚĆ KOLUMN JEST CZĘŚCIĄ UKŁADU, nie stylem: w siatce dwukolumnowej pierwsze dziecko
 * bierze pierwszą kolumnę, więc lista agentów wyrenderowana przed kolumną strumienia stanęłaby
 * po lewej i miałaby 268 px zamiast reszty ekranu — przy identycznej deklaracji siatki.
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby test padał na asercji
 * o treści, nigdy na otwarciu pliku (AGENTS.md §2a p. 5).
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import Run from './index';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Spłaszcza odstępy, żeby `minmax(0, 1fr)  268px` i `minmax(0,1fr) 268px` były równe. */
function tight(value: string): string {
  return value.replace(/\s+/g, ' ').replace(/,\s+/g, ',').trim();
}

/** Ciało reguły CSS o podanym selektorze, z pierwszego wystąpienia. */
function ruleBody(css: string, selector: string): string {
  const found = new RegExp('\\' + selector + '\\s*\\{([^}]*)\\}').exec(css);
  return found?.[1] ?? '';
}

/** Wartość jednej właściwości z ciała reguły. */
function property(body: string, name: string): string {
  const found = new RegExp('(?:^|;)\\s*' + name + '\\s*:([^;]*)').exec(body);
  return tight(found?.[1] ?? '');
}

/** Znacznik otwierający elementu niosącego ten atrybut — razem z całym jego stylem. */
function openingTag(markup: string, attribute: string): string {
  return new RegExp('<[a-z]+[^>]*\\b' + attribute + '="[^"]*"[^>]*>').exec(markup)?.[0] ?? '';
}

/** Deklaracja siatki wyrenderowana na tym elemencie, bez nazwy właściwości. */
function gridOf(markup: string, attribute: string, property_: string): string {
  const style = /style="([^"]*)"/.exec(openingTag(markup, attribute))?.[1] ?? '';
  const found = new RegExp('(?:^|;)\\s*' + property_ + '\\s*:([^;]*)').exec(style);
  return tight(found?.[1] ?? '');
}

const html = fileText(MOCKUP);
const markup = renderToStaticMarkup(<Run />);

describe('the run screen is the layout the mockup draws, read from the mockup', () => {
  it('gives the work area two columns, the second one the width the mockup says', () => {
    const wanted = property(ruleBody(html, '.work'), 'grid-template-columns');

    expect(
      wanted,
      'nothing was read out of the `.work` rule in docs/mockup/index.html, so the comparison ' +
        'below would run between two empty strings and pass on nothing. Either the file moved ' +
        'or the rule stopped declaring grid-template-columns.',
    ).not.toBe('');
    expect(
      wanted.split(' ').length,
      'the mockup has to declare TWO columns for the agents list to stand beside the stream ' +
        'rather than under it. It declares: ' +
        wanted,
    ).toBe(2);

    const rendered = gridOf(markup, 'data-work', 'grid-template-columns');
    expect(
      rendered,
      'the run screen renders no work area declaring grid-template-columns, so nothing says ' +
        'the agents list stands beside the stream. Markup starts: ' +
        markup.slice(0, 200),
    ).not.toBe('');

    expect(
      rendered,
      'the screen and the mockup disagree about the work grid. The mockup `.work` rule is the ' +
        'oracle and it says `' +
        wanted +
        '`. Reading it here, in this run, is the whole point: an assertion that spelled the ' +
        'number out would also pass when the mockup changes and the screen does not.',
    ).toBe(wanted);
  });

  it('gives the stream column the three rows of the mockup, in its order', () => {
    const wanted = property(ruleBody(html, '.feedcol'), 'grid-template-rows');

    expect(
      wanted,
      'nothing was read out of the `.feedcol` rule in docs/mockup/index.html, so the ' +
        'comparison below would pass on two empty strings.',
    ).not.toBe('');
    expect(
      wanted.split(' ').length,
      'the mockup has to declare THREE rows: history, the NOW zone and the entry row. It ' +
        'declares: ' +
        wanted,
    ).toBe(3);

    const rendered = gridOf(markup, 'data-stream-column', 'grid-template-rows');
    expect(
      rendered,
      'the run screen renders no stream column declaring grid-template-rows. Without it the ' +
        'entry row and the NOW zone stop being pinned to the bottom and the history stops ' +
        'being the only part that scrolls.',
    ).not.toBe('');

    expect(
      rendered,
      'the screen and the mockup disagree about the stream column. The mockup `.feedcol` rule ' +
        'says `' +
        wanted +
        '`, and the order of those three tracks IS the layout: the first one takes the free ' +
        'space, the other two take the height of their content.',
    ).toBe(wanted);
  });

  it('renders the stream column before the agents list, so it takes the first column', () => {
    const streamAt = markup.indexOf('data-stream-column');
    const railAt = markup.indexOf('data-rail');

    expect(streamAt, 'the run screen renders no stream column at all').toBeGreaterThanOrEqual(0);
    expect(railAt, 'the run screen renders no agents list at all').toBeGreaterThanOrEqual(0);
    expect(
      streamAt,
      'the stream column has to come first in the markup, because in a two-column grid the ' +
        'first child takes the first column. Rendered the other way round the agents list gets ' +
        'the free space and the stream gets 268 px — with the grid declaration still correct.',
    ).toBeLessThan(railAt);
  });
});
