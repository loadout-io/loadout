/* Każdy terminal ma własną historię — także wtedy, kiedy dwa stoją w JEDNYM folderze.
 *
 * CICHA PORAŻKA, PRZED KTÓRĄ STOI TO KRYTERIUM: terminal, który wygląda na osobny i dzieli
 * strumień. Dwie karty pokazujące tę samą historię są gorsze niż jedna, bo człowiek wpisuje
 * zdanie w jedną i widzi je w obu — i przestaje wierzyć, że cokolwiek na tym ekranie należy
 * do czegokolwiek.
 *
 * DLACZEGO TO NIE JEST TO SAMO, CO `session-per-workspace.test.ts`. Tamten plik pyta o dwa
 * ZAKRESY, czyli o dwa różne foldery, i przechodzi dla rejestru kluczowanego folderem — bo
 * folder jest tam kluczem naturalnym. Tutaj oba terminale mają ten sam folder, więc każdy
 * rejestr kluczowany folderem oddaje im JEDEN model widoku i obie karty pokazują tę samą
 * historię. To jest dokładnie ta różnica, dla której terminal potrzebuje własnej tożsamości.
 *
 * SŁABA WERSJA: test na samym „dwa terminale, dwie historie". Przechodzi dla implementacji,
 * która gubi historię przy przełączeniu — czyli dla wady, którą właściciel zgłosił dwa dni
 * wcześniej („jak się przełączam między workspace to nie tracę sesji"). Rozstrzyga to drugi
 * przypadek: powrót na kartę oddaje jej historię w całości.
 *
 * KTÓRĄ HISTORIĘ WIDAĆ, PYTAMY UCHWYTEM `runFeed`, a nie rejestrem. Rejestr odpowiada na pytanie
 * „gdzie te wiersze leżą", a to jest pytanie, na które dzisiejsza implementacja też umie
 * odpowiedzieć dobrze — kluczem może być dowolny napis. Pytanie tego pliku brzmi „co widzi
 * człowiek, kiedy przełączy kartę", a odpowiada na nie wyłącznie uchwyt, z którego czyta ekran
 * (`src/sections/run/index.tsx`, `currentView`).
 *
 * `../io` PODSTAWIONE, bo magazyn kart bierze stamtąd zatrzymanie biegu, a prawdziwe zatrzymanie
 * woła Rusta przez granicę, której w vitest nie ma. Ten plik nie zatrzymuje żadnego biegu —
 * zamyka wyłącznie kartę, w której nikt nie pracuje.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { FeedLine } from '../../../state/run';
import type { Workspace } from '../../../state/workspaces';
import { useWorkspaces } from '../../../state/workspaces';
import { feedFor, runFeed } from './live';

/* `closeTerminal` dołożone do atrapy 2026-08-20 razem z naprawą po sprawdzającym: magazyn kart
 * bierze stąd DWA kanały — zatrzymanie biegu i koniec rozmowy z liderem zamykanej karty. Ani
 * jedna asercja niżej się o to nie pyta i żadnej nie ubyło; bez tego wiersza vitest przewraca
 * się na dostępie do brakującego eksportu atrapy, czyli przed pierwszą asercją. */
vi.mock('../io', () => ({
  stop: vi.fn(() => Promise.resolve()),
  closeTerminal: vi.fn(() => Promise.resolve()),
}));

const { runTabs } = await import('../tabs/store');

/** Jeden zakres na cały plik: pytanie brzmi „dwa terminale w JEDNYM folderze". */
const LEDGER: Workspace = { id: '/w/ledger-ui', name: 'ledger-ui', folder: '/w/ledger-ui' };

function note(id: number, text: string): FeedLine {
  return { kind: 'note', agent: 'Lead', text, id, at: id * 1_000 };
}

/** Etykiety wierszy historii — to, co człowiek na ekranie naprawdę przeczyta. */
function historyOf(view: { history: readonly { label: string }[] }): readonly string[] {
  return view.history.map((row) => row.label);
}

/**
 * Dwa terminale w jednym folderze i kontrola, że fikstura jest tym, czym mówi.
 *
 * Kontrola stoi tutaj, bo bez niej cały plik mógłby mierzyć dwa FOLDERY — a to jest to, co
 * `session-per-workspace.test.ts` już dowiódł, i co przechodzi dla dzisiejszego rejestru.
 */
function twoTerminalsInOneFolder(a: string, b: string): void {
  useWorkspaces.setState({ all: [LEDGER], activeId: LEDGER.id });
  runTabs.setState({ tabs: [], activeId: null, pendingClose: null });
  runTabs.getState().open({ id: a, name: 'ledger-ui', path: LEDGER.folder, agents: 0 });
  runTabs.getState().open({ id: b, name: 'ledger-ui', path: LEDGER.folder, agents: 0 });

  const cards = runTabs.getState().tabs;
  expect(
    cards.map((card) => card.id),
    'the fixture has to stand up TWO terminals, or everything below is a statement about one ' +
      'card measured twice',
  ).toEqual([a, b]);
  expect(
    new Set(cards.map((card) => card.path)).size,
    'and both of them have to stand in ONE folder. Two folders would be asking what the ' +
      'sibling file about switching projects already answered, and that one passes today.',
  ).toBe(1);
}

/** Przełącza kartę na wierzchu — dokładnie tak, jak robi to kliknięcie w kartę na pasku. */
function lookAt(terminal: string): void {
  runTabs.getState().activate(terminal);
}

const FIRST_SAID = 'Rewrote the field splitter as a three-state machine.';
const SECOND_SAID = 'Renamed the quote column.';

beforeEach(() => {
  runTabs.setState({ tabs: [], activeId: null, pendingClose: null });
});

describe('two terminals in one folder keep two histories', () => {
  it('shows each terminal its own lines, and never the other one lines', () => {
    twoTerminalsInOneFolder('one-a', 'one-b');

    feedFor('one-a').appendLines([note(1, FIRST_SAID)]);
    feedFor('one-b').appendLines([note(2, SECOND_SAID)]);

    /* KONTROLA PRZECIW PUSTEMU PORÓWNANIU: obie historie muszą coś przyjąć. Porównanie dwóch
     * pustych list przechodzi na niczym — i przechodzi dla implementacji, która nie wpuszcza
     * nigdzie niczego. */
    expect(
      historyOf(feedFor('one-a').view).length,
      'the first terminal took no line at all, so every comparison below would be about two ' +
        'empty histories',
    ).toBe(1);
    expect(historyOf(feedFor('one-b').view).length, 'and so did the second').toBe(1);

    lookAt('one-a');
    expect(
      historyOf(runFeed.view),
      'the screen is not showing the terminal a person is looking at. The handle the screen ' +
        'reads still answers by folder, so both cards in this project draw the same history — ' +
        'a person types a line into one card and sees it in both.',
    ).toEqual([FIRST_SAID]);

    lookAt('one-b');
    expect(
      historyOf(runFeed.view),
      'switching to the second terminal kept the first one history on screen. Two terminals ' +
        'that look separate and share their lines are worse than one card, because nothing on ' +
        'this screen belongs to anything any more.',
    ).toEqual([SECOND_SAID]);
  });

  it('gives the whole history back when a person comes back to the first terminal', () => {
    twoTerminalsInOneFolder('two-a', 'two-b');

    feedFor('two-a').appendLines([note(3, FIRST_SAID), note(4, 'Split the third column.')]);
    feedFor('two-b').appendLines([note(5, SECOND_SAID)]);
    const kept = feedFor('two-a');

    lookAt('two-b');
    lookAt('two-a');

    expect(
      historyOf(runFeed.view),
      'coming back to the first terminal lost its history. Switching a card is only a change ' +
        'of view: nothing pauses, nothing detaches and nothing dies — the owner asked for ' +
        'exactly this on 2026-08-18.',
    ).toEqual([FIRST_SAID, 'Split the third column.']);
    expect(
      feedFor('two-a'),
      'the history came back rebuilt, not kept: the registry handed out a second model for the ' +
        'same terminal. That looks identical until a line arrives in the one nobody is holding.',
    ).toBe(kept);
  });

  it('keeps the lines of a terminal that a person closed', () => {
    twoTerminalsInOneFolder('three-a', 'three-b');

    feedFor('three-a').appendLines([note(6, FIRST_SAID)]);
    feedFor('three-b').appendLines([note(7, SECOND_SAID)]);

    /* Karta bez pracujących agentów zamyka się od razu — ten przypadek nie zatrzymuje biegu. */
    runTabs.getState().requestClose('three-b');
    expect(
      runTabs.getState().tabs.map((card) => card.id),
      'the card had to come off the bar first, or the assertion below says nothing about what ' +
        'closing a terminal does to its history',
    ).toEqual(['three-a']);

    expect(
      historyOf(feedFor('three-b').view),
      'closing a terminal threw its history away. The registry has no removal on purpose: what ' +
        'the lead agent said stays readable, and a model dropped on the way out looks identical ' +
        'right up to the moment somebody asks for it again.',
    ).toEqual([SECOND_SAID]);
  });

  it('wakes the screen on the switch itself, not on the next line', () => {
    twoTerminalsInOneFolder('four-a', 'four-b');
    lookAt('four-a');

    let woken = 0;
    const drop = runFeed.subscribe(() => {
      woken += 1;
    });

    lookAt('four-b');
    expect(
      woken,
      'nobody told the screen that the card on top changed, so React keeps the previous ' +
        'history on screen until a line happens to arrive — and until then the window shows ' +
        'one terminal work under another terminal name.',
    ).toBe(1);

    feedFor('four-b').appendLines([note(8, SECOND_SAID)]);
    expect(woken, 'and a line in the terminal now on screen wakes it again').toBe(2);

    drop();
    lookAt('four-a');
    expect(woken, 'and unsubscribing really unsubscribes, switch included').toBe(2);
  });
});
