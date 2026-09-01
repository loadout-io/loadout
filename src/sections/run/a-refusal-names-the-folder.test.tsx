/* Odmowa „coś już tu idzie" NAZYWA FOLDER — i człowiek czyta tę nazwę na ekranie biegu, a nie
 * w wartości zwróconej przez funkcję (niezmiennik 29).
 *
 * CO BYŁO ZEPSUTE. Zapadka „jeden bieg naraz" była globalna, więc bieg w jednym folderze odmawiał
 * pracy w KAŻDYM innym — i zdanie brzmiało tak samo: „A run is already going". Człowiek czytał, że
 * zajęty jest cały Loadout, szedł nacisnąć Stop na ekranie, na którym nic nie idzie, i dostawał
 * „nic nie biegnie". Odkąd zapadka jest kluczowana folderem, zdanie bez nazwy folderu jest wprost
 * nieprawdziwe.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(await launchRun(…)).toBe(zdanie)`. Przechodzi także wtedy,
 * kiedy człowiek nie widzi ani słowa — funkcja żyje, ekran milczy. Mierzymy więc markup
 * prawdziwego ekranu biegu, w kolumnie, w której `sayWhatDidNotStart` naprawdę stawia to zdanie.
 *
 * ZDANIA NIE MA W TYM PLIKU JAKO LITERAŁU i to jest połowa jego wartości — ta sama zasada, co
 * w `skills-refusal-is-visible.test.tsx`. Szablon czytamy z `src-tauri/src/ipc.rs` w tym samym
 * przebiegu: druga kopia jednego zdania jest zawsze tą nieaktualną (niezmiennik 23), a tutaj byłaby
 * dodatkowo kopią przez granicę. Dzięki temu szablon, który przestanie nazywać folder, przewraca
 * ten plik po stronie okna — a nie tylko po stronie Rusta.
 *
 * CO ZNACZY TU „KLIKNIĘCIE". To repo nie ma jsdom, więc wołamy to, co woła przycisk: prawdziwe
 * `launchRun` przy granicy odrzucającej `run_workflow` tak, jak odrzuca Rust (napisem — powód
 * w `src/ipc/why.ts`), a potem ten sam jeden ruch, który robi `start.tsx` po jej powrocie: oddanie
 * zdania kanałowi `onSaid`, który ekran SAM podał swojej kontrolce startu.
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby test padał na asercji o treści,
 * nigdy na otwarciu pliku (AGENTS.md §2a p. 5).
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { Choice } from './choices';
import type { StartProps } from './start';

/* Granica odrzucająca bieg tak, jak odrzuca go Rust: NAPISEM, nie `Error`-em. Zdanie wchodzi tu
 * dopiero po odczytaniu szablonu, więc jedzie przez uchwyt, a nie przez domknięcie nad stałą. */
const { invoked, refusal } = vi.hoisted(() => {
  const refusal = { sentence: '' };
  return {
    refusal,
    invoked: vi.fn((command: string) =>
      command === 'run_workflow' ? Promise.reject(refusal.sentence) : Promise.resolve(undefined),
    ),
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

/* Przelotka, nie atrapa: prawdziwa kontrolka startu dalej się rysuje, a test widzi wyłącznie to,
 * z czym ekran ją zawołał. Ten sam zabieg stoi w `skills-refusal-is-visible.test.tsx`. */
const { seen } = vi.hoisted(() => ({ seen: [] as unknown[] }));

vi.mock('./start', async (importOriginal) => {
  const real = await importOriginal<typeof import('./start')>();
  return {
    ...real,
    Start: (props: StartProps) => {
      seen.push(props);
      return real.Start(props);
    },
  };
});

const Run = (await import('./index')).default;
const { launchRun } = await import('./launch');
const { useWorkspaces } = await import('../../state/workspaces');

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const IPC = resolve(ROOT, 'src-tauri/src/ipc.rs');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Ciało literału stojącego po tej deklaracji, aż do średnika kończącego ją. */
function literalAfter(source: string, declaration: string): string {
  const at = source.indexOf(declaration);
  if (at < 0) return '';
  const tail = source.slice(at + declaration.length);
  const closes = tail.indexOf(';');
  return closes < 0 ? '' : tail.slice(0, closes).trim();
}

/**
 * Napis z takiego literału, złożony tak, jak złoży go kompilator.
 *
 * Dwie rzeczy do zdjęcia i obie zmieniają treść: `\` na końcu linii skleja ją z następną razem
 * z jej wcięciem, a `\"` w środku jest cudzysłowem, który człowiek naprawdę zobaczy.
 */
function rustText(literal: string): string {
  const joined = literal.replace(/\\\r?\n\s*/g, '').trim();
  const quoted = /^"((?:[^"\\]|\\.)*)"/.exec(joined);
  return (quoted?.[1] ?? '').replace(/\\"/g, '"');
}

const TEMPLATE = rustText(literalAfter(fileText(IPC), 'const ALREADY_GOING: &str ='));

/** Nazwa folderu w przełączniku — jedno słowo, bo tak nazywa je człowiek. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };

/** Zdanie, którym Rust odmawia — złożone z JEGO szablonu, nie napisane tutaj. */
const REFUSAL = TEMPLATE.replace('{name}', HERE.name);

refusal.sentence = REFUSAL;

/** Workflow, o który człowiek poprosił drugi raz. */
const CHOICE: Choice = {
  path: 'ship.json',
  name: 'Ship it',
  steps: [{ id: 's_only', name: 'Only step', state: 'pending' }],
};

/** Zdanie zapasowe wołającego. Jeśli wróci ono, precyzyjna odmowa zginęła na granicy. */
const GENERIC = 'Loadout could not start that run.';

useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });

/* Ekran PRZED odmową. Model widoku żyje na poziomie modułu (bieg trwa dłużej niż ekran), więc
 * ekran, który jeszcze o niczym nie mówi, da się zobaczyć tylko raz i tylko tutaj. */
const beforeMarkup = renderToStaticMarkup(<Run />);

/** Kanał, którym ekran odbiera zdanie o tym, czego nie udało się zacząć. */
const channel = (seen.at(-1) as StartProps | undefined)?.onSaid;

const said = await launchRun(CHOICE, 2);

/* Dokładnie to, co robi kontrolka startu w `start.tsx` po powrocie `launchRun`. */
if (typeof channel === 'function') {
  channel(said);
}

const afterMarkup = renderToStaticMarkup(<Run />);

/** Markup tak, jak czyta go człowiek: React zapisuje cudzysłowy i `&` jako encje. */
function readable(markup: string): string {
  return markup
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
}

/**
 * Sama kolumna strumienia, wycięta z ekranu — reszta ekranu nie ma prawa tu odpowiadać.
 *
 * TAM WŁAŚNIE LĄDUJE TO ZDANIE, a nie w slocie pod paskiem: `sayWhatDidNotStart` w `./index.tsx`
 * oddaje je strumieniowi, bo model widoku żyje na poziomie modułu i przeżywa wyjście do innej
 * sekcji, a `useState` ekranu nie.
 */
function streamOf(markup: string): string {
  const opens = markup.indexOf('data-stream-column');
  const closes = markup.indexOf('data-plan-column');
  if (opens < 0 || closes < opens) return '';
  return readable(markup.slice(opens, closes));
}

describe('a refused start names the folder that is busy, on the screen', () => {
  it('runs on the wording the refusal is really made of', () => {
    expect(
      TEMPLATE,
      'nothing was read out of the refusal wording in src-tauri/src/ipc.rs, so every comparison ' +
        'below would run between two empty strings and pass on nothing. Either the file moved, ' +
        'or that refusal stopped carrying the sentence it is made of.',
    ).not.toBe('');
    expect(
      TEMPLATE.includes('{name}'),
      'the wording Loadout turns a second start down with has no place for the name of the ' +
        'folder that is busy, so it says the same thing whichever folder is working. That ' +
        'sentence sends a person to press Stop on a screen where nothing is going, which is ' +
        'exactly the round trip this whole change is here to end. It reads: ' +
        TEMPLATE,
    ).toBe(true);
    expect(
      REFUSAL.includes(HERE.name),
      'the finished sentence does not carry the name that stands in the side menu, so the ' +
        'person reading it cannot tell which of their folders to go back to. It says: ' +
        REFUSAL,
    ).toBe(true);
    expect(
      TEMPLATE.toLowerCase().includes('folder'),
      'the wording says a run is going without saying that Loadout leads one run at a time in ' +
        'EACH folder, so it still reads as though the whole of Loadout were busy. It reads: ' +
        TEMPLATE,
    ).toBe(true);

    expect(
      said,
      'a start turned down because that folder is busy has to come back with the sentence Rust ' +
        'wrote, word for word. If it came back as "' +
        GENERIC +
        '", the precise refusal died at the boundary and no screen can show what it never ' +
        'received. It came back with: ' +
        JSON.stringify(said),
    ).toBe(REFUSAL);
  });

  it('says nothing about it before anything is turned down', () => {
    expect(
      streamOf(beforeMarkup),
      'the run screen rendered nothing to read at all, so the check below would pass on an ' +
        'empty string rather than on a screen.',
    ).not.toBe('');
    expect(
      streamOf(beforeMarkup),
      'the sentence stood on the screen before anything was turned down, so nothing below could ' +
        'tell a screen that answers a refused start from one that says it always.',
    ).not.toContain(REFUSAL);
  });

  it('leaves that sentence, with the folder named in it, on the screen', () => {
    expect(
      typeof channel,
      'the run screen hands its start control nowhere to put the sentence about what could not ' +
        'be started, so a refused start has no way of reaching the screen at all.',
    ).toBe('function');

    const answer = streamOf(afterMarkup);
    expect(
      answer,
      'the run screen rendered nothing to read after the refusal either, so the assertion below ' +
        'would be about an empty string.',
    ).not.toBe('');
    expect(
      answer,
      'the start was turned down because a run is already going in one folder, and the sentence ' +
        'on the screen does not say which one. A person reads that the whole of Loadout is ' +
        'busy, goes to press Stop where nothing is working, and is told nothing is going — the ' +
        'same round trip, one folder away. The sentence that had to be there: ' +
        REFUSAL,
    ).toContain(REFUSAL);
    /* Osobno od porównania wyżej, i to nie jest powtórzenie: tamto przechodzi dla KAŻDEGO
     * zdania, jakie Rust napisze — także dla starego, które nie nazywało nic. To pyta o jedną
     * rzecz, dla której cała ta droga istnieje. */
    expect(
      answer,
      'the sentence reached the screen and does not carry the name the side menu gives that ' +
        'folder, so a person reading it still cannot tell where the work is going on. It has ' +
        'to name: ' +
        HERE.name,
    ).toContain(HERE.name);
  });
});
