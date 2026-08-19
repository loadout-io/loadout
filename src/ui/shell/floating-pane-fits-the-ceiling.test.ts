import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { PANE_GAP } from './titlebar';

/* AC-1 dla T-46: chrome nad pierwsza trescia mieszczy sie w suficie — LICZAC odstep okna
 * i obrys kartki tresci.
 *
 * DLACZEGO NOWY PLIK, A NIE ROZSZERZENIE `chrome-budget.test.ts`. Tamten mierzy dobrze
 * i mierzy CO INNEGO: strone APLIKACJI i wylacznie RODZENSTWO stojace nad `<main>`. Odstep
 * samego kontenera i obrys kartki tresci nie sa rodzenstwem i nie maja `height`, wiec sa dla
 * niego niewidzialne. Jest tez cytowany przez kryterium T-37, a sciezka testu jest globalnie
 * unikalna (AGENTS.md §2a p. 2). Ten plik mierzy strone MAKIETY i liczy te dwa skladniki.
 *
 * DLACZEGO SUFIT JEST CZYTANY, NIE WPISANY. `docs/STATUS.md` nazywa wzorcowym przykladem wady
 * asercje `TITLEBAR_HEIGHT <= 96`: byla ZIELONA przy 138 px realnego chrome, bo mierzyla jeden
 * pasek z trzech i porownywala go z liczba przepisana z palca. Odrozniaja to dwie rzeczy: sufit
 * pochodzi z `docs/ARCHITECTURE.md` §7, a suma bierze WSZYSTKO, co powloka stawia nad trescia.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const ARCHITECTURE = resolve(ROOT, 'docs/ARCHITECTURE.md');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Komentarz nie jest regula. Parser, ktory ich nie odejmuje, sadzi proze. */
function withoutComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, ' ');
}

function tight(value: string): string {
  return value.replace(/\s+/g, ' ').trim();
}

/** Cialo pierwszej reguly o podanym selektorze. */
function ruleBody(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(escaped + '\\s*\\{([^}]*)\\}').exec(css)?.[1] ?? '';
}

/** Wartosc jednej wlasciwosci z ciala reguly. */
function property(body: string, name: string): string {
  return tight(new RegExp('(?:^|;)\\s*' + name + '\\s*:([^;]*)').exec(body)?.[1] ?? '');
}

/** Pierwsza liczba pikseli w wartosci, albo null. Skrot `8px` i `8px 0 0` daja to samo. */
function px(value: string): number | null {
  const found = /(-?\d+(?:\.\d+)?)px/.exec(value);
  return found === null ? null : Number(found[1]);
}

/** Sufit z wiersza tabeli §7, szukany po TRESCI wiersza, nie po numerze linii. */
function ceiling(md: string): number {
  const row = /\|\s*Piksele chrome nad pierwsz[^|]*\|([^|]*)\|/.exec(md);
  const digits = /(\d+)/.exec(row?.[1] ?? '');
  return digits === null ? 0 : Number(digits[1]);
}

describe('chrome nad pierwsza trescia', () => {
  const md = fileText(ARCHITECTURE);
  const html = withoutComments(fileText(MOCKUP));

  it('reads a positive limit out of ARCHITECTURE §7', () => {
    expect(md, 'docs/ARCHITECTURE.md could not be read at all').not.toBe('');
    expect(
      ceiling(md),
      'the limit row could not be parsed out of §7, so the sum below would be compared against ' +
        'zero and any layout at all would pass — which is the exact shape of check this ' +
        'criterion exists to replace',
    ).toBeGreaterThan(0);
  });

  it('reads all four parts out of the mockup, each on its own assertion', () => {
    const parts: ReadonlyArray<readonly [string, number | null]> = [
      ['window inset (.app padding)', px(property(ruleBody(html, '.app'), 'padding'))],
      ['content card border (.screen border)', px(property(ruleBody(html, '.screen'), 'border'))],
      ['tabs height (.tabs)', px(property(ruleBody(html, '.tabs'), 'height'))],
      ['loadout bar height (.strip)', px(property(ruleBody(html, '.strip'), 'height'))],
    ];
    const unread = parts.filter(([, value]) => value === null).map(([name]) => name);
    expect(
      unread,
      'these parts could not be read out of the mockup. A parser that quietly returned nothing ' +
        'adds zeroes and lets any layout through, so each part gets its own assertion before ' +
        'anything is summed.',
    ).toEqual([]);
  });

  it('spends no more than §7 allows', () => {
    const limit = ceiling(md);
    const inset = px(property(ruleBody(html, '.app'), 'padding')) ?? 0;
    const border = px(property(ruleBody(html, '.screen'), 'border')) ?? 0;
    const tabs = px(property(ruleBody(html, '.tabs'), 'height')) ?? 0;
    const strip = px(property(ruleBody(html, '.strip'), 'height')) ?? 0;
    const spent = inset + border + tabs + strip;
    expect(
      spent,
      'the shell spends ' +
        String(spent) +
        ' px above the first content (' +
        String(inset) +
        ' window inset + ' +
        String(border) +
        ' card border + ' +
        String(tabs) +
        ' tabs + ' +
        String(strip) +
        ' bar) and §7 allows ' +
        String(limit) +
        '. §7 says another bar means removing one, never raising the limit — the previous ' +
        'version raised its own to 2,4 times the target and ended at 149 px of chrome.',
    ).toBeLessThanOrEqual(limit);
  });

  it('makes the window declare the same inset the mockup pays for', () => {
    const inset = px(property(ruleBody(html, '.app'), 'padding'));
    expect(inset, 'no window inset was read out of the mockup .app rule').not.toBeNull();
    expect(
      PANE_GAP,
      'the shell and the mockup disagree about the window inset. The mockup can pay for eight ' +
        'pixels the window does not actually have, and then the measurement above is about a ' +
        'layout nobody renders.',
    ).toBe(inset);
  });
});
