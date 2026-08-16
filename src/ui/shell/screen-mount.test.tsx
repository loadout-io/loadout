/* Kryterium 1 dla T-25: sekcja, która ma ekran, pokazuje TEN ekran, a pozostałe cztery nie są
 * w drzewie.
 *
 * `expect(html).toContain('agents-screen')` przechodzi na powłoce, która trzyma wszystkie pięć
 * ekranów naraz i chowa cztery CSS-em — czyli na „always-mounted route stack", przez który
 * poprzedni prototyp renderował 142 elementy niosące tekst przy suficie 60 [raport 03 §4.1]. Odróżniają
 * je dopiero trzy rzeczy naraz: PEŁNA mapa pięciu ekranów (dopóki cztery pozostałe nie mają
 * czego pokazać, „pozostałe cztery" nic nie znaczy), policzenie ich DO ZERA, i zakaz `hidden`
 * oraz `display:none` — bo dokładnie tymi dwiema rzeczami chowa się cztery zamontowane ekrany
 * tak, żeby licznik dalej się zgadzał.
 *
 * Pięć identyfikatorów jest wypisanych TUTAJ na sztywno, a nie czytane z SECTIONS: pętla po
 * rejestrze sprawdzałaby rejestr sam sobą, a pusta tablica przeszłaby wtedy każde „dla każdej
 * sekcji…". Ta sama pułapka jest opisana w sections.test.tsx z T-01.
 *
 * Ekran jest tu jednym pustym elementem z własnym znacznikiem, nie zdaniem: liczymy wystąpienia,
 * a treść, którą dałoby się pomylić z czymkolwiek innym w dokumencie, tylko by w tym mieszała.
 */
import type { ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import type { ScreenMap } from '../screens';

const EXPECTED = ['run', 'workflows', 'agents', 'skills', 'memory'] as const;

type Id = (typeof EXPECTED)[number];

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/* `hidden` jako SAMODZIELNY token, obojętnie czy jest atrybutem, czy klasą.
 *
 * Wąski wzorzec ` hidden(?:=""|>|\s)` łapał tylko atrybut: `hidden=""`, `hidden>` i `hidden `
 * przed następnym atrybutem. `class="p-4 hidden"` przechodziło, bo zaraz za słowem stoi
 * cudzysłów, a `class="hidden"` przechodziło podwójnie — cudzysłów jest po OBU stronach. A to
 * jest dziś najtańszy sposób schowania czterech zamontowanych ekranów: jedno słowo w klasie,
 * reguła w arkuszu.
 *
 * Stąd granica z obu stron zamiast wyliczanki końcówek: `hidden` ma nie być otoczone znakiem
 * słowa ani myślnikiem, cokolwiek stoi obok. `aria-hidden` i `data-hidden` zostają na zewnątrz
 * — przed słowem stoi tam myślnik, a to już inna nazwa, nie ten atrybut.
 *
 * Bez cudzysłowów w samym wzorcu, i to nie jest kosmetyka: `checks/quick-vocabulary.sh` paruje
 * apostrofy w pliku, żeby wyłuskać literały, a nieparzysty apostrof we wnętrzu wyrażenia
 * przesuwa mu parowanie na resztę pliku i sypie trafieniami w losowych miejscach.
 */
const HIDDEN_TOKEN = /(?<![\w-])hidden(?![\w-])/;

/** Ekran, który da się policzyć: jeden element, jeden znacznik, żadnej treści. */
function screenFor(id: Id): () => ReactElement {
  return () => <p data-screen={id} />;
}

/** Wszystkie pięć sekcji mają ekran — inaczej „pozostałe cztery" nie ma czego wykluczać. */
const ALL: ScreenMap = {
  run: screenFor('run'),
  workflows: screenFor('workflows'),
  agents: screenFor('agents'),
  skills: screenFor('skills'),
  memory: screenFor('memory'),
};

function markupFor(id: Id): string {
  return renderToStaticMarkup(<App section={id} screens={ALL} />);
}

describe('the section that has a screen shows it, and the other four are not in the tree', () => {
  it('shows the screen it was handed for the open section', () => {
    const markup = renderToStaticMarkup(
      <App section="agents" screens={{ agents: () => <p data-probe="agents-screen">…</p> }} />,
    );
    expect(
      occurrences(markup, 'data-probe="agents-screen"'),
      'asking for agents with an agents screen in hand has to put that screen in the tree ' +
        'exactly once. The shell this task replaces shows the empty screen for all five ' +
        'sections and never reaches for a screen at all — green everywhere, five blank ' +
        'rectangles in the window',
    ).toBe(1);
  });

  for (const id of EXPECTED) {
    it('draws the ' + id + ' screen and only that one, with all five available', () => {
      const markup = markupFor(id);
      expect(
        occurrences(markup, 'data-screen="' + id + '"'),
        'with ' + id + ' open, its screen has to be in the tree exactly once',
      ).toBe(1);
      for (const other of EXPECTED) {
        if (other === id) continue;
        expect(
          occurrences(markup, 'data-screen="' + other + '"'),
          'with ' +
            id +
            ' open, the ' +
            other +
            ' screen has to be absent from the tree, not merely invisible. Five kept alive and ' +
            'four hidden is the shape that put 142 text-carrying elements on one poprzedni prototyp screen',
        ).toBe(0);
      }
    });

    it('leaves the other four sections out of the tree, with ' + id + ' open', () => {
      const markup = markupFor(id);
      expect(
        occurrences(markup, 'data-section="' + id + '"'),
        'asking for ' + id + ' has to put exactly one element carrying data-section="' + id + '"',
      ).toBe(1);
      for (const other of EXPECTED) {
        if (other === id) continue;
        expect(
          occurrences(markup, 'data-section="' + other + '"'),
          'with ' + id + ' open, ' + other + ' has to be absent from the tree as well',
        ).toBe(0);
      }
    });

    it('never hides a screen instead of leaving it out, with ' + id + ' open', () => {
      const markup = markupFor(id);
      expect(
        HIDDEN_TOKEN.test(markup),
        'nothing in the shell may carry the hidden attribute or the hidden class: hiding is how ' +
          'four screens stay in the tree while the count above still reads one',
      ).toBe(false);
      expect(
        /display\s*:\s*none/i.test(markup),
        'nothing in the shell may set display:none, for the same reason. Which section is open ' +
          'is decided in TypeScript, not in a style sheet (invariant 15)',
      ).toBe(false);
    });
  }
});
