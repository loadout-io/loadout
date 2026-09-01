/* Kryterium 3 dla T-25: mapowanie ścieżek odrzuca to, czego nie zna, i nie wywraca się na tym.
 *
 * `screensFrom` dostaje tu ręcznie zbudowany rekord — dokładnie w tym kształcie, w jakim
 * odkrywanie oddaje swój wynik: klucz to ścieżka pliku, wartość to moduł. Cztery wpisy, każdy
 * za inną awarię:
 *   `../sections/run/index.tsx`         jedyny poprawny — identyfikator z rejestru plus ekran,
 *   `../sections/quantum/index.tsx`     katalog o nazwie, której nie ma w SECTIONS (literówka),
 *   `../sections/run/rail/panel.tsx`    plik głębiej w poddrzewie sekcji, nie jej ekran,
 *   `../sections/agents/index.tsx`      moduł bez eksportu, który dałoby się wyrenderować.
 *
 * `expect(Object.keys(map)).toContain('run')` przechodzi w implementacji, która przepuszcza
 * także `quantum` i moduł bez eksportu — a wtedy pierwszy literówkowy katalog wywraca całe okno
 * zamiast kosztować jedną sekcję. Odróżnia je RÓWNOŚĆ zbioru kluczy z jednoelementowym zbiorem
 * plus pytanie, czy pod `run` naprawdę leży coś wywoływalnego.
 */
import { describe, expect, it } from 'vitest';
import { screensFrom } from './screens';

/* Ekran-atrapa: powłoka ma go wywołać, więc jedyne, co musi umieć, to być funkcją.
 *
 * Deklaracja, nie strzałka — i to nie jest kwestia gustu. `checks/quick-vocabulary.sh` czyta
 * jako tekst widoczny także to, co stoi między `>` a `<`, a to potrafi przeskoczyć kilka linii:
 * `=>` bez klamry po nim złapałoby razem ze sobą ścieżkę `.../rail/panel.tsx` niżej i zgłosiło
 * ją jako żargon w copy. Klamry ciała przerywają ten zakres. */
function screen(): null {
  return null;
}

const RIGHT = '../sections/run/index.tsx';
const UNKNOWN_NAME = '../sections/quantum/index.tsx';
const DEEPER = '../sections/run/rail/panel.tsx';
/* NAZWA MUSI BYĆ SEKCJĄ Z REJESTRU, inaczej ten wpis odpada z DRUGIEGO powodu i przestaje
 * pytać o cokolwiek. Do 2026-08-31 stało tu `skills/`; tego dnia Skills i Memory zeszły się
 * w Knowledge, więc ta ścieżka przestała nazywać sekcję i wpadała do tego samego worka, co
 * `quantum/` wyżej — a wtedy „moduł bez ekranu odpada" jest zdaniem, którego nikt nie sprawdził.
 * `agents/` jest sekcją i ma ekran, więc jedyne, co go tutaj wyklucza, to brak eksportu. */
const NOTHING_TO_SHOW = '../sections/agents/index.tsx';

const MODULES: Record<string, unknown> = {
  [RIGHT]: { default: screen },
  [UNKNOWN_NAME]: { default: screen },
  [DEEPER]: { default: screen },
  [NOTHING_TO_SHOW]: { helper: 'a file with exports, none of them a screen' },
};

function keysOf(modules: Record<string, unknown>): string[] {
  return Object.keys(screensFrom(modules)).sort();
}

describe('the map keeps what it can show and drops the rest without falling over', () => {
  it('keeps the one path that names a section and carries a screen, and nothing else', () => {
    const map = screensFrom(MODULES);
    expect(
      Object.keys(map).sort(),
      'only run may survive here: quantum is not a section this app has, the deeper file is ' +
        'not a section screen, and the last one has nothing to show. Letting any of them ' +
        'through means the first mistyped directory decides what the window renders',
    ).toEqual(['run']);
    expect(
      typeof map.run,
      'the value under run has to be callable — the shell renders it. A truthy value that is ' +
        'not a component reads as success here and takes the window down later',
    ).toBe('function');
  });

  it('stays quiet about a directory nobody has heard of', () => {
    expect(
      () => screensFrom(MODULES),
      'an unknown directory is skipped, never thrown over: throwing while looking for screens ' +
        "costs the whole window for somebody else's file",
    ).not.toThrow();
  });

  it('answers the same whatever order the paths arrive in', () => {
    const backwards: Record<string, unknown> = {};
    for (const key of Object.keys(MODULES).reverse()) {
      backwards[key] = MODULES[key];
    }
    expect(
      keysOf(backwards),
      'the same four paths in the other order have to give the same answer. Order-sensitive ' +
        'means the answer depends on how the file system listed the directories that day',
    ).toEqual(keysOf(MODULES));
  });

  it('drops a file whose only export cannot be rendered', () => {
    expect(
      keysOf({ [RIGHT]: { default: 42 } }),
      'a default export that is not a component is not a screen. Keeping it puts the number ' +
        'straight into the tree, which is the one failure this rule exists to stop',
    ).toEqual([]);
  });
});
