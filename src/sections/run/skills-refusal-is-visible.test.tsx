/* AC-5 dla T-79: zdanie odmowy o umiejętności, której krok nie mógł dostać, stoi w PRAWDZIWYM
 * markupie strumienia biegu — a nie tylko w wartości, którą oddała funkcja (niezmiennik 29).
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(await launchRun(…)).toBe(zdanie)`. Przechodzi DZIŚ i
 * przechodziłaby przez cały czas, w którym człowiek nie widzi ani słowa — bo `launchRun` oddaje
 * zdanie wołającemu, kontrolka startu wkłada je w `useState` ekranu (`data-screen-said`), a stan
 * renderu ginie razem z komponentem. Wyjście do Agentów i powrót zostawia bieg, który się nie
 * zaczął, i ekran, który o tym milczy. To jest dokładnie ta klasa, którą niezmiennik 29 nazywa
 * wprost: funkcja żyje, ekran nie mówi nic.
 *
 * DRUGA SŁABA WERSJA JEST GORSZA, BO WYGLĄDA NA MOCNĄ: przepuścić tę odmowę drogą `/run` z wiersza
 * wejścia. Ta droga jest podpięta od 2026-08-20 (`entry/entry.tsx` woła `onShowInStream(saidOf(…))`),
 * więc kryterium o niej byłoby zielone od pierwszej minuty i nie mierzyłoby ani jednej nowej linii.
 * Mierzymy drogę przycisku Start, czyli tę, która kończy się dziś w jednym akapicie pod paskiem.
 *
 * ZDANIA NIE MA W TYM PLIKU JAKO LITERAŁU i to jest połowa jego wartości — ta sama zasada, którą
 * zapisano w `src-tauri/tests/it/skills_missing_stops_the_run.rs`. Szablon czytamy z atrybutu
 * `#[error(…)]` przy `skills::Missing` w tym samym biegu testu: druga kopia jednego zdania jest
 * zawsze tą nieaktualną (niezmiennik 23), a tutaj byłaby dodatkowo kopią przez granicę. Kontrola
 * przeciw pustemu porównaniu stoi w pierwszym `it`: parser, który cicho nic nie dopasował, dałby
 * puste napisy i wszystko niżej przechodziłoby na niczym.
 *
 * CO ZNACZY TU „KLIKNIĘCIE". To repo nie ma jsdom, więc wołamy to, co woła przycisk: prawdziwe
 * `launchRun` przy granicy odrzucającej `run_workflow` tak, jak odrzuca Rust (napisem — powód
 * w `src/ipc/why.ts`), a potem ten sam jeden ruch, który robi `start.tsx` po jej powrocie:
 * oddanie zdania kanałowi `onSaid`, który ekran SAM podał swojej kontrolce startu. Wszystko
 * pomiędzy jest kodem produkcyjnym. Kryterium nie rozstrzyga, w którym z tych dwóch miejsc
 * powstaje wiersz strumienia — zielone jest i wtedy, gdy zakłada go polityka startu, i wtedy,
 * gdy robi to ekran po odebraniu zdania.
 *
 * MARKUP ODESCAPOWUJEMY, bo React zapisuje cudzysłów jako `&quot;`, a `Missing` bierze w cudzysłów
 * i nazwę kroku, i nazwę umiejętności. Porównanie na surowym markupie nie mogłoby przejść nigdy,
 * czyli byłoby kryterium niespełnialnym, a nie kryterium.
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
 * z czym ekran ją zawołał. Ten sam zabieg stoi w `paused-banner-mounts.test.tsx`. */
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
const SKILLS = resolve(ROOT, 'src-tauri/src/skills/mod.rs');

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
const TEMPLATE = errorAttributeBefore(rust, 'pub struct Missing');
const SENTENCE = rustText(TEMPLATE);
const WHY = rustText(errorAttributeBefore(rust, 'NotInTheLibrary,'));

/** Nazwa kroku — ta z kafelka, bo to jej szuka człowiek na płótnie. */
const STEP = 'Only step';
/** Umiejętność, której nie ma w bibliotece. Ta sama nazwa, co po drugiej stronie granicy. */
const SKILL = 'nowhere';

/** Zdanie, którym Rust odmawia — złożone z JEGO szablonu, nie napisane tutaj. */
const REFUSAL = SENTENCE.replace('{step}', STEP).replace('{skill}', SKILL).replace('{why}', WHY);

refusal.sentence = REFUSAL;

/** Zakres, w którym pracujemy. `id === folder` — kontrakt granicy z 2026-08-18. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };

/** Workflow z jednym krokiem — tym, który nosi nazwę stojącą w odmowie. */
const CHOICE: Choice = {
  path: 'ship.json',
  name: 'Ship it',
  steps: [{ id: 's_only', name: STEP, state: 'pending' }],
};

/** Zdanie zapasowe wołającego. Jeśli wróci ono, precyzyjna odmowa zginęła na granicy. */
const GENERIC = 'Loadout could not start that run.';

useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });

/* Ekran PRZED odmową. Model widoku żyje na poziomie modułu (bieg trwa dłużej niż ekran), więc
 * pusty strumień da się zobaczyć tylko raz i tylko tutaj. */
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

/** Sama kolumna strumienia, wycięta z ekranu — reszta ekranu nie ma prawa tu odpowiadać. */
function streamOf(markup: string): string {
  const opens = markup.indexOf('data-stream-column');
  const closes = markup.indexOf('data-plan-column');
  if (opens < 0 || closes < opens) return '';
  return readable(markup.slice(opens, closes));
}

describe('a refused run leaves the sentence about the missing skill in its stream', () => {
  it('runs on the sentence the refused run really produced', () => {
    expect(
      SENTENCE,
      'nothing was read out of the refusal wording in src-tauri/src/skills/mod.rs, so every ' +
        'comparison below would run between two empty strings and pass on nothing. Either the ' +
        'file moved, or that refusal stopped carrying the sentence it is made of.',
    ).not.toBe('');
    expect(
      SENTENCE.includes('{step}') && SENTENCE.includes('{skill}'),
      'the wording read out of Rust names neither the step nor the skill, so the sentence this ' +
        'file hands the screen could not prove anything about either name. It reads: ' +
        SENTENCE,
    ).toBe(true);
    expect(
      WHY,
      'nothing was read out of the reason Rust gives for a name its library never saw, so the ' +
        'sentence would carry an empty clause exactly where the cause belongs.',
    ).not.toBe('');

    expect(
      said,
      'a run refused because a skill cannot reach the step has to come back with the sentence ' +
        'Rust wrote, word for word. If it came back as "' +
        GENERIC +
        '", the precise refusal died at the boundary and no screen can show what it never ' +
        'received. It came back with: ' +
        JSON.stringify(said),
    ).toBe(REFUSAL);
    expect(
      REFUSAL.includes(SKILL) && REFUSAL.includes(STEP),
      'the sentence has to name both the skill and the step: without the skill a refusal turns ' +
        'one tick box into a search through a list, and without the step a person does not know ' +
        'which tile to open. It says: ' +
        REFUSAL,
    ).toBe(true);
  });

  it('shows nothing about it before the run is refused', () => {
    expect(
      streamOf(beforeMarkup),
      'the run screen rendered no stream at all, so the check below would pass on an empty ' +
        'string rather than on a screen.',
    ).not.toBe('');
    expect(
      streamOf(beforeMarkup),
      'the sentence stood on the screen before anything was refused, so nothing below could ' +
        'tell a screen that answers a refused run from one that says it always.',
    ).not.toContain(REFUSAL);
  });

  it('leaves that sentence in the stream, word for word', () => {
    expect(
      typeof channel,
      'the run screen hands its start control nowhere to put the sentence about what could not ' +
        'be started, so a refused run has no way of reaching the screen at all.',
    ).toBe('function');

    const stream = streamOf(afterMarkup);
    expect(
      stream,
      'the run screen rendered no stream after the refusal either, so the assertion below would ' +
        'be about an empty string.',
    ).not.toBe('');
    expect(
      stream,
      'the run was refused because a skill it needed could not reach the step, and the sentence ' +
        'naming that skill and that step is nowhere in the stream. A person reads a run that ' +
        'never started and no reason for it — and "the agent was never given that skill" looks ' +
        'from outside exactly like "the model did not reach for it". The sentence that had to ' +
        'be there: ' +
        REFUSAL,
    ).toContain(REFUSAL);
  });
});
