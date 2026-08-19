import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { EmptyState } from '../ui/primitives/empty-state';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const THEME = resolve(ROOT, 'src', 'styles', 'theme.css');

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

/* Zrodlo bez komentarzy. Komentarz opisujacy historie migracji jest dokumentacja, nie wolaniem,
 * i ma prawo zostac — a wzorzec, ktory go czyta, melduje defekt w kodzie, ktory jest poprawny.
 * `checks/quick-tokens.sh` ma na to `strip_comments` z tego samego powodu. */
const withoutRemarks = (source: string): string =>
  source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/^\s*\/\/.*$/gm, ' ');

/* AC-1 dla T-50: nazwy zastepcze nie zyja i nikt ich nie wola.
 *
 * T-45 wprowadzil palete ADDYTYWNIE: stare nazwy zostaly jako przekierowania
 * (`--radius-sq: var(--radius-sm)`), zeby zadna powierzchnia nie zostala bez ani jednej reguly CSS
 * w trakcie migracji. T-46, T-47 i T-48 przeniosly powloke, ekran Run i cztery sekcje listowe.
 * Nazwa zastepcza, ktora przezyje fale, nie jest juz migracja — jest druga nazwa tej samej rzeczy
 * (niezmiennik 13), i pierwszy czlowiek, ktory ja zobaczy, uzna ja za decyzje.
 *
 * SLABA WERSJA: asercja, ze `theme.css` nie ma tych definicji. Trzydziesci osiem wolan
 * w osiemnastu plikach zostaje wtedy bez ani jednej reguly CSS — awaria, ktora nie rzuca wyjatku
 * i widac ja tylko okiem, na ekranie, ktorego nikt akurat nie otworzyl.
 */

const GONE = [/--radius-sq\b/, /--radius-dot\b/, /--color-[a-z]+-wash\b/];
/* KSZTALT WOLANIA, NIE LISTA PREFIKSOW.
 *
 * Pierwsza wersja znala `var(--radius-sq)` i `var(--radius-dot)`, ale NIE znala
 * `var(--color-*-wash)` — a wlasnie tak wash byl wolany w arkuszu React Flow. Cofniecie tamtej
 * jednej linii wprowadzalo wolanie nazwy, ktorej `theme.css` nie definiuje: prostokat zaznaczenia
 * tracil tlo w calosci, a wszystkie asercje zostawaly zielone. To jest dokladnie ta awaria, ktora
 * to kryterium opisuje.
 *
 * Lista prefiksow tez byla wezsza od uzycia w tym repo, wiec zamiast jej pilnowac, pytamy
 * o KSZTALT: cokolwiek-`-wash` jako klasa i cokolwiek-`-wash` w `var(...)`. */
const CALLS = [
  /\brounded-sq\b/,
  /\brounded-dot\b/,
  /var\(\s*--radius-(?:sq|dot)\s*\)/,
  /var\(\s*--color-[a-z]+-wash\s*\)/,
  /\b[a-z]+(?:-[a-z]+)*-wash\b/,
];

/** Wszystkie zrodla stylu i widoku, bez komentarzy. */
function sources(): readonly (readonly [string, string])[] {
  const out: [string, string][] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name !== 'node_modules' && entry.name !== 'dist') walk(path);
      } else if (/\.(?:tsx?|css)$/.test(entry.name)) {
        out.push([path.slice(ROOT.length + 1), withoutRemarks(readFileSync(path, 'utf8'))]);
      }
    }
  };
  walk(resolve(ROOT, 'src'));
  return out;
}

describe('nazwy zastepcze', () => {
  const files = sources();

  it('read the whole tree', () => {
    expect(
      files.length,
      'fewer than eighty style and view files were read, so the sweep below is over a fragment',
    ).toBeGreaterThan(79);
    const names = files.flatMap(([, source]) => [...source.matchAll(/\brounded-[a-z]+\b/g)]);
    /* Prog jest niski z rozmyslu i to jest kontrola przeciw MARTWEMU CZYTNIKOWI, nie pomiar
     * pokrycia: czytnik, ktory sie zepsuje, zwraca zero, a nie „o jedenascie mniej". Pierwsza
     * wersja miala tu sto — liczbe zmierzona PRZED migracja, kiedy same nazwy zastepcze dawaly
     * trzydziesci dwa wystapienia — wiec padala na poprawnie posprzatanym drzewie. */
    expect(
      names.length,
      'almost no corner names were read at all, so every assertion below would pass on an empty ' +
        'list',
    ).toBeGreaterThan(39);
  });

  it('defines not one of them any more', () => {
    const sheet = withoutRemarks(text(THEME));
    expect(sheet.length, 'the house sheet could not be read').toBeGreaterThan(1000);
    const alive = GONE.filter((one) => new RegExp(one.source + '\\s*:').test(sheet));
    expect(
      alive.map((one) => one.source),
      'these stand-in names are still declared in the house sheet. A redirection that outlives its ' +
        'migration is a second name for the same thing, and the next person to read it will take ' +
        'it for a decision.',
    ).toEqual([]);
  });

  /* I JEDEN DOWOD NA WYRENDEROWANYM ELEMENCIE, nie tylko na tekscie zrodel.
   *
   * `EmptyState` jest tu wybrany nieprzypadkowo: to prymityw, ktory wolaja wszystkie sekcje, i on
   * niosl dwie nazwy zastepcze — przycisk i ramke znaku. Skan po zrodlach powie, ze slowa nie ma;
   * to pytanie brzmi, czy element, ktory czlowiek naprawde zobaczy, niesie promien z pasma. */
  it('renders the shared empty place with a corner from the band', () => {
    const markup = renderToStaticMarkup(
      createElement(EmptyState, {
        children: 'Nothing here yet.',
        hint: 'It will show up as it happens.',
        action: { label: 'Create', onClick: () => undefined },
      }),
    );
    expect(markup.length, 'the shared empty place rendered nothing').toBeGreaterThan(80);
    for (const call of CALLS) {
      expect(
        new RegExp(call.source).test(markup),
        'the rendered empty place still carries ' +
          call.source +
          ', so the migration stopped at ' +
          'the source text',
      ).toBe(false);
    }
    /* DWA KSZTALTY, KAZDY OSOBNO. Warunek „gdzies w tym komponencie jest promien z pasma"
     * przechodzi dzieki ramce znaku takze wtedy, gdy przycisk swoj STRACIL — zmierzone kontrola
     * negatywna 2026-08-19. Skasowanie nazwy zastepczej bez wpisania prawdziwej daje element
     * kwadratowy, a to nie rzuca wyjatku i nie pojawia sie w zadnym logu. */
    const shaped = [...markup.matchAll(/<[a-z]+[^>]*\sclass="([^"]*)"[^>]*>/g)]
      .map((hit) => hit[1] ?? '')
      .filter((one) => /\brounded-(?:sm|md|lg|pill)\b/.test(one));
    expect(
      shaped.length,
      'the rendered empty place carries fewer than two elements with a corner from the band, and ' +
        'it has two shapes: the badge round the glyph and the one button',
    ).toBeGreaterThan(1);
    const button = /<button[^>]*\sclass="([^"]*)"[^>]*>/.exec(markup)?.[1] ?? '';
    expect(button, 'no button was rendered by the shared empty place').not.toBe('');
    expect(
      /\brounded-(?:sm|md|lg|pill)\b/.test(button),
      'the one button on the shared empty place carries no corner from the band: ' +
        JSON.stringify(button),
    ).toBe(true);
  });

  it('calls not one of them, anywhere under src', () => {
    const left: string[] = [];
    for (const [path, source] of files) {
      for (const call of CALLS) {
        for (const hit of source.matchAll(new RegExp(call.source, 'g'))) {
          left.push(path + ': ' + hit[0]);
        }
      }
    }
    expect(
      left,
      'these places still call a stand-in name: ' +
        JSON.stringify(left) +
        '. With the declaration gone, every one of them is a surface with no rule at all — which ' +
        'throws nothing and shows up only on a screen somebody happens to open.',
    ).toEqual([]);
  });
});
