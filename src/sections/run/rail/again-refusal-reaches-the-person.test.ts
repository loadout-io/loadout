/* T-154: kiedy powtórzenie kroku zostaje odrzucone, człowiek CZYTA zdanie, którym je odrzucono —
 * nie patrzy na przycisk, który nic nie zrobił (niezmiennik 29).
 *
 * ZMIERZONY BRAK. `runStepAgain` ma dwie gałęzie i tylko jedna jest sądzona: ścieżkę `resolve`
 * (zdanie o zmienionym pliku) mierzy `../session/run-again-is-reachable.test.tsx`, a gałąź
 * `.catch` nie ma ani jednego kryterium. To ona odbiera KAŻDĄ odmowę po tamtej stronie granicy —
 * łącznie z tą, dla której powstało to zadanie: materiał, po który tamten bieg sięgnął, przestał
 * być tym samym. Odmowa, która nie ma gdzie wylądować, jest ciszą, a cisza po naciśnięciu wygląda
 * dokładnie jak przycisk bez handlera (niezmiennik 16).
 *
 * SŁABĄ WERSJĄ TEGO KRYTERIUM jest `expect(rerunStep(…)).rejects`. Przechodzi ją dokładnie ten
 * defekt, o który tu chodzi: krawędź oddaje odmowę wołającemu, a wołający ją porzuca. Pytamy więc
 * o strumień, czyli o to, co człowiek naprawdę czyta.
 *
 * DRUGĄ: „coś tam wylądowało". Przechodzi ją zdanie zastępcze, złożone tutaj po drodze — a odmowa
 * bez nazwy umiejętności zamienia jedno przywrócenie pliku w przeszukiwanie biblioteki, i bez
 * nazwy kroku nie mówi, który kafelek otworzyć. Sądzimy więc CAŁE zdanie, co do słowa.
 *
 * ZDANIA NIE MA W TYM PLIKU JAKO LITERAŁU i to jest połowa jego wartości — ta sama zasada i ten
 * sam sposób, co w `../skills-refusal-is-visible.test.tsx`. Szablon czytamy z atrybutu
 * `#[error(…)]` przy `skills::NotAsItWas` w tym samym biegu testu: druga kopia jednego zdania
 * jest zawsze tą nieaktualną (niezmiennik 23), a tutaj byłaby dodatkowo kopią przez granicę.
 * Kontrola przeciw pustemu porównaniu stoi w pierwszym `it`.
 *
 * GRANICA ODRZUCA NAPISEM, nie `Error`-em, i to nie jest szczegół fikstury — tak odrzuca Rust
 * (powód w `src/ipc/why.ts`). Implementacja czytająca wyłącznie `error.message` zostawiłaby
 * człowieka z `[object Object]` w miejscu, w którym stało zdanie.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, vi } from 'vitest';

/* Atrapą jest wyłącznie transport. Krawędź (`../io.ts`) jest prawdziwa, więc literówka w nazwie
 * komendy albo w kluczu argumentu przewraca ten plik. Zdanie wchodzi tu dopiero po odczytaniu
 * szablonu, więc jedzie przez uchwyt, a nie przez domknięcie nad stałą. */
const { invoked, refused } = vi.hoisted(() => {
  const refused = { sentence: '' };
  return {
    refused,
    invoked: vi.fn((command: string) =>
      command === 'rerun_step' ? Promise.reject(refused.sentence) : Promise.resolve(null),
    ),
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { sayAfterRunningAgain } = await import('./rail');
const { runStepAgain } = await import('./again');
const { runFeed } = await import('../feed/live');
const { useRun } = await import('../../../state/run');

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..', '..');
const SKILLS = resolve(ROOT, 'src-tauri/src/skills/mod.rs');

/** Czytamy tak, żeby test padał na asercji o treści, nigdy na otwarciu pliku. */
function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Ciało atrybutu `#[error(…)]` stojącego bezpośrednio przed tą deklaracją. */
function errorAttributeBefore(source: string, declaration: string): string {
  const at = source.indexOf(declaration);
  if (at < 0) return '';
  const head = source.slice(0, at);
  const opens = head.lastIndexOf('#[error(');
  const closes = head.lastIndexOf(')]');
  if (opens < 0 || closes < opens) return '';
  return head.slice(opens + '#[error('.length, closes);
}

/**
 * Napis z takiego atrybutu, złożony tak, jak złoży go kompilator.
 *
 * Dwie rzeczy do zdjęcia i obie zmieniają treść: `\` na końcu linii skleja ją z następną razem
 * z jej wcięciem, a `\"` w środku jest cudzysłowem, który człowiek naprawdę zobaczy.
 */
function rustText(attribute: string): string {
  const joined = attribute.replace(/\\\r?\n\s*/g, '').trim();
  const quoted = /^"((?:[^"\\]|\\.)*)"/.exec(joined);
  return (quoted?.[1] ?? '').replace(/\\"/g, '"');
}

const rust = fileText(SKILLS);
const TEMPLATE = rustText(errorAttributeBefore(rust, 'pub struct NotAsItWas'));
/** Powód: materiał leżący dziś pod tą nazwą to nie ten, który tamten bieg dostał. */
const WHY = rustText(errorAttributeBefore(rust, 'Changed,'));

/** Nazwa kroku — ta z kafelka, bo to jej szuka człowiek na płótnie. */
const STEP = 'Only step';
/** Umiejętność, którą tamten bieg zamroził. */
const SKILL = 'alpha';
/** Klucz kroku, który przycisk każe powtórzyć. */
const TILE = 's_only';

const HERE = '/Users/x/ledger-ui';
const FILE = 'ship-a-feature.json';

/** Zdanie, którym Rust odmawia — złożone z JEGO szablonu, nie napisane tutaj. */
const REFUSAL = TEMPLATE.replace('{step}', STEP).replace('{skill}', SKILL).replace('{why}', WHY);

refused.sentence = REFUSAL;

useRun.setState({ folder: HERE, fileName: FILE });

/** Strumień PRZED naciśnięciem — pusty w tej sprawie i tylko tutaj da się to zobaczyć. */
const before = runFeed.view.history.filter((row) => row.label.includes(REFUSAL)).length;

/* To samo, co robi przycisk: polityka powtórzenia i ten sam kanał na odpowiedź, który okno samo
 * podaje ekranowi agenta. Wszystko pomiędzy jest kodem produkcyjnym. */
runStepAgain(TILE, sayAfterRunningAgain);
await new Promise<void>((done) => {
  setTimeout(done, 0);
});

const landed = runFeed.view.history.filter((row) => row.label.includes(REFUSAL));

describe('a refused repeat leaves its sentence where the person is already reading', () => {
  it('runs on the wording the refusal is really made of', () => {
    expect(
      TEMPLATE,
      'nothing was read out of the refusal wording in src-tauri/src/skills/mod.rs, so every ' +
        'comparison below would run between two empty strings and pass on nothing. Either the ' +
        'file moved, or that refusal stopped carrying the sentence it is made of.',
    ).not.toBe('');
    expect(
      TEMPLATE.includes('{step}') && TEMPLATE.includes('{skill}') && TEMPLATE.includes('{why}'),
      'the wording read out of Rust names neither the step, nor the skill, nor the cause, so the ' +
        'sentence this file hands the screen could not prove anything about any of them. It ' +
        'reads: ' +
        TEMPLATE,
    ).toBe(true);
    expect(
      WHY,
      'nothing was read out of the reason Rust gives when the material under that name is no ' +
        'longer the one the first run was given, so the sentence would carry an empty clause ' +
        'exactly where the cause belongs.',
    ).not.toBe('');
    expect(
      before,
      'the sentence stood in the stream before anything was refused, so nothing below could tell ' +
        'a screen that answers a refused repeat from one that says it always.',
    ).toBe(0);
  });

  it('reaches the other side at all', () => {
    expect(
      invoked.mock.calls.filter((call) => call[0] === 'rerun_step').length,
      'pressing it reached nothing at all, so the refusal below would be about a button that ' +
        'never asked for anything.',
    ).toBe(1);
  });

  it('puts the refusal in the stream, word for word', () => {
    expect(
      landed.length,
      'the repeat was refused and the answer went nowhere. The material this run was given moved ' +
        'under it, Loadout said so in one sentence, and the person reads a button that did ' +
        'nothing - which looks exactly like a repeat that quietly worked. The sentence that had ' +
        'to be there: ' +
        REFUSAL,
    ).toBe(1);
    expect(
      landed[0]?.label.includes(SKILL) === true && landed[0]?.label.includes(STEP) === true,
      'the sentence that landed names neither the skill nor the step. Without the skill a ' +
        'refusal turns one restored file into a search through a library, and without the step a ' +
        'person does not know which tile to open. It says: ' +
        String(landed[0]?.label),
    ).toBe(true);
  });
});
