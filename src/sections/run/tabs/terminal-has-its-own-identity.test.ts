/* Terminal ma własną tożsamość, a folder jest jego polem.
 *
 * ZGŁOSZENIE, Z KTÓREGO TO WZIĘŁO SIĘ W CAŁOŚCI (właściciel, 2026-08-20): „jak klikam plusik to
 * powinno po prostu odpalać nowy nasz terminal i sobie tam możemy kolejne workflow w naszym scope
 * co mamy zaznaczone, a nie tak jak teraz że scope wybieramy znowu".
 *
 * CO STOI NA PRZESZKODZIE, zmierzone z kodu. Karta JEST dziś folderem — `cardForRun` zakłada ją
 * z `id: folder`, a `cardsIn` filtruje `tab.id === folder`. Magazyn mówi to wprost w swoim
 * nagłówku: „w jednym zakresie może stać najwyżej jedna karta". Dopóki tak jest, drugi terminal
 * w tym samym projekcie nie ma jak istnieć: albo dubluje pierwszą kartę, albo ją podmienia.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(cardsIn(...)).toHaveLength(2)` i nic więcej. Przechodzi
 * dla implementacji, która przestała filtrować po zakresie w ogóle — czyli dla paska, który
 * pokazuje człowiekowi karty KAŻDEGO projektu, jaki kiedykolwiek otworzył, bez sposobu na
 * odróżnienie, które z nich stoją tutaj. Rozstrzyga to trzeci przypadek niżej: terminal z innego
 * zakresu ma być niewidoczny w tym, a jego własny — widoczny u siebie.
 *
 * KONTROLA PRZECIW PUSTEMU PRZEJŚCIU jedzie w każdym przypadku, bo fikstura jest tu połową
 * asercji: trzy terminale w dwóch folderach. Dwa terminale o jednej tożsamości albo trzy w jednym
 * folderze zamieniłyby każde porównanie niżej w zdanie o liście jednoelementowej.
 *
 * `../io` PODSTAWIONE, bo magazyn kart bierze zatrzymanie biegu stamtąd, a prawdziwe zatrzymanie
 * woła Rusta przez granicę, której w vitest nie ma. Ten plik nie pyta o zatrzymanie ani razu —
 * zamyka wyłącznie karty, w których nikt nie pracuje.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

/* `closeTerminal` dołożone do atrapy 2026-08-20 razem z naprawą po sprawdzającym: magazyn kart
 * bierze stąd DWA kanały — zatrzymanie biegu i koniec rozmowy z liderem zamykanej karty. Ani
 * jedna asercja niżej się o to nie pyta i żadnej nie ubyło; bez tego wiersza vitest przewraca
 * się na dostępie do brakującego eksportu atrapy, czyli przed pierwszą asercją. */
vi.mock('../io', () => ({
  stop: vi.fn(() => Promise.resolve()),
  closeTerminal: vi.fn(() => Promise.resolve()),
}));

const { cardsIn, runTabs } = await import('./store');
const { newTerminal } = await import('./terminal');

/** Zakres, w którym człowiek pracuje. */
const HERE = '/Users/x/ledger-ui';

/** Drugi zakres — ten, którego karty nie mają prawa pojawić się w pierwszym. */
const THERE = '/Users/x/meetnotes';

beforeEach(() => {
  runTabs.setState({ tabs: [], activeId: null, pendingClose: null });
});

/**
 * Trzy terminale: dwa w `HERE`, jeden w `THERE` — i kontrola, że fikstura jest tym, czym mówi.
 *
 * Kontrola stoi TUTAJ, a nie w osobnym przypadku, żeby biegła przed każdym porównaniem niżej:
 * to ona odróżnia „pasek pokazał dwie karty" od „pasek pokazał tę samą kartę dwa razy".
 */
function threeTerminals(): {
  readonly first: string;
  readonly second: string;
  readonly elsewhere: string;
} {
  const first = newTerminal(HERE, 'ledger-ui');
  const second = newTerminal(HERE, 'ledger-ui');
  const elsewhere = newTerminal(THERE, 'meetnotes');

  const ids = new Set([first.id, second.id, elsewhere.id]);
  expect(
    ids.size,
    'the fixture has to hand out THREE different terminals, or every comparison below is a ' +
      'statement about one card counted twice. It handed out: ' +
      JSON.stringify([first.id, second.id, elsewhere.id]),
  ).toBe(3);
  const folders = new Set([first.path, second.path, elsewhere.path]);
  expect(
    folders.size,
    'and TWO folders, or the last case — a terminal from another project stays out of this bar ' +
      '— would be asking about one project measured twice',
  ).toBe(2);

  runTabs.getState().open(first);
  runTabs.getState().open(second);
  runTabs.getState().open(elsewhere);
  return { first: first.id, second: second.id, elsewhere: elsewhere.id };
}

/** Karty, które pasek pokazuje w tym zakresie — dokładnie ta droga, którą liczy je ekran. */
function shownIn(folder: string | null): readonly string[] {
  return cardsIn(runTabs.getState().tabs, folder).map((card) => card.id);
}

describe('a terminal carries its own name, and the folder is one of its fields', () => {
  it('opens two terminals in one project, each with its own name and the same folder', () => {
    const { first, second } = threeTerminals();

    const here = runTabs.getState().tabs.filter((card) => card.path === HERE);
    expect(
      here.map((card) => card.id),
      'two terminals opened in ONE project have to be two cards with two different names. ' +
        'Today the name of a card IS its folder, so the second one either lands on top of the ' +
        'first or replaces it — and the person who pressed + twice asked for two places to ' +
        'work in the project they had already chosen.',
    ).toEqual([first, second]);
    expect(
      here.map((card) => card.path),
      'and both of them stand in the same folder. That folder is what the card puts in its ' +
        'tooltip, so a terminal that lost it leaves the person with a card that cannot say ' +
        'where its work happens.',
    ).toEqual([HERE, HERE]);

    expect(
      shownIn(HERE),
      'the bar of this project has to carry BOTH terminals. A bar filtered by "the card whose ' +
        'name is this folder" carries at most one, which is the whole reason + never added ' +
        'anything a person could see.',
    ).toEqual([first, second]);
  });

  it('closes one terminal and leaves the other one standing, folder and all', () => {
    const { first, second } = threeTerminals();

    /* Karta bez pracujących agentów zamyka się od razu: pytanie zadaje się tylko wtedy, kiedy
     * jest o co (`./store.ts`, `requestClose`). Ten przypadek nie pyta o zatrzymanie biegu. */
    runTabs.getState().requestClose(first);

    expect(
      shownIn(HERE),
      'closing one terminal took the other one off the bar as well. Two terminals in one folder ' +
        'are two independent places to work, and closing the left one is not an instruction ' +
        'about the right one.',
    ).toEqual([second]);
    expect(
      runTabs.getState().tabs.find((card) => card.id === second)?.path,
      'the terminal that stayed lost the folder it stands in. That folder is the only thing ' +
        'saying where its work happens, and nothing about closing its neighbour changed it.',
    ).toBe(HERE);
  });

  it('keeps a terminal of another project out of this bar, and shows it in its own', () => {
    const { first, second, elsewhere } = threeTerminals();

    expect(
      shownIn(HERE),
      'a terminal belonging to another project showed up on this bar. This is the failure the ' +
        'filter exists to stop: a bar that stopped scoping shows every project a person ever ' +
        'opened, with nothing to tell them which cards are here.',
    ).toEqual([first, second]);
    expect(
      shownIn(THERE),
      'and the other project has to see its own terminal, not the first card on the list',
    ).toEqual([elsewhere]);
    expect(
      shownIn(null).length,
      'without a project there is nothing to filter BY, and a hidden card is work nobody can ' +
        'stop with x (invariant 6). So: no project, no filter.',
    ).toBe(3);
  });
});
