/* PRZEŁĄCZENIE ZAKRESU NIE GUBI SESJI — wymóg twardy właściciela z 2026-08-18.
 *
 * SŁABE WERSJE TEJ ASERCJI, i każda z nich przechodzi na kodzie, który wymóg łamie:
 *   1. „`runFor('/a')` po przełączeniu wciąż ma linie" — przechodzi także wtedy, gdy uchwyt
 *      `useRun` nigdy za przełączeniem nie idzie, czyli gdy ekran po przejściu do drugiego
 *      zakresu pokazuje bieg pierwszego. To jest defekt DOKŁADNIE odwrotny do zgubionej sesji
 *      i wygląda na ekranie tak samo źle.
 *   2. „`useRun` po przełączeniu jest pusty" — przechodzi na implementacji, która sesję
 *      TWORZY NA NOWO przy każdym przełączeniu, czyli na tej, która gubi wszystko.
 *   3. Sam odczyt stanu, bez subskrypcji — przechodzi na uchwycie, który oddaje właściwą sesję
 *      z `getState()`, ale nie budzi nikogo w chwili przełączenia. `useSyncExternalStore` czyta
 *      migawkę WYŁĄCZNIE po powiadomieniu, więc taka wersja zostawia na ekranie bieg
 *      z poprzedniego zakresu do najbliższej paczki z drutu — a w zakresie, w którym nic nie
 *      idzie, na zawsze.
 * Rozstrzyga więc dopiero trójka razem: treść sesji przeżywa, uchwyt idzie za zakresem,
 * a subskrybent dowiaduje się o tym w chwili przełączenia.
 *
 * DLACZEGO TO NIE JEST TEST RENDERU. To repo nie ma jsdom, a `renderToStaticMarkup` montuje
 * ekran raz i nigdy nie odmontowuje — przełączenia zakresu w środku życia okna nie da się tam
 * zobaczyć. Sądzimy więc magazyn i uchwyt wprost, tą samą drogą, którą chodzi ekran
 * (`useSyncExternalStore(useRun.subscribe, useRun.getState, …)`).
 *
 * ZAKRESY SIEJEMY PRZEZ `setState`, nie przez `add()`: `add` idzie na dysk przez `save_workspace`,
 * a ten test nie mówi o dysku ani o granicy — mówi o tym, co się dzieje po przełączeniu.
 */
import { describe, expect, it } from 'vitest';

import { runFor, sessionsAlive, useRun } from '../../state/run';
import { useWorkspaces } from '../../state/workspaces';

/** Dwa zakresy: `id === folder` (kontrakt granicy z 2026-08-18). */
const FIRST = { id: '/Users/x/meetnotes', name: 'meetnotes', folder: '/Users/x/meetnotes' };
const SECOND = { id: '/Users/x/ledger-ui', name: 'ledger-ui', folder: '/Users/x/ledger-ui' };

/* Linie sa bez adnotacji `FeedLine` z POWODU: adnotacja rozszerzylaby typ do unii czternastu
 * rodzajow, a `text` nie stoi w kazdym z nich. Wnioskowany typ literalu jest wezszy i dalej
 * przypisywalny do `FeedLine`, wiec `appendLines` przyjmuje go bez rzutowania. */
const IN_FIRST = {
  kind: 'note' as const,
  agent: 'Build',
  text: 'Rewriting the parser.',
  id: 1,
  at: 1_000,
};

/** Linia z drugiego zakresu — inna treść, żeby pomieszanie dwóch zakresów było widoczne. */
const IN_SECOND = {
  kind: 'note' as const,
  agent: 'Review',
  text: 'Reading the invoices.',
  id: 1,
  at: 2_000,
};

function activate(id: string): void {
  useWorkspaces.setState({ all: [FIRST, SECOND], activeId: id });
}

describe('switching workspaces keeps what every scope was doing, and the handle follows the switch', () => {
  it('keeps the lines of the workspace a person switched away from', () => {
    activate(FIRST.id);
    runFor(FIRST.folder).getState().appendLines([IN_FIRST]);

    expect(
      useRun.getState().lines.map((line) => line.agent),
      'the handle has to read the lines of the ACTIVE workspace, otherwise nothing below ' +
        'distinguishes a working switch from a no-op.',
    ).toEqual([IN_FIRST.agent]);

    activate(SECOND.id);
    runFor(SECOND.folder).getState().appendLines([IN_SECOND]);

    expect(
      useRun.getState().lines.map((line) => line.agent),
      'after the switch the handle still shows the first workspace. A person working in one ' +
        'project and reading the lines of another is the failure this requirement is about, ' +
        'and it is the one that looks like the app works.',
    ).toEqual([IN_SECOND.agent]);

    expect(
      runFor(FIRST.folder)
        .getState()
        .lines.map((line) => line.agent),
      'the workspace we switched AWAY from lost its lines. Nothing may empty it: the run there ' +
        'is still going and it has to come back with its history, not with a blank screen ' +
        '(src-tauri/src/workspace.rs, header).',
    ).toEqual([IN_FIRST.agent]);

    activate(FIRST.id);
    expect(
      useRun.getState().lines.map((line) => line.agent),
      'coming back has to show the same lines. A store rebuilt on return passes every assertion ' +
        'about the ACTIVE workspace and loses the whole run.',
    ).toEqual([IN_FIRST.agent]);
  });

  it('gives every workspace its own store, and never the same one twice', () => {
    activate(FIRST.id);
    expect(
      runFor(FIRST.folder),
      'two calls for one workspace have to give the SAME store. A fresh store per call is empty ' +
        'on every read, which reads on screen as a run that keeps restarting.',
    ).toBe(runFor(FIRST.folder));
    expect(
      runFor(FIRST.folder),
      'two different workspaces sharing one store is the single-store version this change ' +
        'replaced: the second project would read the lines of the first.',
    ).not.toBe(runFor(SECOND.folder));

    expect(
      sessionsAlive(),
      'both stores have to be alive at once — that is the whole of the requirement. A registry ' +
        'holding one entry is a registry that drops the other.',
    ).toEqual(expect.arrayContaining([FIRST.folder, SECOND.folder]));
  });

  it('wakes the subscribers the moment the workspace changes, not on the next line', () => {
    activate(FIRST.id);
    let woken = 0;
    /* Ten sam nasłuch, którego używa ekran: `useSyncExternalStore(useRun.subscribe, …)`. */
    const drop = useRun.subscribe(() => {
      woken += 1;
    });

    activate(SECOND.id);
    expect(
      woken,
      'the switch woke nobody. React re-reads only after a notification, so the screen would keep ' +
        'showing the run of the previous scope until the next batch from the wire — and forever ' +
        'in a scope where nothing is running.',
    ).toBeGreaterThan(0);

    const seen: string[] = [];
    runFor(SECOND.folder)
      .getState()
      .appendLines([{ ...IN_SECOND, id: 2, at: 3_000 }]);
    seen.push('after append');
    expect(
      woken,
      'a line arriving in the now-active scope has to reach the same listener: the handle must be ' +
        'SUBSCRIBED to the new store, not merely reading it. ' +
        seen.join(', '),
    ).toBeGreaterThan(1);

    drop();
    const quiet = woken;
    runFor(SECOND.folder)
      .getState()
      .appendLines([{ ...IN_SECOND, id: 3, at: 4_000 }]);
    expect(
      woken,
      'unsubscribing has to detach from the store too, otherwise every mount of the run screen ' +
        'leaves a listener behind and the leak grows with every visit.',
    ).toBe(quiet);
  });
});
