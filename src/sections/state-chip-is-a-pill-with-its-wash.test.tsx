import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';
import type { Import, InstalledSkill } from '../state/skills';
import { useSkills } from '../state/skills';
import type { Note } from '../state/memory';
import { useMemory } from '../state/memory';
import MemoryScreen from './memory/index';
import SkillsScreen from './skills/index';

/* AC-2 dla T-48: chip stanu jest pigulka z wypelnieniem swojego stanu, i nic nie miesza dwoch
 * stanow.
 *
 * CHIPA POZNAJE SIE PO KSZTALCIE, NIE PO NAZWIE KLASY. Barwe stanu niosa w tych sekcjach trzy
 * rozne rzeczy i tylko jedna z nich jest chipem:
 *
 *   chip                pelny obrys stanu  +  wypelnienie stanu   -> pigulka
 *   przycisk niebezpieczny   obrys stanu   +  BEZ wypelnienia     -> prominencja nalezy do akcentu
 *   pasek bledu              `border-b`    +  wypelnienie stanu   -> obrys jednej krawedzi
 *
 * Dlatego zadanie „bierz pigulke" jest tu postawione WYLACZNIE temu, co jest wypelnione
 * i obwiedzione w calosci. Kryterium, ktore zadaloby pigulki od wszystkiego, co niesie barwe
 * stanu, zabranialoby poprawnego kodu — pasek bledu na calej szerokosci ekranu z zaokraglonymi
 * koncami jest wlasnie tym, czego ten jezyk nie robi.
 *
 * CZYTANE Z ZASIANYCH EKRANOW, bo chipy pojawiaja sie dopiero przy danych: pusta sekcja nie ma
 * ani jednego. Zrodlo przechodzilo tu takze na chipie, ktorego nikt nie montuje.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const STATES = ['live', 'fail', 'attend', 'accent', 'human'] as const;

const WAITING: Note = {
  place: 'project',
  id: 'n-1',
  title: 'Bundled SQLite has no textbook defaults',
  rule: 'Set foreign_keys and busy_timeout on every bare connection.',
  because: 'Measured on a bare connection: neither is on by default.',
  status: 'suggested',
  scope: 'this-project',
  length: 96,
  occurrences: 2,
  modified: '2026-08-19T09:00:00Z',
};

const PENDING: Import = {
  name: 'exact-diff',
  summary: 'Applies the smallest change and refuses a rewrite.',
  reviewed: {
    body: '# exact-diff\n\nApply the smallest change.\n',
    findings: [],
    verdict: 'clean',
  },
  scripts: 0,
  fromTheInternet: true,
};

const PLACED: InstalledSkill = {
  name: 'exact-diff',
  fromTheInternet: true,
  summary: 'Shows the change as it really is',
};

/** Kazdy element z lista klas, razem z nazwa swojego elementu. */
function elements(markup: string): readonly (readonly [string, string])[] {
  return [...markup.matchAll(/<([a-z][a-z0-9]*)\b[^>]*\sclass="([^"]*)"[^>]*>/g)].map(
    (hit) => [hit[1] ?? '', hit[2] ?? ''] as const,
  );
}

/** Pelny obrys ze wszystkich stron: goly `border`, nie `border-b` ani `border-l`. */
const wholeOutline = (classes: string): boolean => /(?:^|\s)border(?:\s|$)/.test(classes);

const outlineState = (classes: string): string | undefined =>
  STATES.find((one) => new RegExp('border-' + one + '-edge\\b').test(classes));

/* Wypelnienie stanu: wash ALBO LITE tlo.
 *
 * Pierwsza wersja rozpoznawala tylko `bg-{stan}-soft`. Lite `bg-fail` bylo wtedy dla tej wyroczni
 * niewidzialne we WSZYSTKICH trzech punktach — a przycisk `border-fail-edge bg-fail text-bg` jest
 * glosniejszy od wszystkiego, czego to kryterium zabrania, i przechodzil nietkniety. */
const fillState = (classes: string): string | undefined =>
  STATES.find((one) => new RegExp('bg-' + one + '(?:-(?:soft|wash))?(?![\\w-])').test(classes));

describe('chip stanu', () => {
  beforeEach(() => {
    useMemory.setState({ notes: [], passed: [], message: null, passedProblem: null, choice: null });
    useSkills.setState({ installed: [], pending: null });
  });

  /** Oba ekrany, ktore niosa chipy, zasiane i wyrenderowane. */
  function seeded(): string {
    useMemory.setState({ notes: [WAITING] });
    useSkills.setState({ installed: [PLACED], pending: PENDING });
    return (
      renderToStaticMarkup(<MemoryScreen store={useMemory} />) +
      renderToStaticMarkup(<SkillsScreen store={useSkills} />)
    );
  }

  it('read some chips at all', () => {
    const chips = elements(seeded()).filter(
      ([, classes]) => wholeOutline(classes) && outlineState(classes) !== undefined,
    );
    expect(
      chips.length,
      'not one filled and fully outlined state element was read out of the two screens that ' +
        'carry them, so every assertion below would sweep an empty list',
    ).toBeGreaterThan(0);
  });

  it('makes every filled state element a pill', () => {
    const square = elements(seeded()).filter(
      ([, classes]) =>
        wholeOutline(classes) &&
        outlineState(classes) !== undefined &&
        fillState(classes) !== undefined &&
        !/\brounded-pill\b/.test(classes),
    );
    expect(
      square,
      'these state elements are filled and outlined all round, which is what a chip is, and they ' +
        'do not take the pill corner: ' +
        JSON.stringify(square) +
        '. A chip is read at a glance from its silhouette before its colour is read at all.',
    ).toEqual([]);
  });

  it('never mixes one state with another', () => {
    const mixed = elements(seeded()).filter(([, classes]) => {
      const outline = outlineState(classes);
      const fill = fillState(classes);
      return outline !== undefined && fill !== undefined && outline !== fill;
    });
    expect(
      mixed,
      'these elements take the outline of one state and the fill of another: ' +
        JSON.stringify(mixed) +
        '. Two states on one element means the element states two things at once, and a person ' +
        'reads whichever is louder.',
    ).toEqual([]);
  });

  it('keeps fill away from buttons that carry a state', () => {
    const filled = elements(seeded()).filter(
      ([tag, classes]) =>
        tag === 'button' && outlineState(classes) !== undefined && fillState(classes) !== undefined,
    );
    expect(
      filled,
      'these buttons carry a state colour AND a fill: ' +
        JSON.stringify(filled) +
        '. A filled button is the most prominent thing on a screen, and prominence belongs to ' +
        'the one interactive colour — a filled warning competes with the action a person came for.',
    ).toEqual([]);
  });

  it('keeps a quiet chip for the things that are in no state at all', () => {
    const quiet = elements(seeded()).filter(
      ([, classes]) =>
        /\brounded-pill\b/.test(classes) &&
        /\bborder-line(?![\w-])/.test(classes) &&
        /\btext-muted\b/.test(classes),
    );
    expect(
      quiet.length,
      'no quiet chip was read. Not everything a chip says is a state: where a skill came from is ' +
        'a plain fact, and painting it in a state colour would make a fact look like a problem.',
    ).toBeGreaterThan(0);
  });
});

/* ── POLOWA ZRODLOWA ────────────────────────────────────────────────────────────────────────
 *
 * Zasiane ekrany dowodza, ze chipy NAPRAWDE sie montuja — i to jest ich zadanie. Nie dowodza
 * niczego o galeziach, ktorych ta jedna fikstura nie otwiera: karta bledu w Agents, chip
 * w edytorze workflow, pasek bledu w edytorze, przyciski usuwania w trzech sekcjach, wymuszony
 * wybor i wiersz przekazania. Zmierzone: barwe stanu niosa w tych sekcjach elementy z SIEDMIU
 * miejsc, a fikstura montuje dwa z nich.
 *
 * Dlatego dwie reguly, ktore nie potrzebuja wiedziec, jakim elementem jest napis — „wypelniony
 * i obwiedziony w calosci bierze pigulke" oraz „nigdy dwa stany na raz" — sa tu zadane KAZDEMU
 * napisowi klasowemu w czterech sekcjach. Regula o przyciskach zostaje na wyrenderowanym ekranie,
 * bo tam i tylko tam wiadomo, ze to jest przycisk.
 *
 * Wzorzec jest ten sam, co w `src/sections/run/live-and-fail-never-share-a-form.test.ts`: klasy
 * mieszkaja tez w stalych i w mapach, wiec `className=` szukane wprost widzi mniej niz polowe. */
const SECTIONS = ['agents', 'skills', 'memory', 'workflows'] as const;

const withoutRemarks = (source: string): string => source.replace(/\/\*[\s\S]*?\*\//g, ' ');

/** Wszystkie napisy klasowe pliku, ze wstawionymi wartosciami stalych z tego samego pliku. */
function classLiterals(source: string): readonly string[] {
  const constants = new Map<string, string>();
  for (const hit of source.matchAll(/const ([A-Z_][A-Z0-9_]*)\s*=\s*\x27([^\x27]*)\x27/g)) {
    constants.set(hit[1] ?? '', hit[2] ?? '');
  }
  const out: string[] = [];
  /* TRZY SPOSOBY ZAPISU NAPISU, nie jeden. Pierwsza wersja czytala apostrofy i backticki, a JSX
   * pisze `className=\x22...\x22` w cudzyslowach — wiec polowa zrodlowa nie widziala ani jednego
   * chipa wpisanego wprost w element, czyli dokladnie tych, ktorych zadna fikstura nie otwiera.
   * Znaki cudzyslowu sa tu przez `\x22`, bo skaner `checks/quick-vocabulary.sh` liczy je w linii
   * i nieparzysta liczba rozjezdza mu odczyt tekstu uzytkownika na kilkadziesiat linii dalej. */
  for (const hit of source.matchAll(/[\x27\x22`]([^\x27\x22`]*)[\x27\x22`]/g)) {
    const text = hit[1] ?? '';
    if (!/(?:^|\s)(?:bg|border|rounded|text|h|px|py|flex|grid|size)-/.test(text)) continue;
    out.push(
      text.replace(/\$\{([A-Za-z_][\w]*)\}/g, (_, name: string) => constants.get(name) ?? ' '),
    );
  }
  for (const [, value] of constants) {
    if (/(?:^|\s)(?:bg|border|rounded|text)-/.test(value)) out.push(value);
  }
  return out;
}

function sources(): readonly (readonly [string, string])[] {
  const out: [string, string][] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (/\.tsx?$/.test(entry.name) && !/\.test\./.test(entry.name)) {
        out.push([path.slice(ROOT.length + 1), withoutRemarks(readFileSync(path, 'utf8'))]);
      }
    }
  };
  for (const one of SECTIONS) walk(resolve(ROOT, 'src', 'sections', one));
  return out;
}

describe('chip stanu, tym razem w kazdej galezi', () => {
  const written = sources().flatMap(([path, text]) =>
    classLiterals(text).map((classes) => [path, classes] as const),
  );

  it('read the class lists out of all four sections', () => {
    expect(
      written.length,
      'almost no class lists were read out of the four sections, so both rules below would sweep ' +
        'an empty list',
    ).toBeGreaterThan(60);
    expect(
      written.filter(([, classes]) => outlineState(classes) !== undefined).length,
      'not one state outline was read. The four sections carry them in seven places; reading zero ' +
        'means the reader stopped seeing constants and maps again.',
    ).toBeGreaterThan(4);
  });

  it('makes every filled state element a pill, in branches no fixture opens', () => {
    const square = written.filter(
      ([, classes]) =>
        wholeOutline(classes) &&
        outlineState(classes) !== undefined &&
        fillState(classes) !== undefined &&
        !/\brounded-pill\b/.test(classes),
    );
    expect(
      square,
      'these state elements are filled and outlined all round, which is what a chip is, and they ' +
        'do not take the pill corner: ' +
        JSON.stringify(square) +
        '. Most of them are in branches a person reaches on a bad day — which is exactly when ' +
        'the screen has to stay readable.',
    ).toEqual([]);
  });

  it('never mixes one state with another, anywhere in the four', () => {
    const mixed = written.filter(([, classes]) => {
      const outline = outlineState(classes);
      const fill = fillState(classes);
      return outline !== undefined && fill !== undefined && outline !== fill;
    });
    expect(
      mixed,
      'these places take the outline of one state and the fill of another: ' +
        JSON.stringify(mixed) +
        '. Two states on one element means it states two things at once, and a person reads ' +
        'whichever is louder.',
    ).toEqual([]);
  });
});
