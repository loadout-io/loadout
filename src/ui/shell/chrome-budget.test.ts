/* AC-2 dla T-37: nawigacja nie zjada ani jednego piksela budżetu chrome.
 *
 * SŁABA WERSJA, i ona naprawdę stoi w tym repo: `expect(TITLEBAR_HEIGHT).toBeLessThanOrEqual(96)`
 * w `window.test.tsx`. Była ZIELONA przy 138 px realnego chrome, bo mierzyła JEDEN pasek z trzech
 * i porównywała go z liczbą przepisaną z palca. Dwie rzeczy ją odróżniają od tego pliku:
 *
 *   1. sufit jest CZYTANY z `docs/ARCHITECTURE.md` §7, nie wpisany — przepisany rozjechałby się
 *      przy pierwszej zmianie architektury i kłamałby cicho (niezmiennik 18);
 *   2. sumujemy WSZYSTKO, co powłoka stawia nad treścią, a nie jeden wybrany element.
 *
 * CZEGO TEN TEST NIE WIDZI, powiedziane wprost, bo sprawdzenie z nieopisaną granicą jest gorsze
 * niż jego brak: mierzy rodzeństwo kontenera powłoki. Pasek schowany WEWNĄTRZ `<main>` jest dla
 * niego niewidzialny — bo tam zaczyna się już treść sekcji, a jej gęstości pilnuje osobny
 * strażnik (`checks/_quick-density.sh`). Granica jest w rodzeństwie i tylko tam.
 *
 * PUNKT (c) NIE JEST OZDOBĄ. Parser, który cicho zwróci zero, zamienia punkt (a) w porównanie
 * „0 <= 0" i przepuszcza dowolny układ. Dlatego sufit musi być liczbą DODATNIĄ, zanim cokolwiek
 * się z nim porówna.
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby test padał na asercji
 * o treści, nigdy na otwarciu pliku (AGENTS.md §2a p. 5).
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import { CHROME_INSET_TOP } from './titlebar';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const ARCHITECTURE = resolve(ROOT, 'docs/ARCHITECTURE.md');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/**
 * Sufit z wiersza tabeli §7. Szukamy po TREŚCI wiersza, nie po numerze linii: numer przesuwa
 * się przy każdej edycji dokumentu, a zdanie „Piksele chrome nad pierwszą treścią" jest tym,
 * co ten wiersz znaczy.
 */
function chromeCeiling(md: string): number {
  const row = /\|\s*Piksele chrome nad pierwsz[^|]*\|([^|]*)\|/.exec(md);
  const digits = /(\d+)/.exec(row?.[1] ?? '');
  return digits === null ? 0 : Number(digits[1]);
}

/** Znacznik otwierający kontener powłoki: pierwszy element z deklaracją kolumn siatki. */
function containerTag(markup: string): string {
  return /<div[^>]*grid-template-columns[^>]*>/.exec(markup)?.[0] ?? '';
}

/** Ile kolumn deklaruje kontener. Zero znaczy „nie znalazłem deklaracji". */
function columnCount(tag: string): number {
  const declared = /grid-template-columns:([^;"]*)/.exec(tag)?.[1] ?? '';
  return declared.trim() === '' ? 0 : declared.trim().split(/\s+/).length;
}

/** Nazwy znaczników bezpośrednich dzieci kontenera, w kolejności renderu. */
function childTags(markup: string): readonly string[] {
  const opening = containerTag(markup);
  if (opening === '') return [];
  const inside = markup.slice(markup.indexOf(opening) + opening.length);
  const out: string[] = [];
  let depth = 0;
  for (const hit of inside.matchAll(/<(\/?)([a-z]+)[^>]*?(\/?)>/g)) {
    const closing = hit[1] === '/';
    const selfClosing = hit[3] === '/';
    if (!closing && depth === 0) out.push(hit[0]);
    if (closing) {
      if (depth === 0) break;
      depth -= 1;
    } else if (!selfClosing) {
      depth += 1;
    }
  }
  return out;
}

/**
 * Własny górny odstęp kontenera powłoki. TRZECI SKŁADNIK, dopisany 2026-08-19 (T-46).
 *
 * Do tej pory ta suma brała wyłącznie RODZEŃSTWO stojące nad `<main>`. To było zupełne dopóki
 * kontener nie miał własnego odstępu — a odkąd kartki PŁYWAJĄ, ma go: osiem pikseli nad kartką
 * treści jest chrome dokładnie tak samo jak pasek, tylko nie jest niczyim rodzeństwem i nie ma
 * `height`, więc dla poprzedniej wersji było niewidzialne.
 *
 * Czego ta strona pomiaru wciąż NIE widzi, powiedziane wprost, bo pomiar z nieopisaną granicą
 * jest gorszy niż jego brak. Po pierwsze: kart workspace ani paska loadoutu — one stoją WEWNĄTRZ
 * `<main>`, a granica tego pomiaru jest w rodzeństwie kontenera i tylko tam (patrz akapit na
 * początku pliku). Dziś rodzeństwa nad treścią nie ma wcale, więc ta suma to w praktyce sam
 * odstęp kontenera. Po drugie: obrysu kartki treści. Jest on zadeklarowany klasą (`paper`),
 * a nie liczbą w markupie, więc z renderu nie da się go odczytać. Wszystkie cztery składniki
 * liczy strona MAKIETY —
 * `floating-pane-fits-the-ceiling.test.ts` — która czyta obie wartości z reguł CSS i osobnym
 * punktem wymaga, żeby powłoka deklarowała ten sam odstęp co makieta. Dwie strony jednego faktu,
 * każda mierząca to, co naprawdę widzi.
 */
function declaredInsetTop(tag: string): number {
  const padding = /(?:^|[;"])\s*padding:\s*(\d+)px/.exec(tag);
  if (padding !== null) return Number(padding[1]);
  const top = /(?:^|[;"])\s*padding-top:\s*(\d+)px/.exec(tag);
  return top === null ? 0 : Number(top[1]);
}

/** Zadeklarowana wysokość elementu, albo `null`, gdy element jej nie podaje. */
function declaredHeight(tag: string): number | null {
  const found = /(?:^|[;"])\s*(?:min-)?height:\s*(\d+)px/.exec(tag);
  return found === null ? null : Number(found[1]);
}

const md = fileText(ARCHITECTURE);
const markup = renderToStaticMarkup(createElement(App, { section: 'run', screens: {} }));
const children = childTags(markup);
const contentAt = children.findIndex((tag) => tag.startsWith('<main'));
const navAt = children.findIndex((tag) => tag.startsWith('<nav'));
/** Rodzeństwo stojące NAD treścią: wszystko przed `<main>`, poza samą nawigacją. */
const above = children.filter((_tag, index) => index < contentAt && index !== navAt);

describe('the nav spends none of the chrome budget', () => {
  it('reads a positive ceiling out of ARCHITECTURE §7', () => {
    expect(
      md,
      'docs/ARCHITECTURE.md could not be read, so every number below would come from nowhere.',
    ).not.toBe('');
    expect(
      chromeCeiling(md),
      'the ceiling row could not be parsed out of §7. Without it the budget check compares ' +
        'against zero and passes on any layout at all — which is exactly the shape of check ' +
        'this criterion exists to replace. §7 has to carry a row saying "Piksele chrome nad ' +
        'pierwszą treścią" with a number in it.',
    ).toBeGreaterThan(0);
  });

  it('puts nothing above the content that the budget cannot pay for', () => {
    const ceiling = chromeCeiling(md);

    expect(contentAt, 'the shell renders no <main> screen region').toBeGreaterThanOrEqual(0);

    const unmeasurable = above.filter((tag) => declaredHeight(tag) === null);
    expect(
      unmeasurable,
      'something stands above the content and declares no height, so the budget cannot be ' +
        'MEASURED — and §7 says the ceiling is measured, never eyeballed (niezmiennik 18). ' +
        'Give it a declared height or take it out of the chrome.',
    ).toEqual([]);

    const spent =
      above.reduce((total, tag) => total + (declaredHeight(tag) ?? 0), 0) +
      declaredInsetTop(containerTag(markup));
    expect(
      spent,
      'the shell spends ' +
        String(spent) +
        ' px above the first content — siblings standing above <main> plus the container inset ' +
        'of ' +
        String(declaredInsetTop(containerTag(markup))) +
        ' px — and §7 allows ' +
        String(ceiling) +
        '. Tabs (34) and the loadout bar (56) already claim 90 of it, so the ' +
        'six that are left are the whole negotiating room. §7 says another bar means removing ' +
        'one, never raising the limit — poprzedni prototyp raised its own to 2,4× and ended at 149 px.',
    ).toBeLessThanOrEqual(ceiling);
  });

  it('contributes zero itself, because it stands beside and not above', () => {
    const tag = containerTag(markup);

    expect(
      columnCount(tag),
      'the shell container declares no two columns, so the nav is not beside anything — it is ' +
        'stacked, and a stacked nav spends its full height out of the 96 px.',
    ).toBeGreaterThanOrEqual(2);
    expect(navAt, 'the shell renders no <nav>').toBeGreaterThanOrEqual(0);
    expect(
      navAt,
      'the nav is not the first child, so it does not take the first column and the screen ' +
        'region is the one standing in the 196 px lane.',
    ).toBe(0);

    /* Ta asercja odróżnia „mierzy właściwą rzecz" od „sumuje każdą liczbę w markupie".
     * Nawigacja ma własny górny odstęp (44 px, żeby marka nie leżała pod światłami macOS)
     * i on NIE JEST chrome nad treścią, bo treść zaczyna się obok, nie pod nim. Sprawdzenie,
     * które sumowałoby wszystko, co ma wysokość, złapałoby te 44 px i kazałoby je zapłacić. */
    expect(
      CHROME_INSET_TOP,
      'the nav has to keep a top inset for the macOS lights; without it there is nothing to ' +
        'tell apart from a bar that really does stand above the content.',
    ).toBeGreaterThan(0);
    expect(
      above,
      'the nav is a column sibling of the content, so its own box — including its ' +
        String(CHROME_INSET_TOP) +
        ' px top inset — spends nothing out of the ceiling. ' +
        'Anything listed here is a real bar above the content: ' +
        JSON.stringify(above),
    ).toEqual([]);
  });
});
