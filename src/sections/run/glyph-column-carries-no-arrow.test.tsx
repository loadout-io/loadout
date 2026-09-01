import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { HistoryRow } from './feed/model';
import type { Kind } from './feed/kinds';
import { kinds } from './feed/kinds';
import { Line } from './feed/line';

/* AC-3 dla T-47: cichy `✓`, czerwony `✕`, i ani jednej strzalki.
 *
 * STRZALKA JEST CYTATEM Z TERMINALA i nic nie mowi: nazwa agenta w tym samym wierszu juz
 * powiedziala, kto to zrobil. Aplikacja jej nie renderuje, ale MAKIETA wciaz ja niesie — a to
 * makieta jest wyrocznia wygladu, wiec dopoki tam stoi, roznica jest jej zdaniem, nie naszym
 * bledem. Dlatego punkt (d) czyta rysunek, nie tylko kod.
 *
 * RZECZ SKONCZONA JEST CICHA. `✓` jest przygaszony, nie zielony: zielony znaczy „dzieje sie
 * teraz", nie „udalo sie". To odroznia ten produkt od kazdego dashboardu, ktory swieci na
 * zielono, kiedy nic sie nie dzieje.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

/**
 * Wiersz historii o PRAWDZIWYM ksztalcie.
 *
 * Poprzednia wersja podawala pola `text`, `meta`, `open` i napisowy `id`, ktorych `HistoryRow`
 * nie ma (deklaruje `label`, `metric`, `expanded`, liczbowy `id`), a rzutowanie `as HistoryRow`
 * kazalo temu sie skompilowac. Wiersz renderowal sie wiec z etykieta `undefined`. Sygnatura bez
 * rzutowania jest cala poprawka: teraz kompilator sadzi kształt, a nie ja.
 */
function row(kind: Kind): HistoryRow {
  return {
    id: 1,
    kind,
    agent: 'Forge',
    label: 'Read 6 files',
    count: 0,
    ids: [1],
    expanded: false,
    metric: '',
    output: kind === 'ran' ? ['parser_handles_crlf ... FAILED'] : [],
    body: [],
    detailId: null,
  };
}

const render = (kind: Kind): string =>
  renderToStaticMarkup(createElement(Line, { row: row(kind), onToggle: () => undefined }));

/**
 * Klasy komorki glifu.
 *
 * `text-center` jest KLASA, nie osobnym atrybutem, wiec wzorzec wymagajacy jej PRZED `class=`
 * nie dopasowal sie nigdy i test padal na poprawnym kodzie. Szukamy atrybutu klasy, ktory ja
 * zawiera.
 */
function glyphClasses(html: string): string {
  return /class="([^\x22]*text-center[^\x22]*)"/.exec(html)?.[1] ?? '';
}

/**
 * SAM ZNAK z komorki glifu, bez znacznikow.
 *
 * Czytanie HTML zamiast tekstu przewrocilo ten test na poprawnym kodzie: znaki `>` domykajace
 * znaczniki trafialy we wzorzec strzalki, wiec „wiersz niesie strzalke" bylo prawda dla kazdego
 * wiersza. Wzorzec sadzacy znak musi dostac znak.
 */
function glyphChar(html: string): string {
  const cell = /<span[^>]*text-center[^>]*>([\s\S]*?)<\/span>/.exec(html)?.[1] ?? '';
  return cell.replace(/<[^>]*>/g, '').trim();
}

describe('kolumna glifow', () => {
  it('marks a finished step QUIETLY', () => {
    const html = render('done');
    expect(glyphChar(html), 'the finished glyph is not the tick the mockup states').toBe('✓');
    expect(
      /muted/.test(glyphClasses(html)),
      'the finished glyph is not muted. A finished thing is quiet: green means "happening now", ' +
        'not "it worked", and that is what separates this screen from every dashboard that ' +
        'glows green while nothing happens.',
    ).toBe(true);
  });

  it('marks a broken step with the broken colour', () => {
    const html = render('problem');
    expect(glyphChar(html), 'the broken glyph is not the cross the mockup states').toBe('✕');
    expect(
      /fail/.test(glyphClasses(html)),
      'the broken glyph does not carry the broken colour, so a failure reads like a note',
    ).toBe(true);
  });

  it('never uses an arrow as a glyph, on ANY kind of row', () => {
    /* POPRAWIONE: pierwotnie ten punkt zadal pustego glifu dla „wiersza czynnosci", a model
     * nie odroznia czynnosci od noty — `marker()` zwraca kropke dla wszystkiego, co nie jest
     * skonczone ani zepsute. Prawdziwa tresc jest wezsza i w pelni sprawdzalna. */
    /* RODZAJE Z REJESTRU, nie z listy wpisanej w test — poprawione po drugiej opinii. Lista
     * pieciu nazw zawierala `said`, ktorego w zamknietym rejestrze `feed/kinds.ts` NIE MA
     * (czyli jeden z pieciu punktow sadzil rodzaj, ktory nie moze istniec), i pomijala `ran`,
     * jedyny rodzaj docierajacy do galezi `✕` przez pole `output`, a nie przez `kind`. */
    const every = Object.keys(kinds()) as readonly Kind[];
    expect(every.length, 'the kind registry is empty').toBeGreaterThan(5);
    const arrows = every
      .map((kind) => [kind, glyphChar(render(kind))] as const)
      .filter(([, char]) => /[→>»]/.test(char))
      .map(([kind, char]) => kind + ' -> ' + char);
    expect(
      arrows,
      'these rows carry an arrow as their glyph. It is a quotation from a terminal and it says ' +
        'nothing the row does not already say: the agent name stands in the same line.',
    ).toEqual([]);
  });

  it('agrees with the mockup, which must not ask for an arrow either', () => {
    const html = text(MOCKUP);
    expect(html.length, 'docs/mockup/index.html could not be read').toBeGreaterThan(100);
    const drawn = [...html.matchAll(/<span class="g">([^<]*)<\/span>/g)].map((hit) =>
      (hit[1] ?? '').trim(),
    );
    expect(
      drawn.length,
      'no glyph cell was read out of the mockup, so this point would pass on an empty list',
    ).toBeGreaterThan(2);
    const arrows = drawn.filter((char) => /[→>»]/.test(char));
    expect(
      arrows,
      'the mockup still draws these arrows in the glyph column: ' +
        JSON.stringify(arrows) +
        '. The mockup is the only oracle for looks, so while it asks for one the app is the thing ' +
        'that looks wrong.',
    ).toEqual([]);

    /* I DRUGA STRONA TEJ SAMEJ ROZBIEZNOSCI, dopisana po drugiej opinii: rysunek nie ma prawa
     * zadac glifu PUSTEGO tam, gdzie aplikacja rysuje kropke. Zamiana strzalki na pustke
     * wymienilaby nazwana rozbieznosc na nienazwana, a wzorzec szukajacy tylko strzalek
     * strukturalnie nie moglby jej zobaczyc. */
    const plain = glyphChar(render('note'));
    const blanks = drawn.filter((char) => char === '' && plain !== '');
    expect(
      blanks.length,
      'the mockup asks for an empty glyph in ' +
        String(blanks.length) +
        ' place(s) while the app draws ' +
        JSON.stringify(plain) +
        '. Trading a named divergence for an unnamed one is worse than leaving the first: nobody ' +
        'is looking for the second.',
    ).toBe(0);
  });

  it('gives what a PERSON said the person colour, not the interactive one', () => {
    /* `told` jest rodzajem, ktoremu `authorityOf` przypisuje autorytet „you" — i to on gatuje
     * przedrostek. `said` niesie wypowiedz AGENTA, wiec sadzenie na nim mierzylo nie ten wiersz. */
    const said = render('told');
    expect(
      /accent/.test(said),
      'the prefix of a human sentence carries the accent. Since 2026-08-19 the accent means ' +
        '"this is interactive"; a sentence a person typed is not a control, and there is a ' +
        'colour whose whole job is "a person did this".',
    ).toBe(false);
    expect(
      /human/.test(said),
      'the prefix of a human sentence carries no person colour at all, so nothing on the row ' +
        'says a person wrote it rather than an agent',
    ).toBe(true);
  });
});
