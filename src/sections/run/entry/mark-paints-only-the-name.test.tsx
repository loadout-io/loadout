/* Warstwa maluje tło pod nazwą i nie robi nic poza tym.
 *
 * # Dlaczego to kryterium istnieje obok `highlight.test.ts`
 *
 * Tamto pyta, CO jest nazwą; to pyta, czy człowiek to zobaczy. Między jednym a drugim mieszka
 * klasa wady, dla której to repo powstało (niezmiennik 29): czysta funkcja może oddawać
 * bezbłędne kawałki, a warstwa i tak nie pomalować niczego — bo klasa nie istnieje, bo token
 * nazywa się inaczej, albo bo cały element nie wchodzi do markupu.
 *
 * # Czego to kryterium nie umie i kto to dokańcza
 *
 * To repo nie ma jsdom, więc `renderToStaticMarkup` nie umie NAPISAĆ znaku w polu — ten plik
 * renderuje więc warstwę wprost, gotowymi kawałkami. Ostatnie ogniwo („po wpisaniu w prawdziwe
 * pole widać wash") należy do przeglądarki i stoi w `e2e/`, tam gdzie już dziś pisze się w ten
 * sam wiersz.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { Mark } from './mark';

/** Kawałki w kształcie, w jakim oddaje je `segments`. */
const PIECES = [
  { text: '/run ', known: false },
  { text: 'ship-a-feature', known: true },
  { text: ' build the parser', known: false },
];

const HTML = renderToStaticMarkup(<Mark pieces={PIECES} />);

describe('the layer under the command line', () => {
  it('paints the recognised name with the colour DESIGN already gives that job', () => {
    expect(
      /<span class="[^"]*bg-accent-soft[^"]*">ship-a-feature<\/span>/.test(HTML),
      'the wash sits on the name and uses `--accent-soft`, which DESIGN §3 calls "the ' +
        'background of the selected element" and which already paints these same names on the ' +
        'list two rows below. A fifth semantic colour is forbidden outright (AGENTS.md §4). ' +
        'It rendered: ' +
        HTML,
    ).toBe(true);
  });

  it('paints nothing on the parts that are not the name', () => {
    expect(
      /<span[^>]*>\/run <\/span>/.test(HTML) && !/bg-accent-soft[^>]*>\/run/.test(HTML),
      'the command and the task stay unpainted. A wash under the whole line says nothing, and ' +
        'a wash under the task says the wrong thing',
    ).toBe(true);
  });

  it('carries every character back, so the wash lines up with the word', () => {
    const text = HTML.replace(/<[^>]*>/g, '');
    expect(
      text,
      'the layer sits under a monospace field and lines up character by character. One ' +
        'character lost here moves the wash off the word by exactly that much',
    ).toBe('/run ship-a-feature build the parser');
  });

  it('is invisible to a screen reader and to the mouse', () => {
    expect(
      HTML.includes('aria-hidden'),
      'without it the screen reader says the whole line twice — once from the layer, once from ' +
        'the field',
    ).toBe(true);
    expect(
      HTML.includes('pointer-events-none'),
      'without it a click on the word stops reaching the field underneath, and the caret never ' +
        'lands where the person aimed',
    ).toBe(true);
  });

  it('draws the letters transparent, because the real field draws them', () => {
    expect(
      HTML.includes('text-transparent'),
      'the layer contributes background only. Visible letters here would double every glyph, ' +
        'half a pixel apart',
    ).toBe(true);
  });

  it('renders nothing at all for an empty line', () => {
    expect(
      renderToStaticMarkup(<Mark pieces={[]} />),
      'the density ratchet on visible text can only go down (ARCHITECTURE §7, invariant 18), so ' +
        'the default empty view must not gain one',
    ).toBe('');
  });
});
