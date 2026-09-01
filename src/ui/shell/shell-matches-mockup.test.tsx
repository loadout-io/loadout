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
import { collapseNav } from '../../state/settings';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Spłaszcza odstępy, żeby `196px  minmax(0, 1fr)` i `196px minmax(0,1fr)` były równe. */
function tight(value: string): string {
  return value.replace(/\s+/g, ' ').replace(/,\s+/g, ',').trim();
}

/** Selektor jako literał w wyrażeniu regularnym.
 *
 * DOPISANE 2026-08-31 razem z drugim trybem. Stało tu `\\` + selektor, co działało wyłącznie
 * dla `.app`: w `.app[data-narrow]` nawiasy kwadratowe są KLASĄ ZNAKÓW, więc wzorzec dopasowałby
 * się do `.appd`, `.appa`, `.appt`… i czytał ciało cudzej reguły albo żadnej. Parser, który po
 * cichu nic nie dopasuje, oddaje pusty napis, a porównanie dwóch pustych napisów przechodzi —
 * dokładnie ta zieleń na niczym, przed którą stoją kontrole w nagłówku. */
function asLiteral(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Ciało reguły CSS o podanym selektorze, z pierwszego wystąpienia. */
function ruleBody(css: string, selector: string): string {
  const found = new RegExp(`${asLiteral(selector)}\\s*\\{([^}]*)\\}`).exec(css);
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
  /* `[\s\S]*?` miedzy przyciskiem a etykieta, bo od T-46 przed nia stoi GLIF
   * (`<span class="ico">`). Stara wersja wymagala `<span>` NATYCHMIAST po znaczniku przycisku
   * i po wstawieniu glifu zwracala pusta liste — czyli test melodwal „no nav buttons were read"
   * na kodzie, ktory byl poprawny. Wzorzec `<span>` bez atrybutow trafia w etykiete, nie w glif. */
  return [...nav.matchAll(/<button[^>]*data-go="[^"]*"[\s\S]*?<span>([^<]*)<\/span>/g)].map(
    (hit) => hit[1]?.trim() ?? '',
  );
}

/** Etykiety przełączników z wyrenderowanej powłoki, w kolejności wystąpienia. */
function shellNavLabels(markup: string): readonly string[] {
  /* Etykieta jest OSTATNIM `<span>` przycisku, bo od T-46 przed nia stoi glif. Stara wersja
   * brala tekst stojacy WPROST miedzy znacznikami przycisku i po owinieciu etykiety w `<span>`
   * zwracala piec pustych napisow — czyli test padal na kodzie, ktory byl poprawny, i to samo
   * zdarzylo sie po stronie makiety. Oba parsery pytaja teraz o to samo: „ktore slowo jest
   * etykieta tego przelacznika". */
  return [...markup.matchAll(/<button[^>]*data-section-switch="[^"]*"[\s\S]*?<\/button>/g)].map(
    (hit) => {
      const spans = [...(hit[0] ?? '').matchAll(/<span[^>]*>([^<]*)<\/span>/g)]
        .map((span) => (span[1] ?? '').trim())
        .filter((text) => text !== '');
      return spans[spans.length - 1] ?? '';
    },
  );
}

const html = fileText(MOCKUP);

/** Powłoka w tym trybie. `collapseNav` przestawia okno natychmiast, więc render widzi już nowy. */
function shellIn(collapsed: boolean): string {
  void collapseNav(collapsed);
  return renderToStaticMarkup(<App section="run" screens={{}} />);
}

const markup = shellIn(false);

/**
 * Reguła makiety i selektor, spod którego ją czytamy, dla każdego z dwóch trybów nawigacji.
 *
 * DWA TRYBY OD 2026-08-31, i to jest zaostrzenie kryterium, nie jego rozluźnienie: makieta
 * deklaruje teraz DWIE szerokości pierwszej kolumny, a powłoka ma trafić w OBIE. Wersja z jedną
 * przechodziłaby dla trybu zwiniętego o dowolnej szerokości — także dla takiego, który zostawia
 * 244 px dziury między menu a treścią, bo kartka zwęziła się, a kolumna siatki nie.
 */
const MODES = [
  { collapsed: false, selector: '.app', named: 'expanded' },
  { collapsed: true, selector: '.app[data-narrow]', named: 'collapsed' },
] as const;

describe('the shell layout agrees with the mockup, and the mockup is the oracle', () => {
  for (const mode of MODES) {
    it('declares two columns in the ' + mode.named + ' mode, the width the mockup says', () => {
      const wanted = property(ruleBody(html, mode.selector), 'grid-template-columns');

      expect(
        wanted,
        'nothing was read out of the `' +
          mode.selector +
          '` rule in docs/mockup/index.html, so the comparison below would run between two ' +
          'empty strings and pass on nothing. Either the file moved or the rule stopped ' +
          'declaring grid-template-columns.',
      ).not.toBe('');
      expect(
        wanted.split(' ').length,
        'the mockup has to declare TWO columns for the side nav to stand beside the content ' +
          'rather than above it. `' +
          mode.selector +
          '` declares: ' +
          wanted,
      ).toBe(2);

      /* JEDNA WLASCIWOSC, nie caly atrybut `style` — poprawione w T-46. Stara wersja brala caly
       * atrybut, wiec dzialala wylacznie dopoki niosl on dokladnie jedna deklaracje; od chwili,
       * gdy powloka dodala `padding` i `gap`, porownywala jedna wlasciwosc makiety z trzema
       * naszymi i padala na kodzie, ktory byl poprawny. Obie strony sa teraz czytane tym samym
       * pytaniem: „co ta regula mowi o `grid-template-columns`". */
      const drawn = shellIn(mode.collapsed);
      const style = /style="([^"]*)"/.exec(drawn)?.[1] ?? '';
      const rendered = property(style, 'grid-template-columns');
      expect(
        rendered,
        'the rendered shell declares no grid-template-columns at all, so nothing says the nav ' +
          'stands beside the content. Markup starts: ' +
          drawn.slice(0, 200),
      ).not.toBe('');

      expect(
        tight(rendered.replace(/^grid-template-columns:/, '')),
        'the shell and the mockup disagree about the shell grid in the ' +
          mode.named +
          ' mode. The mockup `' +
          mode.selector +
          '` rule is the oracle and it says `' +
          wanted +
          '`. Reading it here, in this run, is the whole point: ' +
          'an assertion that spelled the number out would also pass when the mockup changes ' +
          'and the shell does not.',
      ).toBe(wanted);

      /* Kartka nawigacji i kolumna siatki to DWIE deklaracje jednej szerokosci, wiec obie sa
           tu sadzone: kartka 64 px w kolumnie 308 px zostawia 244 px dziury, a punkt czytajacy
           samą siatke widzialby uklad zgodny z makieta. */
      const navStyle = /<nav[^>]*style="([^"]*)"/.exec(drawn)?.[1] ?? '';
      expect(
        tight(property(navStyle, 'width')),
        'in the ' +
          mode.named +
          ' mode the nav pane is not as wide as the column it stands in. The grid says `' +
          wanted +
          '` and the pane says `' +
          navStyle +
          '`, so the difference is a gap of dead glass between the navigation and the work.',
      ).toBe(wanted.split(' ')[0] ?? '');
    });
  }

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
