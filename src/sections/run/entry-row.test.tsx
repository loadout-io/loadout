/* AC-4 dla T-39: wiersz wejścia istnieje, a jego kontrolka NAPRAWDĘ coś robi.
 *
 * SŁABA WERSJA: sprawdzenie samego `<input>`. Pole bez handlera obiecuje sposób pracy, którego
 * nie ma, i jest gorsze niż jego brak (niezmiennik 16) — a w markupie wygląda dokładnie tak
 * samo jak pole podpięte. Dlatego to kryterium wywołuje handler, KTÓRY EKRAN PODAŁ wierszowi,
 * i pyta magazyn, czy coś się po nim zmieniło.
 *
 * ZACHĘTA NIE MA PRAWA OBIECYWAĆ WIĘCEJ, NIŻ WIERSZ UMIE. Makieta pisze w tym polu
 * `/plan · /run · or just say what you want` — planisty w tym repo nie ma, a `/run` potrzebuje
 * limitu „ile naraz", który mieszka w suwaku obok Startu (dwa miejsca na jedną liczbę to
 * niezmiennik 13 złamany w argumencie decydującym, ilu agentów ruszy). Zamiast przepisywać
 * obietnicę, test czyta KAŻDE słowo z ukośnikiem z wyrenderowanej zachęty i wymaga, żeby wiersz
 * je rozumiał. Dopisanie `/plan` do napisu zapala ten test, zanim zobaczy je człowiek.
 *
 * CO CZYTAMY Z MAKIETY: budowę wiersza, nie jego copy. Znak zachęty i klawisz z makiety
 * (`.entry .p`, `.entry kbd`) plus obecność pola i podpowiedzi — wpisane z palca przechodziłyby
 * także wtedy, gdy makieta zmieni je na co innego.
 *
 * ŻARGON SPRAWDZAMY TABELĄ Z PLIKU, nie własną listą (DESIGN §8, niezmiennik 14). Lista
 * przepisana do testu starzeje się w tydzień i wtedy „przechodzi tabelę" znaczy „przechodzi
 * tabelę sprzed tygodnia".
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { EntryProps } from './entry/entry';
import { understand } from './entry/entry';

const PICKED = '/Users/x/ledger-ui';

const { seen, chosen } = vi.hoisted(() => ({
  seen: [] as unknown[],
  /* Okno wyboru folderu po stronie systemu. Atrapa w teście jednostkowym jest w porządku —
   * w vitest nie ma okna Tauri — a to, co ona zwraca, jest dokładnie tym, co zwraca wtyczka:
   * ścieżka albo `null`, kiedy człowiek się rozmyślił. */
  chosen: vi.fn(() => Promise.resolve('/Users/x/ledger-ui')),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: chosen }));

vi.mock('./entry/entry', async (importOriginal) => {
  const real = await importOriginal<typeof import('./entry/entry')>();
  return {
    ...real,
    /* Przelotka: prawdziwy wiersz dalej się renderuje, więc asercje o zachęcie mówią
     * o komponencie z repo, a nie o atrapie z tego pliku. */
    Entry: (props: EntryProps) => {
      seen.push(props);
      return real.Entry(props);
    },
  };
});

const Run = (await import('./index')).default;
const { workspaces } = await import('./index');

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');
const DESIGN = resolve(ROOT, 'docs/design/DESIGN.md');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

const html = fileText(MOCKUP);
const design = fileText(DESIGN);

/** Wiersz wejścia z makiety: od reguły `.entry` w treści do listy agentów, która stoi za nim. */
function mockupEntry(): string {
  const opens = html.indexOf('class="entry"');
  const closes = html.indexOf('class="rail"');
  return opens < 0 || closes < 0 ? '' : html.slice(opens, closes);
}

/** Wiersz wejścia z ekranu. */
function screenEntry(markup: string): string {
  const opens = markup.indexOf('data-entry');
  const closes = markup.indexOf('</form>');
  return opens < 0 || closes < 0 ? '' : markup.slice(opens, closes);
}

function placeholderIn(piece: string): string {
  return /<input[^>]*placeholder="([^"]*)"/.exec(piece)?.[1] ?? '';
}

/** Lewa kolumna tabeli żargonu z DESIGN §8 — słowa, których UI nie pisze. */
function bannedWords(): readonly string[] {
  const table = design.slice(design.indexOf('## 8.'), design.indexOf('## 9.'));
  const cells = [...table.matchAll(/^\|([^|]+)\|/gm)].map((hit) => (hit[1] ?? '').trim());
  const words: string[] = [];
  for (const cell of cells) {
    if (cell === '' || cell.startsWith('---') || cell === 'Zamiast') continue;
    for (const piece of cell.split(/[/,]/)) {
      const word = piece.replaceAll('`', '').replaceAll('*', '').trim().toLowerCase();
      /* Wiersze objaśniające („(nic — nazwij czynność…)") nie są słowami do zakazania. */
      if (word === '' || word.includes('(')) continue;
      words.push(word);
    }
  }
  return words;
}

/** Każde słowo z ukośnikiem, które zachęta wymienia. */
function commandsIn(prompt: string): readonly string[] {
  return [...prompt.matchAll(/\/[a-z][a-z-]*/g)].map((hit) => hit[0]);
}

describe('the entry row is there and its control really does something', () => {
  it('carries the pieces the mockup builds that row from', () => {
    const markup = renderToStaticMarkup(<Run />);
    const wanted = mockupEntry();
    expect(
      wanted,
      'nothing was read out of the `.entry` block of docs/mockup/index.html, so every ' +
        'comparison below would run against an empty string.',
    ).not.toBe('');

    const glyph = /class="p">([^<]+)</.exec(wanted)?.[1]?.trim() ?? '';
    const key = /<kbd>([^<]+)<\/kbd>/.exec(wanted)?.[1]?.trim() ?? '';
    expect(glyph, 'the mockup entry row has to carry its prompt glyph').not.toBe('');
    expect(key, 'the mockup entry row has to name the key that submits the line').not.toBe('');

    const row = screenEntry(markup);
    expect(
      row,
      'the run screen renders no entry row at all. The mockup gives the stream column three ' +
        'rows and this is the third one; without it the screen has nowhere to type and the ' +
        'empty state has nothing to invite anybody to.',
    ).not.toBe('');
    expect(row, 'the row has to carry the prompt glyph the mockup draws (' + glyph + ')').toContain(
      glyph,
    );
    expect(row, 'the row has to name the same key the mockup does (' + key + ')').toContain(key);
    expect(
      placeholderIn(row),
      'the field has to greet an empty screen with something to type. An empty placeholder is ' +
        'the "no data" version of this row (DESIGN §6).',
    ).not.toBe('');
    expect(
      row,
      'the row has to carry the second line the mockup gives it (`.entry .hint`) — the one ' +
        'that says what happens when you press the key.',
    ).toContain('data-entry-hint');
  });

  it('understands every command its own prompt names, and nothing it cannot do', () => {
    const prompt = placeholderIn(screenEntry(renderToStaticMarkup(<Run />)));
    const named = commandsIn(prompt);

    expect(
      named.length,
      'the prompt names no command at all, so "every command it names is real" would be a ' +
        'statement about an empty set. The prompt says: ' +
        JSON.stringify(prompt),
    ).toBeGreaterThan(0);

    for (const command of named) {
      expect(
        understand(command),
        'the prompt offers ' +
          command +
          ', and the row does not understand it. A line that answers "I do not know that" to ' +
          'the very words it printed in its own field promises a way of working that does not ' +
          'exist (invariant 16) — which is exactly what happens if the mockup copy is pasted ' +
          'in whole, `/plan` and all.',
      ).toBe(command);
    }

    expect(
      understand('please rewrite the parser'),
      'prose is not a command and the row has to say so, not swallow it silently',
    ).toBeNull();
  });

  it('changes something real when the handler the screen wired is used', async () => {
    renderToStaticMarkup(<Run />);
    const props = seen.at(-1) as EntryProps | undefined;
    expect(props, 'the screen handed the entry row no props at all').toBeDefined();
    if (props === undefined) return;

    expect(
      workspaces.getState().tabs,
      'before the handler runs there is no folder open, otherwise the assertion below could ' +
        'not tell a working control from a dead one',
    ).toEqual([]);

    props.onOpenFolder();
    await new Promise((settle) => setTimeout(settle, 0));

    expect(
      chosen,
      'the handler has to ASK the system for a folder. A row that opens nothing and quietly ' +
        'does nothing is the dead control invariant 16 is about.',
    ).toHaveBeenCalled();
    expect(
      workspaces.getState().tabs.map((tab) => tab.path),
      'after the folder came back the store has to carry it as an open workspace. This is the ' +
        'whole of "the control does something": the state a person can see changed, and it ' +
        'changed through the handler the SCREEN gave the row, not one written in this test.',
    ).toEqual([PICKED]);

    expect(
      renderToStaticMarkup(<Run />),
      'the folder that came back has to reach the screen as a tab; a store that changes and a ' +
        'screen that does not is the same thing as no handler at all, seen from a chair.',
    ).toContain(PICKED);
  });

  it('says all of that without one word from the jargon table', () => {
    const row = screenEntry(renderToStaticMarkup(<Run />));
    const words = bannedWords();
    expect(
      words.length,
      'no terms were read out of the DESIGN §8 table, so "no jargon" would be a statement ' +
        'about an empty list.',
    ).toBeGreaterThan(10);

    const hint = /data-entry-hint[^>]*>([\s\S]*?)<\/p>/.exec(row)?.[1] ?? '';
    const said = (placeholderIn(row) + ' ' + hint).toLowerCase();
    expect(said.trim(), 'there is no visible text in the entry row to judge').not.toBe('');

    for (const word of words) {
      expect(
        new RegExp('\\b' + word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '\\b').test(said),
        'the entry row says ' +
          JSON.stringify(word) +
          ', and DESIGN §8 puts that word in the left column — the one we never write. It says: ' +
          JSON.stringify(said),
      ).toBe(false);
    }
  });
});
