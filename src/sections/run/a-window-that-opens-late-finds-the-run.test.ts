/* Okno, które wstało w trakcie biegu, ma ten bieg odnaleźć.
 *
 * 2026-08-23 — DRUGA POŁOWA ZGŁOSZENIA WŁAŚCICIELA. Pierwsza („`/stop` mówi »Nothing is
 * running.« nad pracującym biegiem") jest naprawiona tam, gdzie mieszka odpowiedź: pyta się
 * Rusta. Została ta: SKĄD okno ma wiedzieć, że coś idzie. Jego pamięć o żywym biegu jest
 * ulotna — przeładowanie strony zeruje magazyny i moduł, a bieg po tamtej stronie pracuje
 * dalej. Człowiek widzi wtedy ekran bez paska i bez Stopu nad czymś, co kosztuje pieniądze.
 *
 * Odpowiedź jest w historii tego zakresu i nie wymaga ani jednej nowej krawędzi: `list_runs`
 * podaje `state`, a bieg ze słowem `running` w swoim katalogu jest tym, który idzie.
 *
 * CZEGO TEN PLIK NIE SĄDZI: efektu w komponencie. To repo nie ma jsdom, więc kryterium
 * wymagające zamontowania ekranu nigdy by nie świeciło. Sądzona jest REGUŁA — dokładnie tak,
 * jak `./addressee.ts`.
 *
 * SŁABA WERSJA: „bieg ze słowem `running` jest znaleziony". Przechodzi ją `rows[0]`, czyli
 * „najnowszy bieg jest tym, który idzie" — a wtedy okno ogłasza jako żywy bieg, który skończył
 * się wczoraj, i stawia nad nim Stop bez roboty. Dlatego niżej stoją także słowa, które
 * biegiem żywym NIE są, i każde z nich jest w tym repo prawdziwym stanem: `interrupted` niesie
 * bieg porzucony przez zamknięte okno, `paused` bieg stojący na pytaniu.
 */
import { describe, expect, it } from 'vitest';

import { theOneThatIsGoing } from './history-command';
import type { PastRunRow } from './io';

function row(folder: string, state: string): PastRunRow {
  return {
    folder,
    when: '2026-08-23 14:56',
    title: `Run ${folder}`,
    state,
    steps: 22,
    costUsd: null,
    said: null,
  };
}

describe('a window that opens while a run is going finds that run', () => {
  it('picks the one that says it is running, not simply the newest', () => {
    const found = theOneThatIsGoing([
      row('20260823-150000__d', 'failed'),
      row('20260823-145648__c', 'running'),
      row('20260823-010034__b', 'interrupted'),
    ]);

    expect(
      found?.folder,
      'the newest row was taken for the live one. The window would then draw Stop over a run ' +
        'that ended, and press it against whatever is really going somewhere else',
    ).toBe('20260823-145648__c');
  });

  it('says there is none when every run is over, however it ended', () => {
    for (const over of ['succeeded', 'failed', 'cancelled', 'interrupted', 'paused', '']) {
      expect(
        theOneThatIsGoing([row('20260823-010034__b', over)]),
        `a run whose state is "${over}" was reported as going. Nobody is carrying it: the ` +
          'window would announce a live run, put Stop over it, and have nothing to stop',
      ).toBeNull();
    }
  });

  it('says there is none when the folder has no runs at all', () => {
    expect(
      theOneThatIsGoing([]),
      'an empty history has no live run in it, and answering otherwise would be a sentence about ' +
        'an empty set',
    ).toBeNull();
  });
});
