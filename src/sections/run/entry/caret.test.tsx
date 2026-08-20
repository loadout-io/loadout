/* Pole ma kursor od pierwszej sekundy — i dokładnie jedno miejsce w tym wierszu go bierze.
 *
 * Zgłoszenie właściciela 2026-08-20, pierwsza z czterech wad w tym wierszu: „kursor nie stoi
 * w polu, trzeba kliknąć, za każdym razem". Niezmiennik 16 mówi o kontrolce bez handlera, a to
 * jest jej odmiana: pole, w którym nie stoi kursor, obiecuje terminal i go nie dowozi — człowiek
 * płaci jedno kliknięcie za każde wejście na ekran pracy.
 *
 * MARKUP, bo `renderToStaticMarkup` wypisuje `autofocus=""` (zmierzone). Samo OGNISKO w żywej
 * przeglądarce sądzi kryterium z `e2e/tests/terminal-behaves.spec.ts` — render serwerowy nie
 * odpala ani jednego zdarzenia, więc tutaj pytamy o to, co markup NIESIE, i o nic więcej.
 *
 * SŁABA WERSJA: `expect(markup).toContain('autofocus')`. Przechodzi, gdy atrybut wyląduje na
 * dowolnym elemencie — także na podpowiedzi pod polem albo na przycisku. Rozróżnia to pytanie
 * o KONKRETNY element plus policzenie, ile ich w ogóle jest: dwa ogniska to zero ognisk, bo
 * przeglądarka daje kursor jednemu i nie mówi któremu.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { Entry } from './entry';

/** Etykieta pola z makiety — jedyna droga do niego, jaką ma czytnik ekranu i ten plik. */
const FIELD = 'aria-label="Command line"';

/**
 * Wiersz wejścia z propsami, których wymaga jego kształt, i niczym poza tym.
 *
 * Handlery są puste z premedytacją: to kryterium nie pyta, co się dzieje po Enterze, tylko czy
 * pole w ogóle zaprasza do pisania. O tym, że handler żyje, mówią osobne kryteria.
 */
function markup(): string {
  return renderToStaticMarkup(
    <Entry
      onOpenFolder={() => undefined}
      onStopRun={null}
      onSayToAgent={() => Promise.resolve(null)}
      onRunWorkflow={() => Promise.resolve(null)}
    />,
  );
}

/** Otwierające znaczniki tego markupu. React ucieka `<` i `>` w tekście, więc to jest cała lista. */
function tags(html: string): readonly string[] {
  return [...html.matchAll(/<[^>]+>/g)].map((hit) => hit[0]);
}

describe('the command line owns the caret before anybody clicks anything', () => {
  it('renders the field at all, so the two answers below are not statements about nothing', () => {
    const html = markup();
    const fields = tags(html).filter((tag) => tag.includes(FIELD));

    expect(
      fields.length,
      'the entry row has to render exactly one field labelled ' +
        JSON.stringify(FIELD) +
        '. Zero means every assertion in this file is measuring an empty string; two means ' +
        '"the field" names two elements and the caret question has no single answer.',
    ).toBe(1);
  });

  it('puts the caret in that field, not somewhere else in the row', () => {
    const html = markup();
    const field = tags(html).find((tag) => tag.includes(FIELD)) ?? '';

    expect(
      /\bautofocus\b/.test(field),
      'the caret has to start in the command line. A person who opens the work screen to type ' +
        'has to be able to type — clicking the field first is a step nobody asked for, and it is ' +
        'paid on every single visit. The field renders as: ' +
        JSON.stringify(field),
    ).toBe(true);
  });

  it('gives it to exactly one element, because two carets are none', () => {
    const html = markup();
    const focused = tags(html).filter((tag) => /\bautofocus\b/.test(tag));

    expect(
      focused.length,
      'exactly one element in this row may ask for the caret. The browser hands it to one of ' +
        'them and says nothing about which, so two is not "twice as welcoming" — it is a row ' +
        'whose behaviour changes with the order of the markup. Asking elements: ' +
        JSON.stringify(focused),
    ).toBe(1);
  });
});
