/* AC-1 dla T-37: układ powłoki zgadza się z makietą, i to MAKIETA jest wyrocznią.
 *
 * DLACZEGO WARTOŚĆ OCZEKIWANA JEST CZYTANA, A NIE WPISANA. Słabą wersją tego kryterium jest
 * `expect(markup).toContain('196px')`. Przechodzi ona w dwóch przypadkach, w których układ jest
 * zepsuty: gdy `196px` stoi gdziekolwiek w markupie — także w poziomym pasku o szerokości
 * 196 px — i gdy makieta zmieni się na 220, a powłoka nie. Odróżnia je to, że oczekiwana wartość
 * jest **czytana z `docs/mockup/index.html` w tym samym biegu testu**: kiedy pliki się rozjadą,
 * test pada, i to jest jego jedyne zadanie.
 *
 * Tu nie porównujemy samej pierwszej kolumny, a CAŁĄ deklarację `grid-template-columns`. Reguła
 * `.app` mówi `196px minmax(0,1fr)`, i to `minmax(0,1fr)` jest połową sensu: bez niego szeroka
 * treść rozpycha kolumnę zamiast się przewijać. Asercja na samej liczbie przepuściłaby `1fr`.
 *
 * KONTROLA PRZECIW PUSTEMU PORÓWNANIU. Parser, który cicho nic nie dopasował, dałby dwa puste
 * napisy i porównanie przeszłoby na niczym — ta sama wada, którą AC-2 zamyka punktem (c).
 * Dlatego każdy odczyt z makiety ma osobną asercję na to, że coś realnie znalazł.
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby test padał na asercji
 * o treści, nigdy na otwarciu pliku (AGENTS.md §2a p. 5).
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Spłaszcza odstępy, żeby `196px  minmax(0, 1fr)` i `196px minmax(0,1fr)` były równe. */
function tight(value: string): string {
  return value.replace(/\s+/g, ' ').replace(/,\s+/g, ',').trim();
}

/** Ciało reguły CSS o podanym selektorze, z pierwszego wystąpienia. */
function ruleBody(css: string, selector: string): string {
  const found = new RegExp(`\\${selector}\\s*\\{([^}]*)\\}`).exec(css);
  return found?.[1] ?? '';
}

/** Wartość jednej właściwości z ciała reguły. */
function property(body: string, name: string): string {
  const found = new RegExp(`(?:^|;)\\s*${name}\\s*:([^;]*)`).exec(body);
  return tight(found?.[1] ?? '');
}

/** Etykiety przełączników z `<nav class="nav">` makiety, w kolejności wystąpienia. */
function mockupNavLabels(html: string): readonly string[] {
  const nav = /<nav class="nav">([\s\S]*?)<\/nav>/.exec(html)?.[1] ?? '';
  return [...nav.matchAll(/<button[^>]*data-go="[^"]*"[^>]*>\s*<span>([^<]*)<\/span>/g)].map(
    (hit) => hit[1]?.trim() ?? '',
  );
}

/** Etykiety przełączników z wyrenderowanej powłoki, w kolejności wystąpienia. */
function shellNavLabels(markup: string): readonly string[] {
  return [
    ...markup.matchAll(/<button[^>]*data-section-switch="[^"]*"[^>]*>([^<]*)<\/button>/g),
  ].map((hit) => hit[1]?.trim() ?? '');
}

const html = fileText(MOCKUP);
const markup = renderToStaticMarkup(<App section="run" screens={{}} />);

describe('the shell layout agrees with the mockup, and the mockup is the oracle', () => {
  it('declares two columns, and the first one is the width the mockup says', () => {
    const wanted = property(ruleBody(html, '.app'), 'grid-template-columns');

    expect(
      wanted,
      'nothing was read out of the `.app` rule in docs/mockup/index.html, so the comparison ' +
        'below would run between two empty strings and pass on nothing. Either the file moved ' +
        'or the rule stopped declaring grid-template-columns.',
    ).not.toBe('');
    expect(
      wanted.split(' ').length,
      'the mockup has to declare TWO columns for the side nav to stand beside the content ' +
        'rather than above it. It declares: ' +
        wanted,
    ).toBe(2);

    const rendered = /style="([^"]*grid-template-columns[^"]*)"/.exec(markup)?.[1] ?? '';
    expect(
      rendered,
      'the rendered shell declares no grid-template-columns at all, so nothing says the nav ' +
        'stands beside the content. Markup starts: ' +
        markup.slice(0, 200),
    ).not.toBe('');

    expect(
      tight(rendered.replace(/^grid-template-columns:/, '')),
      'the shell and the mockup disagree about the shell grid. The mockup `.app` rule is the ' +
        'oracle and it says `' +
        wanted +
        '`. Reading it here, in this run, is the whole point: ' +
        'an assertion that spelled the number out would also pass when the mockup changes and ' +
        'the shell does not.',
    ).toBe(wanted);
  });

  it('puts the nav BEFORE the screen region, and both under one container', () => {
    const navAt = markup.indexOf('<nav');
    const mainAt = markup.indexOf('<main');

    expect(navAt, 'the shell renders no <nav> at all').toBeGreaterThanOrEqual(0);
    expect(mainAt, 'the shell renders no <main> screen region at all').toBeGreaterThanOrEqual(0);
    expect(
      navAt,
      'the nav has to come before the screen region in the markup, because in a two-column ' +
        'grid the first child takes the first column. Rendered the other way round it lands in ' +
        'the content column and the screen goes under the 196 px one.',
    ).toBeLessThan(mainAt);

    /* Rodzeństwo, nie zagnieżdżenie: między zamknięciem nawigacji i otwarciem ekranu nie ma
     * prawa stać nic. Cokolwiek tam stanie, stoi NAD treścią i zjada budżet chrome z §7 —
     * czego pilnuje AC-2, ale zobaczy to dopiero wtedy, gdy tu jest rodzeństwo. */
    const between = markup.slice(markup.indexOf('</nav>') + '</nav>'.length, mainAt);
    expect(
      between.trim(),
      'between </nav> and <main> the shell renders something else: ' +
        JSON.stringify(between) +
        '. Those two have to be siblings of one container — anything standing between them ' +
        'stands above the content and spends the chrome budget from ARCHITECTURE §7.',
    ).toBe('');
  });

  it('carries the same switch labels, in the same order as the mockup', () => {
    const wanted = mockupNavLabels(html);

    expect(
      wanted.length,
      'no nav buttons were read out of the mockup, so the comparison below would pass on two ' +
        'empty lists. docs/mockup/index.html has to carry <nav class="nav"> with data-go buttons.',
    ).toBeGreaterThan(0);

    expect(
      shellNavLabels(markup),
      'the shell switches and the mockup switches disagree. The mockup answers "what am I ' +
        'doing" with exactly these words, in this order, and ARCHITECTURE §7 makes that one of ' +
        'the two navigation axes — so a different set here is a different product, not a ' +
        'different style.',
    ).toEqual([...wanted]);
  });
});
