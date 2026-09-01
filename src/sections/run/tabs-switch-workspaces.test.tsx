/* AC-3 dla T-39: pasek kart jest zamontowany, jest TYM z `tabs/tab-bar.tsx`, i naprawdę
 * przestawia otwarty folder.
 *
 * SŁABA WERSJA: `expect(markup).toContain('TabBar')`. Nazwa komponentu nie występuje
 * w markupie ani razu, więc taka asercja jest albo zawsze czerwona, albo — po przepisaniu na
 * `toContain('data-tab-bar')` — zielona także wtedy, gdy ekran ma WŁASNĄ kopię paska.
 * Rozstrzyga podmiana modułu: atrapa opakowuje PRAWDZIWY `TabBar`, zapisuje propsy i renderuje
 * go dalej. Pusty zapis znaczy „ekran narysował coś innego", a nie „coś się nie skompilowało".
 *
 * DLACZEGO KLIKNIĘCIE JEST TU WYWOŁANIEM PROPSA. To repo nie ma jsdom — komponenty renderują
 * się statycznie (`renderToStaticMarkup`), a `onClick` nigdy się nie odpala. Klikamy więc to,
 * co klika przycisk: handler, KTÓRY EKRAN NAPRAWDĘ PODAŁ paskowi. Handler wpisany w test
 * dowodziłby czegoś o teście; ten przyjechał z `index.tsx` i zmienia magazyn albo nie zmienia.
 *
 * DWIE OSIE, PROSTOPADŁE (ARCHITECTURE §7). Boczne menu odpowiada „co robię", karty „w którym
 * folderze". Osi jest DWIE, więc ekran pracy nie ma prawa nieść trzeciej: żadnego przełącznika
 * sekcji w środku i żadnej karty, która udaje sekcję. Trzecia oś nawigacji to poprzedni prototyp,
 * w którym jedna rzecz dawała się otworzyć czterema drogami i żadna nie mówiła, gdzie stoisz.
 *
 * WYSOKOŚĆ 34 px CZYTAMY Z MAKIETY w tym samym biegu testu — karty biorą 34 z 96 px budżetu
 * chrome, a liczba wpisana z palca przechodzi także wtedy, gdy makieta mówi co innego.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { TabBarProps } from './tabs/tab-bar';
import { SECTIONS } from '../../ui/sections';

/* Zapis propsów żyje w `vi.hoisted`, bo fabryka `vi.mock` jest podnoszona nad importy i nie
 * widzi zwykłej stałej modułu. */
const { seen } = vi.hoisted(() => ({ seen: [] as unknown[] }));

vi.mock('./tabs/tab-bar', async (importOriginal) => {
  const real = await importOriginal<typeof import('./tabs/tab-bar')>();
  return {
    ...real,
    /* PRZELOTKA, nie zaślepka: prawdziwy pasek dalej się renderuje, więc asercje o wysokości
     * i o napisach mówią o komponencie, który stoi w repo, a nie o atrapie z tego pliku. */
    TabBar: (props: TabBarProps) => {
      seen.push(props);
      return real.TabBar(props);
    },
  };
});

const Run = (await import('./index')).default;
const { workspaces } = await import('./index');

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

function tight(value: string): string {
  return value.replace(/\s+/g, ' ').replace(/,\s+/g, ',').trim();
}

function ruleBody(css: string, selector: string): string {
  const found = new RegExp('\\' + selector + '\\s*\\{([^}]*)\\}').exec(css);
  return found?.[1] ?? '';
}

function property(body: string, name: string): string {
  const found = new RegExp('(?:^|;)\\s*' + name + '\\s*:([^;]*)').exec(body);
  return tight(found?.[1] ?? '');
}

const html = fileText(MOCKUP);

/** Dwa foldery — dwie karty. Nazwa karty to nazwa folderu, nigdy nazwa sekcji. */
const FIRST = {
  id: '/Users/x/meetnotes',
  name: 'meetnotes',
  path: '/Users/x/meetnotes',
  agents: 0,
};
const SECOND = {
  id: '/Users/x/ledger-ui',
  name: 'ledger-ui',
  path: '/Users/x/ledger-ui',
  agents: 0,
};

workspaces.getState().open(FIRST);
workspaces.getState().open(SECOND);

/** Rząd kart, wycięty z ekranu: od paska kart do paska loadoutu, czyli do następnego rzędu. */
function tabRow(markup: string): string {
  const opens = markup.indexOf('data-tab-bar');
  const closes = markup.indexOf('data-strip');
  return opens < 0 || closes < 0 ? '' : markup.slice(opens, closes);
}

/** Napisy na kartach — przyciski z pełną ścieżką w podpowiedzi, czyli te z `tabs/tab.tsx`. */
function tabLabels(markup: string): readonly string[] {
  /* Po `data-tab`, nie po `title`: `title` ma na tym pasku także `＋` (i będzie miał każdy
   * następny przycisk), więc czytnik oparty na podpowiedzi liczył jako kartę wszystko, co da się
   * kliknąć. Znacznik nazywa, czym element JEST. */
  return [
    ...tabRow(markup).matchAll(/<button[^>]*data-tab="[^"]*"[^>]*>([\s\S]*?)<\/button>/g),
  ].map((hit) => (hit[1] ?? '').replace(/<[^>]*>/g, '').trim());
}

/** Napis na karcie, którą pasek pokazuje jako otwartą. */
function currentLabel(markup: string): string {
  const hit = /<button[^>]*data-tab="[^"]*"[^>]*aria-current="true"[^>]*>([\s\S]*?)<\/button>/.exec(
    tabRow(markup),
  );
  return (hit?.[1] ?? '').replace(/<[^>]*>/g, '').trim();
}

/** Propsy z OSTATNIEGO renderu paska. */
function lastProps(): TabBarProps | undefined {
  return seen.at(-1) as TabBarProps | undefined;
}

describe('the workspace tabs are mounted, and clicking one really moves the open folder', () => {
  it('draws the bar from tabs/tab-bar.tsx exactly once, never a copy of its own', () => {
    seen.length = 0;
    const markup = renderToStaticMarkup(<Run />);

    expect(
      seen.length,
      'the run screen has to render the TabBar from src/sections/run/tabs/tab-bar.tsx. Zero ' +
        'means it draws tabs of its own — a second live place for one fact (invariant 13) and ' +
        'a second `×` with its own idea of what closing a folder does. More than one means two ' +
        'bars in one screen.',
    ).toBe(1);
    expect(
      tabLabels(markup),
      'the mounted bar has to carry one tab per open folder, named by the folder',
    ).toEqual([FIRST.name, SECOND.name]);
  });

  it('changes the open workspace in the store when the tab the screen wired is used', () => {
    renderToStaticMarkup(<Run />);
    const props = lastProps();
    expect(props, 'the screen handed the tab bar no props at all').toBeDefined();
    if (props === undefined) return;

    expect(
      workspaces.getState().activeId,
      'opening a folder puts it on top, so the second one has to be the open one before the ' +
        'click — otherwise the assertion below cannot tell a working handler from a no-op.',
    ).toBe(SECOND.id);

    props.onSelect(FIRST.id);

    expect(
      workspaces.getState().activeId,
      'using the handler the screen gave the tab bar has to change WHICH FOLDER IS OPEN in the ' +
        'store. A handler that is wired up and changes nothing looks identical in the markup ' +
        'and identical in a screenshot (invariant 16).',
    ).toBe(FIRST.id);

    expect(
      currentLabel(renderToStaticMarkup(<Run />)),
      'after the switch the bar has to show the first folder as the open one. The store and ' +
        'the screen disagreeing here is the state in which a person works in one folder and ' +
        'reads the name of another.',
    ).toBe(FIRST.name);
  });

  it('keeps the two navigation axes perpendicular, and there are two of them', () => {
    const markup = renderToStaticMarkup(<Run />);
    const labels = tabLabels(markup);

    expect(labels.length, 'no tabs were read out of the screen, so this case is free').toBe(2);
    expect(
      markup.includes('data-section-switch'),
      'the work screen carries a section switch of its own. "What am I doing" is answered by ' +
        'the side nav and nowhere else; a second answer inside the screen is a third axis, and ' +
        'a third axis is how the earlier prototype got four ways to open one thing.',
    ).toBe(false);

    const sections = SECTIONS.map((entry) => entry.label);
    expect(
      sections.length,
      'the section registry names no sections, so the check below is free',
    ).toBeGreaterThan(0);
    for (const label of labels) {
      expect(
        sections,
        'the tab ' +
          JSON.stringify(label) +
          ' is named like a section, not like a folder. Tabs answer "in which folder", the ' +
          'side nav answers "what am I doing" — one of them wearing the other\'s words is the ' +
          'moment the two axes stop being perpendicular.',
      ).not.toContain(label);
    }
  });

  it('gives the bar the height the mockup declares', () => {
    const wanted = property(ruleBody(html, '.tabs'), 'height');
    expect(
      wanted,
      'nothing was read out of the `.tabs` rule in docs/mockup/index.html, so the comparison ' +
        'below would pass on two empty strings.',
    ).not.toBe('');

    const markup = renderToStaticMarkup(<Run />);
    const style = /<div[^>]*\bdata-tab-bar="[^"]*"[^>]*style="([^"]*)"/.exec(markup)?.[1] ?? '';
    const rendered = tight(/(?:^|;)\s*height\s*:([^;]*)/.exec(style)?.[1] ?? '');

    expect(
      rendered,
      'the mounted tab bar declares no height at all. 34 px is what the tabs are allowed to ' +
        'spend out of the 96 px chrome budget (ARCHITECTURE §7), and a bar that sizes itself ' +
        'to its content spends whatever it likes.',
    ).not.toBe('');
    expect(
      rendered,
      'the bar and the mockup disagree about the height of the tabs row. The mockup `.tabs` ' +
        'rule says `' +
        wanted +
        '`, read here in this run.',
    ).toBe(wanted);
  });
});
