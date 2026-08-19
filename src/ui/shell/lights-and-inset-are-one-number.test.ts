import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { CHROME_INSET_TOP, LIGHTS_GAP, LIGHTS_HEIGHT, PANE_GAP } from './titlebar';

/* AC-2 dla T-46: odstep panelu i pozycja swiatel macOS sa JEDNA liczba, mierzona razem.
 *
 * Swiatla macOS plywaja nad trescia (`titleBarStyle: "Overlay"`, `hiddenTitle: true`), a marka
 * zaczyna sie dopiero pod nimi. Panel plywa teraz o `PANE_GAP` nizej niz okno, wiec jego wlasny
 * gorny odstep MALEJE o dokladnie tyle:
 *
 *   trafficLightPosition.y  +  LIGHTS_HEIGHT  +  LIGHTS_GAP  −  PANE_GAP  =  CHROME_INSET_TOP
 *
 * ZADNA z tych liczb nie jest w tym tescie wpisana. Pierwsza jest czytana z `tauri.conf.json`,
 * ostatnia z makiety, a dwie srodkowe sa nazwanymi stalymi — do dzisiaj byly liczbami
 * w KOMENTARZU, czyli wartosciami, ktorych nie da sie sprawdzic.
 *
 * SLABA WERSJA: `expect(CHROME_INSET_TOP).toBe(36)`. Przechodzi po zmianie samego
 * `trafficLightPosition` na 24, po ktorej marka wchodzi pod swiatla — a osobno kazda z tych
 * dwoch liczb wyglada rozsadnie.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const CONF = resolve(ROOT, 'src-tauri/tauri.conf.json');
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

/** `trafficLightPosition.y` z konfiguracji okna, albo null. */
function lightsY(): number | null {
  const raw = fileText(CONF);
  if (raw === '') return null;
  try {
    const conf = JSON.parse(raw) as {
      app?: { windows?: ReadonlyArray<{ trafficLightPosition?: { y?: number } }> };
    };
    const y = conf.app?.windows?.[0]?.trafficLightPosition?.y;
    return typeof y === 'number' ? y : null;
  } catch {
    return null;
  }
}

describe('odstep panelu i swiatla macOS', () => {
  const html = withoutComments(fileText(MOCKUP));

  it('reads the lights position out of the window setup file', () => {
    expect(
      lightsY(),
      'trafficLightPosition.y could not be read out of src-tauri/tauri.conf.json, so the total ' +
        'below would be built on a number from nowhere',
    ).not.toBeNull();
  });

  it('reads the window inset out of the mockup', () => {
    expect(
      px(property(ruleBody(html, '.app'), 'padding')),
      'no window inset was read out of the mockup .app rule',
    ).not.toBeNull();
  });

  it('names the two middle numbers instead of hiding them in a comment', () => {
    expect(
      LIGHTS_HEIGHT,
      'the height of the macOS lights is not a named value. As a number inside a comment it ' +
        'cannot be checked, and the arithmetic below would be two thirds guesswork.',
    ).toBeGreaterThan(0);
    expect(LIGHTS_GAP, 'the gap under the lights is not a named value').toBeGreaterThan(0);
    expect(PANE_GAP, 'the pane inset is not a named value').toBeGreaterThan(0);
  });

  it('keeps the four numbers as ONE arithmetic, so one cannot move without the rest', () => {
    const y = lightsY() ?? 0;
    const inset = px(property(ruleBody(html, '.app'), 'padding')) ?? 0;
    expect(
      CHROME_INSET_TOP,
      'the top inset of the panel is not the sum it has to be: ' +
        String(y) +
        ' (lights position) + ' +
        String(LIGHTS_HEIGHT) +
        ' (lights height) + ' +
        String(LIGHTS_GAP) +
        ' (gap under them) − ' +
        String(inset) +
        ' (window inset) = ' +
        String(y + LIGHTS_HEIGHT + LIGHTS_GAP - inset) +
        '. Move one of these without the others and the brand sits under the lights, which is ' +
        'unreadable and looks like nothing is wrong.',
    ).toBe(y + LIGHTS_HEIGHT + LIGHTS_GAP - inset);
  });

  it('agrees with the mockup on the inset the panel floats by', () => {
    expect(
      PANE_GAP,
      'the shell floats the panel by a different step than the mockup, so the arithmetic above ' +
        'is right about a layout that is not the one drawn',
    ).toBe(px(property(ruleBody(html, '.app'), 'padding')));
  });
});
