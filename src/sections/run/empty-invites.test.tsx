/* AC-6 dla T-39: świeży ekran Run zaprasza do działania — i lista `EXCUSED` wraca do pustej.
 *
 * ZMIERZONE 2026-08-18 NA WYLADOWANYM TRUNKU, i to jest powód istnienia całego zadania:
 *
 *     main button  ->  ["Start [disabled]"]
 *
 * Jeden przycisk, wyłączony. Ekran, od którego zaczyna się praca w tym produkcie, nie
 * zapraszał do niczego — a `docs/design/DESIGN.md` §6 mówi wprost: „Pusty ekran to zaproszenie
 * do działania, nie komunikat o braku danych."
 *
 * SŁABA WERSJA: policzenie przycisków. Przycisk `disabled` jest widoczny i policzalny, a nie
 * da się go użyć — dlatego to kryterium pyta o kontrolkę CZYNNĄ, czyli taką, której człowiek
 * naprawdę może użyć, i osobno o to, czy któraś z nich jest przyciskiem: ekran, którego jedyną
 * żywą kontrolką jest pole tekstowe, nie da się obsłużyć kliknięciem.
 *
 * DRUGA POŁOWA KRYTERIUM JEST CZYTANA Z PLIKU, nie zakładana. `e2e/tests/no-dead-controls.spec.ts`
 * niósł wpis w `EXCUSED` dla `Start`, którego powód kończył się zdaniem „ten wpis znika, kiedy
 * T-39 dowiezie te bloki". Test czyta ten plik i wymaga, żeby wpis zniknął — a przy okazji,
 * żeby sam MECHANIZM wyjątków został: skasowanie całej listy razem z interfejsem przechodziłoby
 * „nie ma wpisu dla Start" i kasowało jedyne miejsce, w którym takie wyjątki są widoczne.
 *
 * ZAPROSZENIE MUSI WSKAZYWAĆ NA COŚ, CO ISTNIEJE. Zdanie „Type /plan to start" nad ekranem bez
 * `/plan` jest gorsze niż brak zdania (niezmiennik 16), więc każde słowo z ukośnikiem z zachęty
 * jedzie tu przez `understand()` z wiersza wejścia — a samo pole musi być czynne.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { understand } from './entry/entry';
import Run from './index';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const SPEC = resolve(ROOT, 'e2e/tests/no-dead-controls.spec.ts');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Świeży ekran: magazyny są puste, bo nikt nic do nich nie włożył w tym pliku. */
const markup = renderToStaticMarkup(<Run />);

/** Znaczniki otwierające wszystkich kontrolek ekranu. */
function controls(kind: string): readonly string[] {
  return [...markup.matchAll(new RegExp('<' + kind + '\\b[^>]*>', 'g'))].map((hit) => hit[0]);
}

/** Kontrolka czynna to taka, której człowiek może użyć — wyłączona jest widoczna i bezużyteczna. */
function live(tags: readonly string[]): readonly string[] {
  return tags.filter((tag) => !/\sdisabled\b/.test(tag));
}

const buttons = controls('button');
const fields = [...controls('input'), ...controls('select'), ...controls('textarea')];

/**
 * Zachęta z WIERSZA WEJŚCIA — zdanie, które czyta się na pustym ekranie.
 *
 * 2026-08-23 — SZUKANE W WIERSZU WEJŚCIA, nie „pierwszym polem w dokumencie". Ta druga wersja
 * była skrótem prawdziwym dokładnie tak długo, jak długo wiersz wejścia był jedynym polem
 * z podpowiedzią. Kiedy grupa startowa dostała pole „co ten bieg ma zbudować", skrót zaczął
 * czytać CUDZE zdanie i żądać od niego nazwy komendy — czyli sądzić rzecz, o której to
 * kryterium nigdy nie mówiło. Komentarz nad tą stałą od początku mówił „z wiersza wejścia";
 * teraz mówi to także kod.
 *
 * Kotwicą jest `data-entry`, czyli ten sam znacznik, po którym wiersz wejścia rozpoznaje reszta
 * kryteriów tego ekranu.
 */
const entryRow = /<form[^>]*\bdata-entry\b[\s\S]*?<\/form>/.exec(markup)?.[0] ?? '';
const invitation = /<input[^>]*placeholder="([^"]*)"/.exec(entryRow)?.[1] ?? '';

describe('an empty run screen invites, and the EXCUSED list is back to empty', () => {
  it('carries at least one control a person can actually use', () => {
    expect(
      buttons.length + fields.length,
      'the run screen renders no controls at all, so "one of them is usable" would be a ' +
        'statement about an empty set.',
    ).toBeGreaterThan(0);

    expect(
      live([...buttons, ...fields]).length,
      'every control on a fresh run screen is disabled, which is exactly the state measured on ' +
        '2026-08-18: one button, `Start`, and it refused. A screen that refuses everything is ' +
        'not an invitation to act — it is a notice that there is no data, and DESIGN §6 is ' +
        'blunt about which of the two an empty screen has to be. Buttons found: ' +
        JSON.stringify(buttons.map((tag) => tag.slice(0, 60))),
    ).toBeGreaterThan(0);

    expect(
      live(buttons).length,
      'not one BUTTON on the fresh screen is usable. A screen whose only live control is a ' +
        'text field cannot be operated by clicking, and clicking is what a person does first.',
    ).toBeGreaterThan(0);
  });

  it('no longer excuses Start in e2e/tests/no-dead-controls.spec.ts', () => {
    const spec = fileText(SPEC);
    expect(
      spec,
      'e2e/tests/no-dead-controls.spec.ts could not be read, so every assertion below would ' +
        'be true of an empty string.',
    ).not.toBe('');

    const opens = spec.indexOf('const EXCUSED');
    const closes = spec.indexOf('];', opens);
    expect(
      opens,
      'the spec has to declare EXCUSED; nothing else in it names the exceptions',
    ).toBeGreaterThanOrEqual(0);
    expect(closes, 'the EXCUSED declaration in the spec is never closed').toBeGreaterThan(opens);

    const block = spec.slice(opens, closes);
    expect(
      /\bStart\b/.test(block),
      'the spec still excuses `Start` on the run screen. That entry said in so many words that ' +
        'it disappears when T-39 delivers the workspace tabs, the agents list and the entry ' +
        'row — and the list going back to empty is what the file itself calls the best ' +
        'possible state. It says: ' +
        JSON.stringify(block.slice(0, 200)),
    ).toBe(false);

    /* Mechanizm zostaje, pusty. Skasowanie go w całości przechodziłoby asercję wyżej
     * i zabierało jedyne miejsce, w którym wyjątek od niezmiennika 16 jest widoczny. */
    expect(
      spec.includes('interface Excused') && spec.includes('excuseFor('),
      'the exception mechanism has to stay in the spec, empty. Deleting it passes "no entry ' +
        'for Start" and takes away the one place where an exception to invariant 16 is written ' +
        'down where somebody can see it.',
    ).toBe(true);
  });

  it('greets the empty screen with an invitation that points at something real', () => {
    expect(
      invitation,
      'the fresh screen carries no invitation at all: no field greets the person with ' +
        'something to type. "Nothing here yet" on its own is the notice DESIGN §6 rules out.',
    ).not.toBe('');

    const named = [...invitation.matchAll(/\/[a-z][a-z-]*/g)].map((hit) => hit[0]);
    expect(
      named.length,
      'the invitation names no action to take. An empty screen has to say what to do next, ' +
        'and it says: ' +
        JSON.stringify(invitation),
    ).toBeGreaterThan(0);

    for (const command of named) {
      expect(
        understand(command),
        'the invitation offers ' +
          command +
          ' and nothing on this screen carries it out. An invitation pointing at a control ' +
          'that does not exist is worse than one sentence less (invariant 16).',
      ).toBe(command);
    }

    const field =
      new RegExp(
        '<input[^>]*placeholder="' + invitation.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&') + '"[^>]*>',
      ).exec(markup)?.[0] ?? '';
    expect(field, 'the field carrying the invitation was not found in the markup').not.toBe('');
    expect(
      live([field]).length,
      'the field that carries the invitation is disabled, so the screen asks a person to type ' +
        'into something they cannot type into.',
    ).toBe(1);
  });
});
