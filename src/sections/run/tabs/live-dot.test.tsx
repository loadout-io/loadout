/* Kryterium 4 dla T-24: kropka jest tam, gdzie coś chodzi — i NIE MA JEJ tam, gdzie nie chodzi.
 *
 * Trzy karty: ta, na którą patrzysz, z pracującym agentem; jedna w tle z pracującym agentem;
 * jedna w tle bez niczego. Pierwsze dwie mają kropkę, trzecia nie ma.
 *
 * SŁABA WERSJA: „kropka istnieje na karcie z biegiem". Przechodzi na implementacji, w której
 * kropka jest ZAWSZE i zmienia się tylko jej przezroczystość — a wtedy karta, w której nic nie
 * chodzi, dalej przyciąga wzrok, dalej przesuwa napis i dalej kłamie przy każdej zmianie
 * jasności motywu. Rozróżnia asercja o NIEOBECNOŚCI elementu dla trzeciej karty, nie o jego
 * stylu: `toBe(0)`, nie „ma inny kolor".
 *
 * Druga połowa jest o słowniku kolorów. Kropka bierze `--live`, bo TO on znaczy „teraz".
 *
 * PRZEPISANE 2026-08-19 (T-47), nie skasowane. Do T-45 ten punkt żądał `--accent` i uzasadniał
 * się zdaniem „accent znaczy teraz" — a T-45 to zdanie unieważnił: `--accent` odpowiada wyłącznie
 * na „to jest interaktywne", a „teraz" dostało własny token. Kropka na karcie w tle nie jest
 * kontrolką; jest odczytem, i to jedynym, jaki ta karta o sobie robi. Zostawiona tak, jak była,
 * ta wyrocznia broniłaby reguły, której DESIGN.md już nie stawia.
 *
 * Karta nieaktywna nadal nie ma prawa do żadnego z pozostałych trzech kolorów stanu:
 * `--attend` pyta „co czeka na moją decyzję", `--fail` mówi „zepsute", `--human` znaczy „zrobił
 * to człowiek". Karta w tle mówi o sobie dokładnie jedno zdanie i żadne z tych trzech nim nie
 * jest (ARCHITECTURE §6a reguła 4).
 *
 * Bez jsdom: `renderToStaticMarkup` z `react-dom/server`. Dopisanie `@testing-library/react`
 * to zmiana `package.json`, czyli moment na zatrzymanie się i zapytanie człowieka
 * (AGENTS.md §7).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { WorkspaceTab } from '../../../state/run-tabs';
import { Tab } from './tab';
import { TabBar } from './tab-bar';

/** Karta, na którą człowiek patrzy, i w której pracuje dwóch agentów. */
const WATCHED: WorkspaceTab = {
  id: 'ws-meetnotes',
  name: 'meetnotes',
  path: '/Users/you/Projects/meetnotes',
  agents: 2,
};

/** Karta w tle, w której też ktoś pracuje. To jest ta, o którą naprawdę chodzi. */
const BACKGROUND: WorkspaceTab = {
  id: 'ws-spreadsheet',
  name: 'spreadsheet',
  path: '/Users/you/Projects/spreadsheet',
  agents: 1,
};

/** Karta w tle, w której nic nie chodzi. */
const QUIET: WorkspaceTab = {
  id: 'ws-loadout',
  name: 'Loadout',
  path: '/Users/you/Projects/Loadout',
  agents: 0,
};

/** Znacznik kropki. Pytamy o element, nie o kolor — bo pytanie brzmi „czy on w ogóle jest". */
const DOT = 'data-live-dot';

/**
 * Trzy pozostałe kolory stanu z DESIGN §3.
 *
 * Wypisane tutaj, a nie czytane z motywu: to jest kontrakt między tym kryterium a projektem,
 * a lista wyczytana z tego samego pliku, który komponent importuje, zgadzałaby się sama ze sobą.
 */
const COLOURS_A_QUIET_TAB_MAY_NOT_USE = ['attend', 'fail', 'human'];

function noop(): void {
  // Handlery są wymagane, ale to kryterium nie pyta, co robią.
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

function tabMarkup(workspace: WorkspaceTab, active: boolean): string {
  return renderToStaticMarkup(
    <Tab workspace={workspace} active={active} onSelect={noop} onClose={noop} />,
  );
}

/** Otwierający znacznik elementu niosącego kropkę, razem z jego atrybutami. */
function dotElement(markup: string): string {
  const hit = /<[a-z]+[^>]*\bdata-live-dot\b[^>]*>/i.exec(markup);
  return hit?.[0] ?? '';
}

describe('the dot that says an agent is working in this folder', () => {
  it('is on the tab you are looking at when somebody is working there', () => {
    expect(
      occurrences(tabMarkup(WATCHED, true), DOT),
      'the tab on top with two agents at work has to carry exactly one dot',
    ).toBe(1);
  });

  it('is on a tab in the background just the same', () => {
    expect(
      occurrences(tabMarkup(BACKGROUND, false), DOT),
      'this is the whole reason the dot exists: without it you forget that something is still ' +
        'running in another folder, and you pay for that in your monthly limit. Being in the ' +
        'background is not something that happens to a run',
    ).toBe(1);
  });

  it('is absent — not dimmed — on a tab where nothing is running', () => {
    expect(
      occurrences(tabMarkup(QUIET, false), DOT),
      'the element itself has to be missing, not faded. A dot that is always drawn and only ' +
        'changes its transparency still holds width, still pushes the name across and still ' +
        'catches the eye the moment anybody lightens the theme',
    ).toBe(0);
  });

  it('takes its colour from the one that means now, which is no longer the accent', () => {
    const cases: readonly (readonly [WorkspaceTab, boolean])[] = [
      [WATCHED, true],
      [BACKGROUND, false],
    ];
    for (const [workspace, active] of cases) {
      const dot = dotElement(tabMarkup(workspace, active));
      expect(
        dot,
        'there has to be an element carrying the dot in ' +
          workspace.name +
          ', otherwise there is nothing whose colour could be asked about',
      ).not.toBe('');
      expect(
        /\blive\b/.test(dot),
        'the dot in ' +
          workspace.name +
          ' answers one question — what is happening right now — and since 2026-08-19 there is a ' +
          'colour whose whole job is that answer. A colour spelled out in the component instead ' +
          'of named would also put a fifth meaning on a palette that has room for four.',
      ).toBe(true);
      expect(
        /\baccent\b/.test(dot),
        'the dot in ' +
          workspace.name +
          ' carries the accent, which now means "this is interactive" and nothing else. The dot ' +
          'is a readout: it says a run is alive in a folder you are not looking at.',
      ).toBe(false);
    }
  });

  it('leaves every other state colour off a tab that is not on top', () => {
    for (const workspace of [BACKGROUND, QUIET]) {
      const markup = tabMarkup(workspace, false);
      for (const colour of COLOURS_A_QUIET_TAB_MAY_NOT_USE) {
        expect(
          markup.includes(colour),
          'a tab in the background says exactly one thing about itself — something is running ' +
            'here — and ' +
            colour +
            ' answers a different question entirely. Found it in the markup of ' +
            workspace.name,
        ).toBe(false);
      }
    }
  });

  it('draws one dot per working folder and no more, across the whole bar', () => {
    const markup = renderToStaticMarkup(
      <TabBar
        tabs={[WATCHED, BACKGROUND, QUIET]}
        activeId={WATCHED.id}
        busy={3}
        atOnce={3}
        waitingIn={null}
        onSelect={noop}
        onClose={noop}
        onOpenFolder={noop}
      />,
    );
    expect(
      occurrences(markup, DOT),
      'three tabs, two of them with agents at work: the bar as a whole has to hold two dots. ' +
        'One fact, one live place on screen (invariant 13)',
    ).toBe(2);
  });
});
