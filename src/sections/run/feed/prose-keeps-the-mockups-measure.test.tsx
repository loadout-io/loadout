/* Proza w strumieniu ma miarę wiersza, którą podaje makieta — czytaną z makiety.
 *
 * # Skarga
 *
 * Właściciel, 2026-08-30, ze zrzutu biegu `20260830-191440`: „nie podoba mi się ta ściana tekstu,
 * ciężko się to czyta". Odpowiedź agenta szła przez całą szerokość kolumny strumienia — grubo
 * ponad dwieście znaków w wierszu. Oko gubi początek następnego wiersza, kiedy musi po niego
 * wracać przez pół ekranu, i to jest połowa tego, co na tamtym zrzucie czyta się jak ściana.
 *
 * # Dlaczego to kryterium w ogóle musi istnieć
 *
 * Bo ta wartość **stała w makiecie od początku i okno jej nigdy nie zastosowało**.
 * `docs/mockup/index.html` ma `.ln.note .t{max-width:64ch}`, a `feed/line.tsx` nie miał ani
 * jednej deklaracji szerokości. Rozjazd trwał do 2026-08-30 i nie zapalił ani jednej czerwieni,
 * bo `run-matches-mockup.test.tsx` sądzi z całej makiety **dwie reguły**: siatkę `.work`
 * i siatkę `.feedcol`. Wszystko poza nimi mogło odjechać po cichu i odjechało.
 *
 * # Dlaczego wartość jest CZYTANA, a nie wpisana
 *
 * Słabą wersją tego kryterium jest `expect(markup).toContain('64ch')`. Przechodzi ona w dwóch
 * przypadkach, w których jest źle: gdy `64ch` stoi gdziekolwiek w markupie — także jako
 * szerokość czegoś zupełnie innego — i gdy makieta zmieni się na 72, a okno zostanie na 64.
 * Odróżnia je to, że oczekiwana wartość jest **czytana z makiety w tym samym biegu testu**.
 * Ten sam zabieg i ten sam powód, co w `run-matches-mockup.test.tsx`.
 *
 * # Czego to kryterium pilnuje po drugiej stronie
 *
 * Że miary NIE dostaje wiersz, który prozą nie jest. Komenda, ścieżka i licznik są etykietami
 * czynności; etykieta zawinięta w połowie kolumny czyta się gorzej, nie lepiej, a kryterium
 * sądzące samą obecność miary przepuściłoby wersję, która nakłada ją wszystkim naraz.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { Line } from './line';
import { createFeed } from './model';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..', '..');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');

/** Ciało reguły CSS o podanym selektorze, z pierwszego wystąpienia. */
function ruleBody(css: string, selector: string): string {
  return new RegExp(selector.replace(/\./g, '\\.') + '\\s*\\{([^}]*)\\}').exec(css)?.[1] ?? '';
}

/** Wartość jednej właściwości z ciała reguły, bez odstępów. */
function property(body: string, name: string): string {
  const found = new RegExp('(?:^|;)\\s*' + name + '\\s*:([^;]*)').exec(body);
  return (found?.[1] ?? '').replace(/\s+/g, '').trim();
}

const css = existsSync(MOCKUP) ? readFileSync(MOCKUP, 'utf8') : '';

/** Markup jednego wiersza zbudowanego przez model z prawdziwej linii z drutu. */
function markupOf(built: ReturnType<typeof line.note>): string {
  const feed = createFeed(sealedScroller());
  feed.appendLines([built]);
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

describe('prose in the stream is read at the measure the mockup sets', () => {
  it('finds the measure in the mockup at all, or this criterion judges nothing', () => {
    expect(
      property(ruleBody(css, '.ln.note .t'), 'max-width'),
      'the mockup is the oracle here. If this rule is gone from it, every assertion below is ' +
        'comparing the window against an empty string and would pass for a window with no ' +
        'measure at all — which is exactly the state this criterion was written to end',
    ).toBe('64ch');
  });

  it('renders agent prose at that measure, not at the width of the column', () => {
    const measure = property(ruleBody(css, '.ln.note .t'), 'max-width');
    const html = markupOf(line.note(1, 0, 'Frontend', 'Implementation is complete.'));

    expect(
      html.includes('max-w-[' + measure + ']'),
      'an answer set across the full width of the stream column runs past two hundred ' +
        'characters a line. The eye loses the start of the next line when it has to travel half ' +
        'a screen to find it — and that is half of what reads as a wall. It rendered: ' +
        html.slice(0, 300),
    ).toBe(true);
  });

  it('leaves an action row alone, because a label is not prose', () => {
    const measure = property(ruleBody(css, '.ln.note .t'), 'max-width');
    const html = markupOf(line.ran(2, 0, 'Frontend', 'npx ng lint', true, []));

    expect(
      html.includes('max-w-[' + measure + ']'),
      'a command, a path and a count are labels for an action, not prose to be read. A label ' +
        'wrapped halfway across its column reads worse, not better — and a criterion that only ' +
        'checked the measure was present would pass a version that put it on every row',
    ).toBe(false);
  });
});
