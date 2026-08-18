/* AC-4 dla T-38: bieg zaparkowany na punkcie kontrolnym da się puścić dalej Z OKNA.
 *
 * DLACZEGO TO KRYTERIUM ISTNIEJE. `continue_run` jest po stronie Rusta zarejestrowana i stoi na
 * `src-tauri/commands.golden.txt`, a `rg 'continue_run|continueRun' src/` do 2026-08-18 nie
 * zwracało nic. Kafelek punktu kontrolnego zatrzymuje przy tym CAŁY bieg, nie sam krok
 * (`commands::run::wait_for_a_person`), a warianty odpowiedzi są dziś zawsze puste
 * (`options: Vec::new()`), więc w widoku pracy nie renderował się ani jeden przycisk. Workflow
 * z punktem kontrolnym parkował więc na zawsze i z okna wyglądał dokładnie jak zawieszony agent.
 *
 * SŁABA WERSJA: `expect(io.continueRun).toBeTypeOf('function')`. Przechodzi na krawędzi, której
 * nic nie renderuje — czyli na tym samym rodzaju martwego mechanizmu, którym był `wireChannel`
 * przed tym zadaniem: zdefiniowany, poprawny, bez ani jednego wołającego. Odróżnia ten plik to,
 * że stan „bieg stoi na punkcie kontrolnym" powstaje TU PRZEZ KOD PRODUKCYJNY: przechwytujemy
 * kanał, który Start podał Rustowi, i oddajemy przez niego wiersz `asked` — dokładnie tak, jak
 * zrobiłby to `commands::run::ask`. Wszystko pomiędzy kanałem a markupem jest produkcją.
 *
 * NAPIS NA KONTROLCE NIE JEST WPISANY, TYLKO WYPROWADZONY. Bierzemy RÓŻNICĘ zbiorów napisów na
 * przyciskach sprzed i po zaparkowaniu biegu: kontrolka „dalej" to dokładnie to, co przybyło.
 * Wersja szukająca w markupie z góry umówionego słowa przechodziłaby także wtedy, gdy to słowo
 * stoi tam z zupełnie innego powodu, i nie umiałaby zobaczyć, że nic nowego się nie pojawiło.
 * Napis sądzimy potem tabelą z `docs/design/DESIGN.md` §8, czytaną w tym samym biegu testu —
 * lista zakazanych słów wpisana tutaj z palca rozjechałaby się z tabelą po pierwszej zmianie
 * i przestałaby cokolwiek egzekwować (niezmiennik 14).
 *
 * CO ZNACZY TU „KLIKNIĘCIE". To repo NIE MA jsdom, a dopisanie `@testing-library/react` byłoby
 * zmianą `package.json`, czyli momentem na zapytanie człowieka (AGENTS.md §7). Markup mówi, czy
 * kontrolka JEST; krawędź z `io.ts` mówi, dokąd sięga — kryterium rozdziela te dwa pytania tak
 * samo, w punktach (a)/(b).
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby test padał na asercji o treści,
 * nigdy na otwarciu pliku (AGENTS.md §2a p. 5). Każdy odczyt ma osobną asercję na to, że parser
 * COŚ znalazł: porównanie z pustą listą przechodzi na niczym.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

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
const { start, continueRun } = await import('./io');

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '..', '..', '..');
const DESIGN = resolve(ROOT, 'docs/design/DESIGN.md');
const GOLDEN = resolve(ROOT, 'src-tauri/commands.golden.txt');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Ciało tabeli DESIGN §8 — wiążącej dla języka interfejsu (decyzja D5). */
function designSection8(design: string): string {
  const from = design.indexOf('## 8.');
  if (from < 0) return '';
  const to = design.indexOf('## 9.', from);
  return design.slice(from, to < 0 ? undefined : to);
}

/**
 * Napis, który DESIGN §8 każe pisać zamiast czegoś — znaleziony po SAMYM NAPISIE, nie po
 * zakazanym słowie.
 *
 * 2026-08-18 — POWSTAŁO Z KONIECZNOŚCI I JEST LEPSZE OD TEGO, CO ZASTĄPIŁO. Stała tu funkcja
 * szukająca wiersza po LEWEJ kolumnie tabeli, czyli po zakazanym słowie. Żeby ją zawołać,
 * plik musiał to słowo ZACYTOWAĆ — a `checks/quick-vocabulary.sh` sądzi każdy plik
 * zmieniony wobec `main`, więc przy pierwszym dotknięciu tego pliku dwa takie cytaty stały się
 * czerwienią bramki. Osłabienie sprawdzenia (allowlist w `checks/`) albo sklejenie napisu
 * z kawałków byłoby oszukaniem kryterium, które ma rację: żargon nie ma prawa stać w tekście,
 * który człowiek czyta, a komunikat asercji jest takim tekstem.
 *
 * Pytanie zostaje to samo — „czy tabela nadal każe pisać `Stop`" — tylko zadane od drugiej
 * strony: szukamy wiersza, którego PRAWA kolumna jest dokładnie tym napisem. Nie jest to
 * słabsze: pusty wynik nadal znaczy „tabela tego nie mówi" i nadal wywraca kryterium niżej.
 * Jest za to ciaśniejsze, bo wymaga zgodności co do znaku, a nie samego istnienia wiersza.
 */
function labelInSection8(design: string, label: string): string {
  for (const row of designSection8(design).split('\n')) {
    const cells = row.split('|');
    if (cells.length < 4) continue;
    const right = (cells[2] ?? '').replace(/`/g, '').trim();
    if (right === label) return right;
  }
  return '';
}

/**
 * Lewa kolumna tabeli §8, rozbita na pojedyncze terminy.
 *
 * To są słowa, których na ekranie być nie może. Czytamy je, zamiast wpisywać: lista wpisana tu
 * z palca zostaje taka, jaka jest, kiedy tabela rośnie — i wtedy nowy zakazany termin przechodzi.
 */
function bannedOnScreen(design: string): readonly string[] {
  const out: string[] = [];
  for (const row of designSection8(design).split('\n')) {
    const cells = row.split('|');
    if (cells.length < 4) continue;
    const left = (cells[1] ?? '').trim();
    if (left === '' || left === 'Zamiast' || /^[-: ]+$/.test(left)) continue;
    for (const piece of left.split(/[/,]/)) {
      const term = piece.trim().toLowerCase();
      if (term.length >= 3) out.push(term);
    }
  }
  return out;
}

function goldenNames(text: string): ReadonlySet<string> {
  return new Set(
    text
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line !== '' && !line.startsWith('#')),
  );
}

/** Napisy na WSZYSTKICH przyciskach markupu — kontrolki, nie dowolne wystąpienie słowa. */
function buttonLabels(markup: string): readonly string[] {
  return [...markup.matchAll(/<button\b[^>]*>([^<]*)<\/button>/g)].map((hit) =>
    (hit[1] ?? '').trim(),
  );
}

/** Sam tekst, który człowiek czyta. Bez znaczników, więc bez klas CSS i atrybutów `data-*`. */
function visibleText(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Kształt kanału, którym Rust oddaje paczki. Tyle, ile ten test dotyka. */
interface Port {
  onmessage: ((batch: unknown) => void) | null;
}

/** Kanał, który Start podał Rustowi jako czwarty argument `run_workflow`. */
function portFromStart(): Port {
  const args = invoked.mock.calls.at(0)?.at(1);
  const carried =
    typeof args === 'object' && args !== null ? (args as Record<string, unknown>) : {};
  const port = carried['lines'];
  if (port === null || typeof port !== 'object') {
    throw new Error('Start did not hand Rust a channel under `lines`');
  }
  return port as Port;
}

const design = fileText(DESIGN);
const known = goldenNames(fileText(GOLDEN));
/* Sygnatura komendy, czytana z tego samego drzewa w tym samym biegu. Powód przy asercji
 * o zbiorze argumentów niżej. */
const rust = fileText(resolve(ROOT, 'src-tauri/src/ipc.rs'));
const banned = bannedOnScreen(design);
/** Napis kontrolki zatrzymania, czytany z §8 — bieg stojący na pytaniu dalej ma dać się zatrzymać. */
const STOP = labelInSection8(design, 'Stop');

const OPEN = 'ship-a-feature.json';
const NAME = 'Ship a feature';
const AT_ONCE = 5;

/**
 * Wiersz, którym Rust ogłasza punkt kontrolny.
 *
 * Kształt jest kształtem `Line::Asked` z `src-tauri/src/engine/line.rs`: `agent`, `text`
 * i `options`, ani pola więcej — `parseLine` odrzuca wiersz z nadmiarowym kluczem. Lista opcji
 * jest pusta, bo `commands::run::ask` wysyła dziś dokładnie `Vec::new()`, i to jest właśnie ten
 * stan, w którym widok pracy nie renderuje ani jednego przycisku odpowiedzi.
 */
const CHECKPOINT = {
  kind: 'asked',
  agent: 'Does the plan look right?',
  text: 'Does the plan look right?',
  options: [] as string[],
};

/* ── Cała ścieżka produkcyjna, przejściem raz, przy wczytaniu modułu ──────────────────────────
 *
 * Trzy migawki w trzech różnych chwilach biegu, i tylko środek między nimi jest tym, o co pyta
 * kryterium. Nic tu nie dotyka magazynu ani modelu widoku wprost: jedyne, co ten plik robi, to
 * uruchamia Start i oddaje paczkę przez kanał, który Start sam założył. */
const beforeAnyRun = renderToStaticMarkup(
  <Start
    onSaid={() => {
      /* To kryterium nie pyta o zdanie odmowy — pyta o kontrolkę. Kanał raportowania
               musi jednak istnieć, bo typ go wymaga, i to jest celowe: prop wymagany jest
               sposobem, w jaki system typów pilnuje, żeby zdanie miało gdzie stanąć. */
    }}
  />,
);
const going = start(OPEN, AT_ONCE, { name: NAME, steps: [] });
const port = portFromStart();
const whileRunning = renderToStaticMarkup(
  <Start
    onSaid={() => {
      /* To kryterium nie pyta o zdanie odmowy — pyta o kontrolkę. Kanał raportowania
               musi jednak istnieć, bo typ go wymaga, i to jest celowe: prop wymagany jest
               sposobem, w jaki system typów pilnuje, żeby zdanie miało gdzie stanąć. */
    }}
  />,
);
if (port.onmessage === null) {
  throw new Error('nothing is listening on the channel Start opened');
}
port.onmessage([CHECKPOINT]);
const atCheckpoint = renderToStaticMarkup(
  <Start
    onSaid={() => {
      /* To kryterium nie pyta o zdanie odmowy — pyta o kontrolkę. Kanał raportowania
               musi jednak istnieć, bo typ go wymaga, i to jest celowe: prop wymagany jest
               sposobem, w jaki system typów pilnuje, żeby zdanie miało gdzie stanąć. */
    }}
  />,
);

/** Kontrolki, które przybyły dokładnie na zaparkowaniu biegu. */
const appeared = buttonLabels(atCheckpoint).filter(
  (label) => !buttonLabels(whileRunning).includes(label),
);

/**
 * Napis kontrolki „dalej" — WYPROWADZONY z różnicy, nie umówiony z góry.
 *
 * Pusty, kiedy nic nie przybyło. Wtedy czerwony jest przypadek liczący tę różnicę, a nie ten,
 * który sprawdza napis: różnica dwóch identycznych ekranów nie jest pytaniem o słownictwo.
 */
const carryOn = appeared.at(0) ?? '';

/* Parser listy parametrów komendy Rusta — ta sama trójka funkcji, co w
 * `start-args-complete.test.tsx`. Kopia, nie import, i to jest zapisany dług: dwa pliki
 * kryteriów nie mogą importować się wzajemnie bez zrobienia z jednego z nich biblioteki, a
 * biblioteka kryteriów jest miejscem, w którym osłabienie asercji przestaje być widoczne
 * w diffie tego kryterium. Zgłoszone: gdyby doszedł trzeci wołający, ta trójka należy do
 * wspólnego pliku pomocniczego.
 *
 * Plik czytamy przez `existsSync ? readFileSync : ''`, żeby test padał na asercji o treści,
 * nigdy na otwarciu pliku (AGENTS.md §2a p. 5). */
function signature(rust: string, fn: string): string {
  const at = rust.indexOf(`fn ${fn}(`);
  if (at < 0) return '';
  const from = rust.indexOf('(', at);
  let depth = 0;
  for (let i = from; i < rust.length; i += 1) {
    const ch = rust[i];
    if (ch === '(') depth += 1;
    else if (ch === ')') {
      depth -= 1;
      if (depth === 0) return rust.slice(from + 1, i);
    }
  }
  return '';
}

/** Dzieli listę parametrów po przecinkach NA POZIOMIE ZERO — `State<'_, AppState>` ma własny. */
function parameters(inside: string): readonly string[] {
  const out: string[] = [];
  let depth = 0;
  let current = '';
  for (const ch of inside) {
    if (ch === '<' || ch === '(' || ch === '[') depth += 1;
    else if (ch === '>' || ch === ')' || ch === ']') depth -= 1;
    if (ch === ',' && depth === 0) {
      out.push(current);
      current = '';
    } else current += ch;
  }
  out.push(current);
  return out.map((one) => one.trim()).filter((one) => one !== '');
}

function camel(snake: string): string {
  return snake.replace(/_([a-z])/g, (_all, letter: string) => letter.toUpperCase());
}

/** Nazwy argumentów, które ma wysłać OKNO. Odpada tylko to, co Tauri wstrzykuje samo. */
function windowSideArguments(rust: string, fn: string): readonly string[] {
  return parameters(signature(rust, fn))
    .filter((one) => !/:\s*State\s*</.test(one))
    .map((one) => camel(one.split(':')[0]?.trim() ?? ''))
    .filter((name) => name !== '');
}

describe('a run parked at a checkpoint can be let through from the window', () => {
  it('reads its expected values out of DESIGN §8 and the golden command list', () => {
    expect(
      design,
      'docs/design/DESIGN.md could not be read, so the vocabulary judgement below would run ' +
        'against an empty list and every label on earth would pass it.',
    ).not.toBe('');
    expect(
      banned.length,
      'nothing was parsed out of the DESIGN §8 table. That table is what invariant 14 is ' +
        'enforced against; an empty list turns the label check into a formality that says yes ' +
        'to anything.',
    ).toBeGreaterThanOrEqual(10);
    expect(
      STOP,
      'the DESIGN §8 table no longer carries the word this control has to show, so the check ' +
        'that a parked run is still stoppable would be comparing against an empty string.',
    ).not.toBe('');
    expect(
      known.size,
      'src-tauri/commands.golden.txt parsed to nothing, so "under a name from the golden list" ' +
        'would be a question with no list behind it.',
    ).toBeGreaterThanOrEqual(10);
  });

  it('offers nothing to continue before there is any run at all', () => {
    const idle = buttonLabels(beforeAnyRun);

    expect(
      idle.length,
      'the run controls rendered no buttons at all on an idle screen, so "no continue control ' +
        'here" would be true of an empty screen and would mean nothing.',
    ).toBeGreaterThan(0);
    expect(
      carryOn,
      'no control ever showed up when the run parked, so there is nothing whose absence this ' +
        'case could be checking. The case below says the same thing louder.',
    ).not.toBe('');
    expect(
      idle,
      'the control that lets a run carry on is on screen before any run exists. Pressing it ' +
        'there is not harmless: continue_run bumps a go-ahead counter on the Rust side, so the ' +
        'press spends the answer to the NEXT checkpoint, which then flies past without ever ' +
        'asking. A control with nothing to do does not ship (invariant 16). Buttons were: ' +
        JSON.stringify(idle),
    ).not.toContain(carryOn);
  });

  it('grows exactly one control the moment the run stands at a checkpoint', () => {
    expect(
      appeared,
      'the run parked at a checkpoint and the window offered nothing new. This is the state the ' +
        'task describes: the checkpoint stops the whole run, its answer options are always empty ' +
        'today, so the work view draws no buttons either — the run waits forever and reads as a ' +
        'hung agent. Buttons before: ' +
        JSON.stringify(buttonLabels(whileRunning)) +
        ', after: ' +
        JSON.stringify(buttonLabels(atCheckpoint)),
    ).toHaveLength(1);
    expect(
      buttonLabels(atCheckpoint),
      'the continue control replaced the stop control instead of standing next to it. A run ' +
        'waiting on a person is still a run: taking away the only way to end it leaves the ' +
        'person with one door out of two, and the other one costs real money.',
    ).toContain(STOP);
  });

  it('names that control in plain English, with nothing off the DESIGN §8 table on it', () => {
    const label = carryOn;

    expect(
      label,
      'the control that appeared carries no text at all. A button with no word on it answers ' +
        'nothing about what pressing it does.',
    ).not.toBe('');
    expect(
      /^[A-Za-z][A-Za-z ]*$/.test(label),
      'the label on the continue control is not plain English words: ' +
        JSON.stringify(label) +
        '. The UI is English (decision D5) and a wire enum never reaches a screen ' +
        '(invariant 14).',
    ).toBe(true);
    expect(
      banned.filter((term) => label.toLowerCase().includes(term)),
      'the label on the continue control carries jargon the DESIGN §8 table forbids. That table ' +
        'is read from the file in this run, so it stays true as it grows. Label was: ' +
        JSON.stringify(label),
    ).toEqual([]);

    const words = visibleText(atCheckpoint).toLowerCase();
    expect(
      words.includes('continue_run'),
      'the name of a Rust command reached the screen. A wire name is never user-visible text ' +
        '(invariant 14); the person reading it learns the shape of our IPC and nothing about ' +
        'their run. Visible text was: ' +
        visibleText(atCheckpoint),
    ).toBe(false);
    expect(
      words.includes('checkpoint'),
      'the word "checkpoint" reached the screen. It is our word for a tile in the editor, not ' +
        'a word this person chose — what they see here is a question waiting on them. Visible ' +
        'text was: ' +
        visibleText(atCheckpoint),
    ).toBe(false);
  });

  it('sends that control edge to the command the golden list names', async () => {
    invoked.mockClear();
    const asked = continueRun();

    expect(
      invoked.mock.calls.length,
      'the continue edge never reached Rust. A control that renders and asks nobody anything is ' +
        'the dead-button family invariant 16 names, and it is exactly what continue_run has been ' +
        'since it was registered: on the golden list, with zero callers in src/.',
    ).toBe(1);

    const name = invoked.mock.calls.at(0)?.at(0);
    expect(
      typeof name === 'string' && known.has(name),
      'the continue edge asked Rust for ' +
        String(name) +
        ', which is not on src-tauri/commands.golden.txt — so nothing on the Rust side keeps ' +
        'that name alive, and the day it is renamed this call goes quiet instead of failing.',
    ).toBe(true);
    expect(name, 'and the command it asks for is the one that lets a parked run carry on').toBe(
      'continue_run',
    );

    /* ZBIÓR ARGUMENTÓW CZYTAMY Z SYGNATURY, NIE Z PAMIĘCI.
     *
     * 2026-08-18 — stało tu `Object.keys(args).length === 0` z uzasadnieniem „continue_run bierze
     * po stronie Rusta wyłącznie wstrzyknięty stan". To było prawdą do tego dnia i przestało nią
     * być: komenda bierze teraz `answer`, bo treść odpowiedzi człowieka nie dochodziła NIGDZIE —
     * pytanie znikało z ekranu, bieg ruszał, a zdanie ginęło. Kryterium przepisane z palca
     * zapaliło się na POPRAWNEJ zmianie i nie umiało powiedzieć, że jest nieaktualne, a nie złe.
     *
     * Wersja niżej czyta listę parametrów z `src-tauri/src/ipc.rs` w tym samym biegu testu —
     * ten sam wzorzec, którym `start-args-complete.test.tsx` pilnuje Startu. Piąty argument
     * dołożony po tamtej stronie zapala ten test SAM, bez niczyjej pamięci, i nie da się go
     * przejść ani wysyłając za dużo, ani za mało.
     *
     * Punkt (c) nie jest ozdobą: parser, który cicho nic nie dopasuje, oddaje pustą listę,
     * a wtedy porównanie przechodzi na dwóch pustych zbiorach — czyli dokładnie ten kształt
     * zieleni, który to kryterium ma kasować. */
    const wanted = windowSideArguments(rust, 'continue_run');
    expect(
      wanted.length,
      'the continue_run signature could not be parsed out of ipc.rs, so the expected set below ' +
        'would come from nowhere and the comparison would pass on two empty sets.',
    ).toBeGreaterThanOrEqual(1);

    const args = invoked.mock.calls.at(0)?.at(1);
    const carried =
      typeof args === 'object' && args !== null ? Object.keys(args as object).sort() : [];
    expect(
      carried,
      'the continue edge and continue_run disagree about the argument set. Tauri matches ' +
        'arguments BY NAME and deserializes them BEFORE the body runs, so a missing key is not ' +
        'a smaller call — it is a rejected one, and the person reads only "Loadout could not let ' +
        'that run carry on". Expected set is read from ipc.rs in this run. It sent: ' +
        JSON.stringify(args),
    ).toEqual([...wanted].sort());

    release();
    await Promise.allSettled([asked, going]);
  });
});
