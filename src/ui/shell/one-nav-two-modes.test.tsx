/* JEDNA nawigacja o dwóch trybach — i ani jednego miejsca, do którego prowadzą dwie kontrolki.
 *
 * CO TU BYŁO ZEPSUTE, ZMIERZONE 2026-08-31. `src/ui/shell/titlebar.tsx` rysował DWIE kontrolki
 * na tę samą pracę i obie naraz: wąską kolumnę glifów (`data-jump`) i listę wierszy
 * (`data-section-switch`). Obie wołały `useSectionStore.go(entry.id)`, więc każde z siedmiu
 * miejsc miało dwie drogi stojące obok siebie — jeden fakt, dwa nośniki (niezmiennik 13).
 * Zdanie właściciela brzmiało: „nawigacja na pasku vs ta sidebar to to samo, więc możemy zrobić
 * z tego 2 mode, jeden dla collapsed, drugi expanded".
 *
 * DLACZEGO KRYTERIUM LICZY KONTROLKI, A NIE ATRYBUTY. Asercja „jest dokładnie jeden
 * `data-section-switch` na sekcję" przechodziła przez CAŁY czas trwania tej wady: drugi nośnik
 * nazywał się inaczej. Punkt niżej pyta więc o coś, czego nie da się obejść przemianowaniem —
 * ile przycisków bocznego menu w ogóle NIESIE identyfikator danej sekcji, w jakimkolwiek
 * atrybucie. Dwa nośniki dają dwa niezależnie od tego, jak się nazywają.
 *
 * PODMIOTEM JEST TO, CO WIDZI CZŁOWIEK (niezmiennik 29): wszystko niżej jest czytane
 * z wyrenderowanej powłoki, w obu trybach, a nie z wartości zwróconej przez funkcję.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { App } from '../../App';
import { useRun } from '../../state/run';
import { collapseNav, navIsCollapsed, subscribeToNavCollapsed } from '../../state/settings';
import { moveFor } from '../palette/keys';
import { SECTIONS } from '../sections';
import { useWhatYouHave } from './what-you-have';
import { NAV_NARROW, NAV_WIDTH } from './titlebar';

/* Atrapa granicy, podniesiona razem z `vi.mock`. Zwinięcie menu ZAPISUJE wybór do tego samego
 * pliku, co lider i sufit wydatku, więc bez atrapy ten plik pytałby prawdziwego Tauri. Ta droga
 * mierzy WYŁĄCZNIE to, co pojechało w stronę Rusta, i to, co okno zrobiło z odpowiedzią. */
const { invoked, answer } = vi.hoisted(() => ({
  invoked: vi.fn((..._sent: unknown[]) => Promise.resolve(undefined as unknown)),
  answer: { of: undefined as unknown },
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...sent: unknown[]) => {
    invoked(...sent);
    return Promise.resolve(answer.of);
  },
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

/** Powłoka w tym trybie, na tym, co człowiek naprawdę ma. */
function shell(): string {
  return renderToStaticMarkup(<App section="run" screens={{}} />);
}

/** Boczne menu wycięte z powłoki. */
function nav(markup: string): string {
  return /<nav[\s\S]*?<\/nav>/.exec(markup)?.[0] ?? '';
}

/** Znaczniki otwierające wszystkich przycisków bocznego menu. */
function navButtons(markup: string): readonly string[] {
  return [...nav(markup).matchAll(/<button[^>]*>/g)].map((hit) => hit[0]);
}

/** Blok przycisku, który prowadzi do tej sekcji — od znacznika otwierającego do zamykającego. */
function switchFor(markup: string, id: string): string {
  const found = new RegExp('<button[^>]*data-section-switch="' + id + '"[\\s\\S]*?</button>');
  return found.exec(nav(markup))?.[0] ?? '';
}

/**
 * Czy ten przycisk niesie ten identyfikator — w JAKIMKOLWIEK swoim atrybucie.
 *
 * Porównujemy CAŁĄ wartość, nie zawieranie: etykieta „Run" i podpowiedź „Run · ⌘3" mówią
 * o miejscu słowami dla człowieka i nie są drugą drogą do niego, a identyfikator w atrybucie
 * jest dokładnie tym, czym kryterium wchodzi na sekcję.
 */
function carries(tag: string, id: string): boolean {
  return [...tag.matchAll(/="([^"]*)"/g)].some((hit) => hit[1] === id);
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Wartość atrybutu ze znacznika otwierającego, albo pusty napis. */
function attribute(tag: string, name: string): string {
  return new RegExp('\\b' + name + '="([^"]*)"').exec(tag)?.[1] ?? '';
}

/** Nic nie ma i nikt nic nie zaczął — świat, w którym kłódki są prawdziwe. */
const NOTHING = { agents: 0, workflows: 0 };

/** Sekcje, których na pustej maszynie nie da się jeszcze użyć, razem z powodem z rejestru. */
const SHUT = SECTIONS.filter((entry) => entry.needs !== null);

beforeEach(() => {
  invoked.mockClear();
  answer.of = undefined;
  useWhatYouHave.setState(NOTHING);
  useRun.setState({ workflow: '' });
});

afterEach(async () => {
  await collapseNav(false);
});

describe('the side nav is one navigation with two modes, never two navigations', () => {
  it('reaches each place with exactly one control, in each of the two modes', async () => {
    for (const collapsed of [false, true]) {
      await collapseNav(collapsed);
      const buttons = navButtons(shell());
      const named = collapsed ? 'collapsed' : 'expanded';

      expect(
        buttons.length,
        'no button was read out of the side nav in the ' +
          named +
          ' mode, so every count below would pass on an empty list',
      ).toBeGreaterThan(0);

      for (const entry of SECTIONS) {
        expect(
          buttons.filter((tag) => carries(tag, entry.id)).length,
          'in the ' +
            named +
            ' mode the side nav offers more than one way into ' +
            entry.id +
            ', or none at all. Two controls for one place is one fact with two carriers ' +
            '(invariant 13): the person reads a narrow strip of icons and a list of rows ' +
            'standing beside each other, and both land on the same screen. One navigation, ' +
            'two modes — the icons ARE the list, narrowed.',
        ).toBe(1);
      }
    }
  });

  it('carries the same marker in both modes, so the keyboard and the mouse keep working', async () => {
    for (const collapsed of [false, true]) {
      await collapseNav(collapsed);
      const markup = shell();
      const named = collapsed ? 'collapsed' : 'expanded';

      for (const entry of SECTIONS) {
        expect(
          occurrences(nav(markup), 'data-section-switch="' + entry.id + '"'),
          'in the ' +
            named +
            ' mode nothing carries data-section-switch="' +
            entry.id +
            '" exactly once. That marker is how every browser criterion in e2e/ walks into a ' +
            'section, so a mode that drops it or doubles it takes the whole suite down with it.',
        ).toBe(1);
      }

      expect(
        occurrences(markup, 'aria-current="true"'),
        'in the ' +
          named +
          ' mode the shell says which place is open ' +
          String(occurrences(markup, 'aria-current="true"')) +
          ' times. Exactly one: zero means it never says where you are, two mean it says it ' +
          'twice and one of them is wrong (invariant 13).',
      ).toBe(1);
    }
  });

  it('names every place for a person who can no longer read a label', async () => {
    await collapseNav(true);
    const markup = shell();

    for (const entry of SECTIONS) {
      const tag = /<button[^>]*>/.exec(switchFor(markup, entry.id))?.[0] ?? '';
      expect(tag, 'the collapsed nav renders no control for ' + entry.id + ' at all').not.toBe('');
      expect(
        attribute(tag, 'aria-label'),
        'the collapsed nav shows ' +
          entry.id +
          ' as a bare glyph and gives it no name. A strip of seven unlabelled shapes is a ' +
          'guessing game for the eye and silence for a screen reader — the label has to survive ' +
          'the narrowing as the accessible name.',
      ).toContain(entry.label);
      expect(
        attribute(tag, 'title'),
        'the collapsed nav gives ' +
          entry.id +
          ' no hover title, so the only way to learn what a glyph means is to press it and see ' +
          'where you land',
      ).toContain(entry.label);
    }
  });

  it('keeps the lock visible when only the glyph is left, and keeps its reason reachable', async () => {
    await collapseNav(true);
    const markup = shell();

    expect(
      SHUT.length,
      'no section in the registry declares what it needs, so this point would demand nothing',
    ).toBeGreaterThan(0);

    for (const entry of SHUT) {
      const block = switchFor(markup, entry.id);
      expect(block, 'the collapsed nav renders no control for ' + entry.id).not.toBe('');
      expect(
        block,
        'the collapsed nav drops the lock on ' +
          entry.id +
          '. A locked place that looks exactly like an open one is a click into a dead screen, ' +
          'and the person is never told why — which is the one thing the lock exists to say.',
      ).toContain('data-nav-locked');
      expect(
        attribute(/<button[^>]*>/.exec(block)?.[0] ?? '', 'title'),
        'the collapsed nav shows a lock on ' +
          entry.id +
          ' and nowhere says what would unlock it. "You may not" without "here is what would ' +
          'change that" is the sentence this whole navigation exists to delete.',
      ).toContain(entry.needs?.why ?? '');
    }
  });

  it('still admits from the icons alone that work is going', async () => {
    await collapseNav(true);

    useRun.setState({ workflow: '' });
    const quiet = switchFor(shell(), 'run');
    useRun.setState({ workflow: 'Ship a feature' });
    const going = switchFor(shell(), 'run');

    expect(
      quiet,
      'the collapsed nav wears the live mark while nothing is running. A mark that is always ' +
        'there says nothing when it matters.',
    ).not.toContain('data-nav-live');
    expect(
      going,
      'a run is going and the collapsed nav is silent about it. Being able to see from any ' +
        'screen that work is happening somewhere is the whole reason this mark lives in the ' +
        'navigation — narrowing the column is not a reason to hide it.',
    ).toContain('data-nav-live');
  });

  it('folds and unfolds from one control, and the shell really moves', async () => {
    await collapseNav(false);
    const open = shell();
    const openTag = /<nav[^>]*>/.exec(open)?.[0] ?? '';

    expect(
      occurrences(nav(open), 'data-nav-fold'),
      'the expanded nav offers no way to narrow itself. The house puts "Collapse sidebar" at ' +
        'the bottom of the panel; without it the two modes exist and nobody can reach the ' +
        'second one.',
    ).toBe(1);
    expect(
      attribute(openTag, 'style'),
      'the expanded nav is not ' +
        String(NAV_WIDTH) +
        ' px wide, so the mode a person opens on is not the one the mockup draws',
    ).toContain(String(NAV_WIDTH));

    await collapseNav(true);
    const shut = shell();
    const shutTag = /<nav[^>]*>/.exec(shut)?.[0] ?? '';

    expect(
      navIsCollapsed(),
      'the control that narrows the nav left the window thinking it is still wide. A handler ' +
        'that is wired up and has no effect looks identical in the markup and identical in a ' +
        'screenshot — that is the whole family of defect this criterion exists to catch.',
    ).toBe(true);
    expect(
      attribute(shutTag, 'style'),
      'the nav was narrowed and it still declares the wide width. The mode is a number a person ' +
        'sees, not a flag in a store.',
    ).toContain(String(NAV_NARROW));
    expect(
      occurrences(nav(shut), 'data-nav-fold'),
      'the collapsed nav offers no way back. A one-way fold is a trap: the list of places, the ' +
        'counts, the reasons and the next step are all behind it.',
    ).toBe(1);

    await collapseNav(false);
    expect(
      attribute(/<nav[^>]*>/.exec(shell())?.[0] ?? '', 'style'),
      'the nav folded and would not unfold. It has to work in both directions.',
    ).toContain(String(NAV_WIDTH));
  });

  it('tells everybody watching, so the shell redraws instead of waiting for the next click', async () => {
    let told = 0;
    const stop = subscribeToNavCollapsed(() => {
      told += 1;
    });
    try {
      await collapseNav(true);
      expect(
        told,
        'the mode changed and nobody subscribed to it heard. The shell reads this through ' +
          'useSyncExternalStore, so a change nobody announces is a nav that folds only after ' +
          'something else happens to redraw the window.',
      ).toBeGreaterThan(0);
    } finally {
      stop();
    }
  });

  it('remembers the mode in the one file that remembers what Loadout does by default', async () => {
    await collapseNav(true);

    const wrote = invoked.mock.calls.filter((call) => call[0] === 'save_settings');
    expect(
      wrote.length,
      'narrowing the nav wrote nothing to disk, so the choice dies with the window and the ' +
        'person makes it again on every launch. That is the same defect the default lead had ' +
        'before 2026-08-29, and it is fixed the same way: through save_settings, the one path ' +
        'this repo has for a choice that outlives the window.',
    ).toBe(1);
    expect(
      (wrote[0]?.[1] ?? {}) as Record<string, unknown>,
      'save_settings was called without saying what the nav mode now is. The file is one, so ' +
        'the write carries the whole entry — a call missing this key remembers everything ' +
        'except the thing that just changed.',
    ).toMatchObject({ navCollapsed: true });
  });

  it('opens on the mode the file remembers, not on the one the code ships with', async () => {
    /* ŚWIEŻE MODUŁY, czyli świeże okno — i to jest jedyny uczciwy kształt tego punktu.
       `loadSettings()` pyta dysk RAZ na okno i od tej chwili oddaje tę samą obietnicę, więc
       wołanie go po zapisie z punktu wyżej nie dotknęłoby pliku ani razu i sprawdzałoby
       pamięć tego procesu. `vi.resetModules()` daje magazyn w stanie, w jakim ma go człowiek
       otwierający aplikację; powłokę importujemy razem z nim, żeby czytała TEN magazyn. */
    vi.resetModules();
    answer.of = { defaultLead: '', defaultBudgetUsd: 75, navCollapsed: true };
    const fresh = await import('../../state/settings');
    const { App: FreshApp } = await import('../../App');
    await fresh.loadSettings();

    expect(
      fresh.navIsCollapsed(),
      'the file says the person left the nav collapsed and the window opened it wide anyway. A ' +
        'choice that is written and never read back is a write nobody asked for.',
    ).toBe(true);
    const drawn = renderToStaticMarkup(<FreshApp section="run" screens={{}} />);
    expect(
      attribute(/<nav[^>]*>/.exec(drawn)?.[0] ?? '', 'style'),
      'the disk was read, the mode came back collapsed, and the shell still draws the wide nav',
    ).toContain(String(NAV_NARROW));
  });

  it('draws the key that folds it, and the keyboard really takes that key', async () => {
    await collapseNav(false);

    /* KLAWISZ MA BYĆ NARYSOWANY, nie schowany w podpowiedzi — zmierzone mutacją 2026-08-31.
       Pierwsza wersja tego punktu pytała, czy `⌘B` stoi gdziekolwiek w bocznym menu, i była
       ZIELONA po skasowaniu klawiszy z ekranu: napis został w atrybucie `title`, czyli
       w miejscu, które trzeba najpierw znaleźć myszą i przytrzymać. Podpowiedź jest dla kogoś,
       kto już wie, że jest czego szukać. Pytamy więc o klawisz W TREŚCI, w `<kbd>`, tak jak
       rysują go wiersze sekcji obok. */
    const caps = [...nav(shell()).matchAll(/<kbd[^>]*>([^<]*)<\/kbd>/g)].map((hit) =>
      (hit[1] ?? '').trim(),
    );
    expect(
      caps,
      'the fold control draws no key cap. A shortcut nobody is shown is a shortcut nobody has: ' +
        'the person has to learn it by looking, not by reading documentation and not by ' +
        'hovering over a control they already found. The nav draws these caps: ' +
        JSON.stringify(caps),
    ).toContain('⌘B');

    expect(
      moveFor(
        { key: 'b', metaKey: true, ctrlKey: false, altKey: false, shiftKey: false },
        null,
        false,
      ),
      'the nav draws ⌘B beside the fold control and the keyboard does nothing with it. A drawn ' +
        'key that does nothing is a control without a handler (invariant 16): the person ' +
        'presses it, the window stands still, and the only thing learned is that they cannot ' +
        'work this app.',
    ).toEqual({ move: 'sidebar' });

    expect(
      moveFor(
        { key: 'b', metaKey: false, ctrlKey: false, altKey: false, shiftKey: false },
        { tagName: 'INPUT', isContentEditable: false },
        false,
      ),
      'a bare b folds the nav while somebody is typing. The modifier is what tells a shortcut ' +
        'apart from a letter, and this one has to stay out of every text field.',
    ).toEqual({ move: 'none' });
  });
});
