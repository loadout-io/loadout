/* Kryterium 5 dla T-24: czekanie na wolne miejsce jest wypowiedziane, nie przemilczane.
 *
 * Trzeci bieg przy limicie 2 stoi w kolejce. Milczące czekanie jest nieodróżnialne od
 * zawieszenia — człowiek patrzy na kartę, w której nic się nie rusza, i ubija bieg, który był
 * zdrowy. Pasek kart mówi więc jedno zdanie: ile miejsc jest zajętych, ile ich jest i w którym
 * folderze agent stoi w kolejce.
 *
 * SŁABA WERSJA: `getByText(/slots/)`. Przechodzi, kiedy zdanie wisi na pasku na stałe — a wtedy
 * kłamie przez większość czasu i najgłośniej wtedy, gdy miejsce właśnie się zwolniło.
 * Rozróżniają trzy rzeczy naraz: zdanie ZNIKA po zwolnieniu miejsca, występuje w dokumencie
 * DOKŁADNIE RAZ (niezmiennik 13) i niesie liczby, które się ZMIENIAJĄ — zdanie wpisane na
 * sztywno czyta się tak samo przy dwóch zajętych miejscach i przy trzech.
 *
 * DLACZEGO PASEK KART, A NIE WIDOK PRACY. Zdanie mówi o FOLDERZE, w którym agent czeka, czyli
 * o informacji kartowej. Widok pracy należy do T-08 i T-09; asercja postawiona na nim mierzyłaby
 * cudzy plik i nie mogłaby być czerwona z właściwego powodu (AGENTS.md §2a p. 5).
 *
 * Kształt liczby — `2 of 2` — jest tu wybrany, a nie odziedziczony: to jest zapis
 * z ARCHITECTURE §6a („2 z 3 zajęte"), a asercja na sam licznik bez sufitu przechodziłaby na
 * zdaniu, które mówi „2 in use" i nigdy nie zdradza, ile ich w ogóle jest.
 *
 * Bez jsdom: `renderToStaticMarkup` z `react-dom/server`.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { WorkspaceTab } from '../../../state/workspaces';
import { TabBar } from './tab-bar';

const MEETNOTES: WorkspaceTab = {
  id: 'ws-meetnotes',
  name: 'meetnotes',
  path: '/Users/you/Projects/meetnotes',
  agents: 1,
};

const SPREADSHEET: WorkspaceTab = {
  id: 'ws-spreadsheet',
  name: 'spreadsheet',
  path: '/Users/you/Projects/spreadsheet',
  agents: 1,
};

/** Trzeci folder: jego agent czeka, bo obie miejsca są zajęte przez dwa poprzednie. */
const LOADOUT: WorkspaceTab = {
  id: 'ws-loadout',
  name: 'Loadout',
  path: '/Users/you/Projects/Loadout',
  agents: 0,
};

const TABS = [MEETNOTES, SPREADSHEET, LOADOUT];

/** Znacznik zdania o czekaniu. */
const WAITING = 'data-slots-waiting';

function noop(): void {
  // Handlery są wymagane, ale to kryterium nie pyta, co robią.
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

function barMarkup(busy: number, atOnce: number, waitingIn: string | null): string {
  return renderToStaticMarkup(
    <TabBar
      tabs={TABS}
      activeId={MEETNOTES.id}
      busy={busy}
      atOnce={atOnce}
      waitingIn={waitingIn}
      onSelect={noop}
      onClose={noop}
      onOpenFolder={noop}
    />,
  );
}

/** Treść jedynego elementu ze znacznikiem czekania, bez znaczników i bez nadmiarowych odstępów. */
function waitingText(markup: string): string {
  const hit = /<([a-z]+)[^>]*\bdata-slots-waiting\b[^>]*>([\s\S]*?)<\/\1>/i.exec(markup);
  return (hit?.[2] ?? '')
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('the tab bar when an agent is queued for a free slot', () => {
  it('says so exactly once', () => {
    expect(
      occurrences(barMarkup(2, 2, LOADOUT.name), WAITING),
      'with both slots taken and a third agent queued the bar has to say so, and say it once. ' +
        'How many slots are in use is one fact, so it gets exactly one live place on screen ' +
        '(invariant 13); poprzedni prototyp showed its connection state in six',
    ).toBe(1);
  });

  it('carries the sentence in that element rather than beside it', () => {
    expect(
      waitingText(barMarkup(2, 2, LOADOUT.name)),
      'the marked element has to hold the words themselves, otherwise there is no way to tell ' +
        'a sentence that was worked out from a decorative one',
    ).not.toBe('');
  });

  it('names the folder where the agent is queued', () => {
    const said = waitingText(barMarkup(2, 2, LOADOUT.name));
    expect(
      said.includes(LOADOUT.name),
      'the sentence has to name the folder to look in. "Something somewhere is waiting" does ' +
        'not tell a person which of three tabs to open, and that is the only reason this ' +
        'sentence is on screen at all. It said: ' +
        said,
    ).toBe(true);

    const elsewhere = waitingText(barMarkup(2, 2, SPREADSHEET.name));
    expect(
      elsewhere.includes(SPREADSHEET.name) && !elsewhere.includes(LOADOUT.name),
      'and it has to name the folder that is actually queued, not one picked at build time. ' +
        'It said: ' +
        elsewhere,
    ).toBe(true);
  });

  it('counts the busy slots rather than naming a number once', () => {
    const two = waitingText(barMarkup(2, 2, LOADOUT.name));
    const three = waitingText(barMarkup(3, 3, LOADOUT.name));

    expect(two, 'two slots in use out of two').toContain('2 of 2');
    expect(three, 'three slots in use out of three').toContain('3 of 3');
    expect(
      two,
      'and the two readings have to differ. A sentence typed in by hand reads the same at two ' +
        'busy slots and at three, which is the version that keeps saying 2 of 2 while the ' +
        'person raised the number an hour ago',
    ).not.toBe(three);
  });

  it('stops saying it the moment a slot comes free', () => {
    expect(
      occurrences(barMarkup(1, 2, null), WAITING),
      'nobody is queued any more, so the sentence has to be gone — the element, not just its ' +
        'words. A line that stays up after the wait is over is the one that teaches people to ' +
        'stop reading the bar',
    ).toBe(0);
  });
});
