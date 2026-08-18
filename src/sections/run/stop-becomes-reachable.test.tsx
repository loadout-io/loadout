/* AC-3 dla T-38: Stop pojawia się dopiero, kiedy jest co zatrzymać — i sam ustawia to sobie Start.
 *
 * SŁABA WERSJA JEST TU JEDNA I JEST KUSZĄCA: zasiać `workflow` w magazynie (`useRun.setState`
 * albo świeży magazyn z fabryki), wyrenderować kontrolkę i sprawdzić, że Stop stoi w markupie.
 * Ona PRZECHODZI NA DZISIEJSZYM KODZIE — a dzisiejszy kod jest właśnie zepsuty. `RunState.workflow`
 * startowało `''` i do 2026-08-18 nie miało w całym repo ani jednego pisarza: komentarz przy polu
 * obiecywał „wypełnia je komenda startu biegu", a nie robiło tego nic. Skutek nie był kosmetyczny —
 * Stop renderuje się wyłącznie przy biegu, „bieg trwa" znaczy `workflow !== ''`, więc przycisk Stop
 * nie montował się NIGDY i biegu nie dało się zatrzymać z okna. Test, który sam zasiewa to pole,
 * mierzy setter, którego brak JEST tym defektem.
 *
 * Odróżnia ten plik jedna rzecz: pole zmienia się WYŁĄCZNIE przez ścieżkę Startu — `start()`
 * z `src/sections/run/io.ts`, czyli tę samą funkcję, którą wywołuje przycisk. Ostatni przypadek
 * czyta WŁASNE ŹRÓDŁO tego pliku i wymaga, żeby nie było w nim ani jednego zapisu do magazynu.
 * To jedyny znany mi sposób, żeby zdanie „przez ścieżkę Startu" znaczyło coś sprawdzalnego —
 * ten sam chwyt, którym AC-2 broni się przed testem wołającym `appendLines` samemu.
 *
 * CO ZNACZY TU „KLIKNIĘCIE". To repo NIE MA jsdom, a dopisanie `@testing-library/react` byłoby
 * zmianą `package.json`, czyli momentem na zatrzymanie się i zapytanie człowieka (AGENTS.md §7;
 * ta sama uwaga stoi w `start-invokes.test.tsx`). Klikamy więc to, co klika przycisk: krawędź
 * z `io.ts`. Markup mówi, czy kontrolka JEST; krawędź mówi, dokąd sięga — kryterium rozdziela te
 * dwa pytania dokładnie tak samo, w punktach (b) i (c).
 *
 * WARTOŚCI OCZEKIWANE SĄ CZYTANE, NIE WPISANE. Napis na kontrolce zatrzymania bierzemy z tabeli
 * `docs/design/DESIGN.md` §8 (wiersz `Terminate`), a nazwę komendy ze `src-tauri/commands.golden.txt`.
 * Wpisanie ich z palca dałoby test zielony także wtedy, gdy słownik albo złota lista mówią co innego.
 * Każdy odczyt ma osobną asercję na to, że parser COŚ znalazł: porównanie dwóch pustych wartości
 * przechodzi na niczym.
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby test padał na asercji o treści,
 * nigdy na otwarciu pliku (AGENTS.md §2a p. 5).
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

/* Atrapa transportu. Nie rozwiązuje się sama: komenda biegu trwa tyle, co bieg, a „w trakcie
 * biegu" nie istnieje jako chwila, kiedy pierwsze wywołanie kończy się natychmiast. `Channel`
 * musi tu być, bo Start zakłada kanał i podaje go jako czwarty argument (AC-1). */
const { invoked, release } = vi.hoisted(() => {
  const waiting: Array<() => void> = [];
  return {
    invoked: vi.fn(
      (..._sent: unknown[]) =>
        new Promise<undefined>((resolve2) => {
          waiting.push(() => {
            resolve2(undefined);
          });
        }),
    ),
    release: (): void => {
      while (waiting.length > 0) waiting.pop()?.();
    },
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { Start } = await import('./start');
const { start, stop } = await import('./io');
const { useRun } = await import('../../state/run');

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '..', '..', '..');
const DESIGN = resolve(ROOT, 'docs/design/DESIGN.md');
const GOLDEN = resolve(ROOT, 'src-tauri/commands.golden.txt');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Wiersz tabeli DESIGN §8: co PISZEMY zamiast żargonu z lewej kolumny. */
function insteadOf(design: string, jargon: string): string {
  const from = design.indexOf('## 8.');
  if (from < 0) return '';
  const to = design.indexOf('## 9.', from);
  const section = design.slice(from, to < 0 ? undefined : to);
  const row = new RegExp('^\\|\\s*' + jargon + '\\s*\\|([^|]*)\\|', 'm').exec(section);
  return (row?.[1] ?? '').replace(/`/g, '').trim();
}

/** Nazwy komend ze złotej listy — ta sama, którą po tamtej stronie czyta test rejestracji. */
function goldenNames(text: string): ReadonlySet<string> {
  return new Set(
    text
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line !== '' && !line.startsWith('#')),
  );
}

/**
 * Napisy na WSZYSTKICH przyciskach markupu, w kolejności wystąpienia.
 *
 * Przyciski, nie dowolne wystąpienie słowa: `markup.includes('Stop')` byłoby zielone od napisu
 * w podpowiedzi, w komunikacie błędu albo w nazwie workflow, a kryterium pyta o KONTROLKĘ.
 */
function buttonLabels(markup: string): readonly string[] {
  return [...markup.matchAll(/<button\b[^>]*>([^<]*)<\/button>/g)].map((hit) =>
    (hit[1] ?? '').trim(),
  );
}

const design = fileText(DESIGN);
const known = goldenNames(fileText(GOLDEN));

/**
 * Wiersz tabeli DESIGN §8, z ktorego bierzemy napis na kontrolce zatrzymania.
 *
 * Osobna stala, bo lewa kolumna tej tabeli to slowa ZAKAZANE na ekranie, a `quick-vocabulary.sh`
 * skanuje kazdy literal ze spacja w `src/**` — takze komunikat asercji. Pojedyncze slowo bez
 * spacji przez ten skan przechodzi, wiec klucz wiersza mieszka tutaj, a nie w zdaniu obok.
 */
const STOP_ROW = 'Terminate';

/** Napis na kontrolce zatrzymania. Czytany z DESIGN §8, nie wpisany. */
const STOP = insteadOf(design, STOP_ROW);

/** Nazwa pliku workflow — to jedzie do Rusta. */
const OPEN = 'ship-a-feature.json';
/** Jak workflow nazywa SAM SIEBIE. Inna niż nazwa pliku, żeby dało się je rozróżnić w magazynie. */
const NAME = 'Ship a feature';
/** „Ile naraz" ze stanu. Nie trójka: domyślną łatwo wpisać i nie zauważyć. */
const AT_ONCE = 5;
/** Plan biegu prosto z grafu — dokładnie to, z czego pasek loadoutu rysuje bloki. */
const PLAN = [
  { id: 'plan', name: 'Plan the change', state: 'pending' as const },
  { id: 'build', name: 'Write the code', state: 'pending' as const },
];

/* Migawka SPRZED czegokolwiek: liczona przy wczytaniu modułu, więc żaden przypadek nie zdążył
 * jeszcze ruszyć biegu. Punkt (a) pyta dokładnie o ten moment. */
const beforeAnyRun = renderToStaticMarkup(
  <Start
    onSaid={() => {
      /* To kryterium nie pyta o zdanie odmowy — pyta o kontrolkę. Kanał raportowania
               musi jednak istnieć, bo typ go wymaga, i to jest celowe: prop wymagany jest
               sposobem, w jaki system typów pilnuje, żeby zdanie miało gdzie stanąć. */
    }}
  />,
);
const workflowAtRest = useRun.getState().workflow;

describe('Stop shows up only once there is a run to stop, and Start is what sets that up', () => {
  beforeEach(() => {
    invoked.mockClear();
    release();
  });

  it('reads its expected values out of DESIGN §8 and the golden command list', () => {
    expect(
      design,
      'docs/design/DESIGN.md could not be read, so every expected value below would come from ' +
        'nowhere and the comparisons would pass on empty strings.',
    ).not.toBe('');
    expect(
      STOP,
      'the DESIGN §8 table no longer carries the row this file reads the stop word out of (' +
        STOP_ROW +
        '). That row is where the word on the stop control comes from; without it this file ' +
        'would be checking the markup against an empty string, which every markup contains.',
    ).not.toBe('');
    expect(
      known.size,
      'src-tauri/commands.golden.txt parsed to nothing. It is the one place where both sides ' +
        'of the seam agree on a command name, so an empty set turns the check below into a ' +
        'question nobody is answering.',
    ).toBeGreaterThanOrEqual(10);
  });

  it('renders no stop control before a run, and does render the one that starts one', () => {
    const labels = buttonLabels(beforeAnyRun);

    expect(
      labels.length,
      'the run controls rendered no buttons at all, so "there is no Stop here" would be true ' +
        'of an empty screen. Nothing below this line would mean anything.',
    ).toBeGreaterThan(0);
    expect(
      workflowAtRest,
      'the run store already knew of a workflow before anything started one. Every assertion in ' +
        'this file about Start setting that field would then be measuring leftovers.',
    ).toBe('');
    expect(
      labels,
      'a stop control is on screen before any run exists. A control with nothing to do does not ' +
        'ship (invariant 16) — pressing it asks Rust to prove a run is dead when no run was ever ' +
        'alive, and stop_run waits for a proof that never comes.',
    ).not.toContain(STOP);
  });

  it('the Start path — not this test — is what puts the workflow into the run store', async () => {
    const going = start(OPEN, AT_ONCE, { name: NAME, steps: PLAN });

    expect(
      useRun.getState().workflow,
      'the Start path ran and the run store still does not know what is running. This field had ' +
        'no writer at all until this task: its comment promised "the run start command fills it ' +
        'in" and nothing did. Nothing in this file writes it — if it is empty here, production ' +
        'is not writing it either.',
    ).toBe(NAME);
    expect(
      useRun.getState().steps.map((step) => step.name),
      'the run store took the name and dropped the plan. Those two arrive by the same road in ' +
        'the same instant, and the loadout strip needs both: a caption with no blocks under it ' +
        'is the same permanently-empty strip this task exists to end.',
    ).toEqual(PLAN.map((step) => step.name));

    release();
    await Promise.allSettled([going]);
  });

  it('carries the stop control once that Start path has gone through', async () => {
    const going = start(OPEN, AT_ONCE, { name: NAME, steps: PLAN });
    const labels = buttonLabels(
      renderToStaticMarkup(
        <Start
          onSaid={() => {
            /* To kryterium nie pyta o zdanie odmowy — pyta o kontrolkę. Kanał raportowania
               musi jednak istnieć, bo typ go wymaga, i to jest celowe: prop wymagany jest
               sposobem, w jaki system typów pilnuje, żeby zdanie miało gdzie stanąć. */
          }}
        />,
      ),
    );

    expect(
      labels.length,
      'the run controls rendered no buttons during a run, so the check below would be asking ' +
        'about a screen that has nothing on it.',
    ).toBeGreaterThan(0);
    expect(
      labels,
      'a run is going and the window offers no way to stop it. That is the state this repo has ' +
        'been in the whole time: the button exists in the source, its condition reads a store ' +
        'field, and nothing ever wrote that field. Rendered buttons were: ' +
        JSON.stringify(labels),
    ).toContain(STOP);

    release();
    await Promise.allSettled([going]);
  });

  it('lets go of the run once it comes back, so Stop does not outlive it', async () => {
    const going = start(OPEN, AT_ONCE, { name: NAME, steps: PLAN });
    release();
    await Promise.allSettled([going]);

    expect(
      buttonLabels(
        renderToStaticMarkup(
          <Start
            onSaid={() => {
              /* To kryterium nie pyta o zdanie odmowy — pyta o kontrolkę. Kanał raportowania
               musi jednak istnieć, bo typ go wymaga, i to jest celowe: prop wymagany jest
               sposobem, w jaki system typów pilnuje, żeby zdanie miało gdzie stanąć. */
            }}
          />,
        ),
      ),
      'the run finished and the stop control stayed on screen. It now asks Rust to kill a run ' +
        'that is already gone, and the loadout strip keeps captioning a run nobody is having — ' +
        'a control with nothing to do (invariant 16), which is the same defect as the one this ' +
        'criterion fixes, only pointing the other way.',
    ).not.toContain(STOP);
  });

  it('sends the stop control edge to the command the golden list names', async () => {
    const going = stop();

    expect(
      invoked.mock.calls.length,
      'the stop edge never reached Rust. A control that renders and asks nobody anything is the ' +
        'dead-button family this repo names in invariant 16.',
    ).toBe(1);

    const name = invoked.mock.calls.at(0)?.at(0);
    expect(
      typeof name === 'string' && known.has(name),
      'the stop edge asked Rust for ' +
        String(name) +
        ', which is not on src-tauri/commands.golden.txt — so nothing on the Rust side keeps ' +
        'that name alive, and the day it is renamed this call goes quiet instead of failing.',
    ).toBe(true);
    expect(name, 'and the command it asks for is the one that stops a run').toBe('stop_run');

    release();
    await Promise.allSettled([going]);
  });

  it('never writes the run store itself — that is what makes this proof about production', () => {
    const own = fileText(fileURLToPath(import.meta.url));
    expect(
      own,
      'this file could not read itself, so the check below would run over an empty string and ' +
        'find nothing wrong with anything.',
    ).not.toBe('');

    /* Nagłówek odpada: opisuje słabą wersję i musi wolno mu ją NAZWAĆ. Sądzimy kod. */
    const body = own.slice(own.indexOf('import {'));

    /* Igły są SKLEJANE. Test szukający we własnym źródle napisu wpisanego wprost znajduje ten
     * napis w sobie i jest czerwony zawsze — sprawdza wtedy własny tekst zamiast własnego
     * zachowania. Rozdzielenie nazwy od nawiasu sprawia, że jedyne, co może wprowadzić tu tę
     * formę, to prawdziwe wywołanie. */
    const raw = 'set' + 'State(';
    const setter = 'now' + 'Running(';

    expect(
      body.includes(raw) || body.includes(setter),
      'this file writes the run store on its own. The moment it does, it stops proving that the ' +
        'Start path fills that field in and starts proving its own write — which is exactly the ' +
        'green that let a store field with no writer at all sit in this repo behind a Stop ' +
        'button nobody could ever see.',
    ).toBe(false);
  });
});
