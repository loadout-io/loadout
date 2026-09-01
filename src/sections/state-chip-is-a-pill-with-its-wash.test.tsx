import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';
import type { Import, InstalledSkill } from '../state/skills';
import { useSkills } from '../state/skills';
import type { Note } from '../state/memory';
import { useMemory } from '../state/memory';
import NotesShelf from './memory/shelf';
import SkillsShelf from './skills/shelf';

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
 *
 * ── DRUGI NOSNIK WYGLADU. 2026-08-31 ─────────────────────────────────────────────────────
 *
 * Do dzis caly wyglad chipa stal w atrybucie `class` w miejscu uzycia
 * (`rounded-pill border border-attend-edge bg-attend-soft`), wiec dalo sie go przeczytac
 * z samego napisu. Przebudowa UI przeniosla go do arkusza: dzis pisze sie
 * `<span class="chip" data-tone="attend">`, a pigulke, obrys i wypelnienie rozstrzyga
 * `@layer components` w `src/styles/theme.css`.
 *
 * Pytanie zostaje TO SAMO — „czy rzecz wypelniona i obwiedziona w calosci barwa stanu jest
 * pigulka" — tylko przestaje wierzyc, ze odpowiedz mieszka w napisie klasowym. Czytamy oba
 * nosniki: klase narzedziowa w miejscu uzycia ORAZ regule domu z arkusza, wybrana przez nazwe
 * klasy i `data-tone`. To jest ta sama naprawa, ktora dostal juz
 * `radii-band-reaches-the-sections.test.tsx`, i jest OSTRZEJSZA, nie slabsza: `rounded-pill`
 * wpisane z reki wystarczalo samo z siebie, a nazwa `chip` musi miec pokrycie w arkuszu —
 * regule, ktora naprawde deklaruje `--radius-pill`.
 *
 * Bez tej polowy wyrocznia nie byla czerwona za wade, tylko slepa: po przebudowie czytala
 * z obu zasianych ekranow ZERO elementow stanu, wiec trzy z pieciu regul zamiatalyby pusta
 * liste, a dwie pozostale zglaszaly brak czegos, co stoi na ekranie.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const STATES = ['live', 'fail', 'attend', 'accent', 'human'] as const;
type State = (typeof STATES)[number];

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

/* Cztery fakty o wygladzie, ktore rozstrzygaja o kazdej regule nizej. Zadne z pol nie jest
 * opcjonalne, tylko jawnie „albo stan, albo nic": `exactOptionalPropertyTypes` w bramce typow
 * odroznia brak pola od pola o wartosci pustej, a tutaj to jest jedna i ta sama odpowiedz. */
interface Look {
  outline: boolean;
  outlineState: State | undefined;
  fill: State | undefined;
  pill: boolean;
}

const SHEET = readFileSync(resolve(ROOT, 'src', 'styles', 'theme.css'), 'utf8');

const edgeIn = (value: string): State | undefined =>
  STATES.find((one) => new RegExp('var\\(--color-' + one + '-edge\\)').test(value));

/* Wypelnienie stanu w arkuszu: `--{stan}-soft` albo goly `--{stan}`. Nawias zamykajacy jest
 * czescia wzorca, inaczej `--color-accent-ring` i `--color-accent-hover` czytalyby sie jak
 * wypelnienie akcentem — a pierwszy jest pierscieniem skupienia, drugi barwa najechania. */
const surfaceIn = (value: string): State | undefined =>
  STATES.find((one) => new RegExp('var\\(--color-' + one + '(?:-soft|-wash)?\\)').test(value));

interface Patch {
  readonly key: string;
  readonly look: Partial<Look>;
}

/* Arkusz przeczytany na regule na raz, W KOLEJNOSCI PLIKU — bo w tej kolejnosci CSS je stosuje.
 * Klucz to `nazwa` albo `nazwa|ton`, wiec `.chip` i `.chip[data-tone="attend"]` skladaja sie
 * dokladnie tak, jak sklada je przegladarka: baza daje pigulke i obrys, ton podmienia barwy.
 *
 * SELEKTORY ZE STANEM KONTROLKI SA POMINIETE i to jest decyzja, nie uproszczenie.
 * `.btn-danger:hover` dokłada wypelnienie `--fail-soft`, ale wypelnienie na najechaniu nie jest
 * tym, o co pyta to kryterium — pyta o wyglad SPOCZYNKOWY, ten, ktory czlowiek widzi, zanim
 * czegokolwiek dotknie. Selektor z dwukropkiem odpada wiec razem z calym swoim cialem. */
const HOUSE: readonly Patch[] = (() => {
  const out: Patch[] = [];
  const text = SHEET.replace(/\/\*[\s\S]*?\*\//g, ' ');
  for (const rule of text.matchAll(/([^{}]+)\{([^{}]+)\}/g)) {
    const body = rule[2] ?? '';
    const declared = (property: string): string | undefined =>
      new RegExp('(?:^|;)\\s*' + property + '\\s*:([^;]*)').exec(body)?.[1];

    const look: Partial<Look> = {};
    let says = false;
    const border = declared('border');
    if (border !== undefined) {
      look.outline = !/\bnone\b/.test(border);
      look.outlineState = edgeIn(border);
      says = true;
    }
    const edge = declared('border-color');
    if (edge !== undefined) {
      look.outlineState = edgeIn(edge);
      says = true;
    }
    const surface = declared('background');
    if (surface !== undefined) {
      look.fill = surfaceIn(surface);
      says = true;
    }
    const corner = declared('border-radius');
    if (corner !== undefined) {
      look.pill = /--radius-pill/.test(corner);
      says = true;
    }
    if (!says) continue;

    /* Znak cudzyslowu jest tu przez `\x22`, bo tak stoi w tym pliku od poczatku: skaner slownictwa
     * liczy cudzyslowy w linii i nieparzysta liczba rozjezdza mu odczyt tekstu uzytkownika. */
    for (const one of (rule[1] ?? '').split(',')) {
      const hit = /^\.([a-z][a-z0-9-]*)(?:\[data-tone=\x22([a-z]+)\x22\])?$/.exec(one.trim());
      if (hit === null) continue;
      const name = hit[1] ?? '';
      const tone = hit[2];
      out.push({ key: tone === undefined ? name : name + '|' + tone, look });
    }
  }
  return out;
})();

function houseLook(classes: string, tone: string | undefined): Look {
  const wanted = new Set<string>();
  for (const name of classes.split(/\s+/)) {
    if (name.length === 0) continue;
    wanted.add(name);
    if (tone !== undefined) wanted.add(name + '|' + tone);
  }
  const out: Look = { outline: false, outlineState: undefined, fill: undefined, pill: false };
  for (const patch of HOUSE) {
    if (!wanted.has(patch.key)) continue;
    if (patch.look.outline !== undefined) out.outline = patch.look.outline;
    if ('outlineState' in patch.look) out.outlineState = patch.look.outlineState;
    if ('fill' in patch.look) out.fill = patch.look.fill;
    if (patch.look.pill !== undefined) out.pill = patch.look.pill;
  }
  return out;
}

/** Pelny obrys ze wszystkich stron: goly `border`, nie `border-b` ani `border-l`. */
const wholeOutline = (classes: string): boolean => /(?:^|\s)border(?:\s|$)/.test(classes);

const outlineState = (classes: string): State | undefined =>
  STATES.find((one) => new RegExp('border-' + one + '-edge\\b').test(classes));

/* Wypelnienie stanu: wash ALBO LITE tlo.
 *
 * Pierwsza wersja rozpoznawala tylko `bg-{stan}-soft`. Lite `bg-fail` bylo wtedy dla tej wyroczni
 * niewidzialne we WSZYSTKICH trzech punktach — a przycisk `border-fail-edge bg-fail text-bg` jest
 * glosniejszy od wszystkiego, czego to kryterium zabrania, i przechodzil nietkniety. */
const fillState = (classes: string): State | undefined =>
  STATES.find((one) => new RegExp('bg-' + one + '(?:-(?:soft|wash))?(?![\\w-])').test(classes));

/* Oba nosniki zlozone w jeden odczyt. Klasa narzedziowa bije regule domu, bo warstwa `utilities`
 * bije warstwe `components` — to jest kolejnosc Tailwinda, nie wybor tego pliku. */
function look(classes: string, tone: string | undefined): Look {
  const house = houseLook(classes, tone);
  return {
    outline: wholeOutline(classes) || house.outline,
    outlineState: outlineState(classes) ?? house.outlineState,
    fill: fillState(classes) ?? house.fill,
    pill: /\brounded-pill\b/.test(classes) || house.pill,
  };
}

/** Element z markupu: nazwa, lista klas, wybrany ton. */
type Seen = readonly [string, string, string | undefined];

function elements(markup: string): readonly Seen[] {
  return [...markup.matchAll(/<([a-z][a-z0-9]*)\b([^>]*)>/g)].flatMap((hit): Seen[] => {
    const attributes = hit[2] ?? '';
    const classes = /(?:^|\s)class=\x22([^\x22]*)\x22/.exec(attributes)?.[1];
    if (classes === undefined) return [];
    return [[hit[1] ?? '', classes, /(?:^|\s)data-tone=\x22([^\x22]*)\x22/.exec(attributes)?.[1]]];
  });
}

describe('chip stanu', () => {
  beforeEach(() => {
    useMemory.setState({ notes: [], passed: [], message: null, passedProblem: null, choice: null });
    useSkills.setState({ installed: [], pending: null });
  });

  /* Zbior pusty jest tu AWARIA, nie zieloną. Odkad wyglad chipa mieszka w arkuszu, kazda regula
   * ponizej opiera sie o to, ze arkusz naprawde maluje pigulke stanu. Gdyby ktos skasowal
   * `.chip[data-tone="…"]`, chipy na ekranach straciłyby barwe po cichu, a wyrocznia dalej
   * swiecilaby na zielono — na pustej liscie. */
  it('the sheet has to paint at least one filled state pill', () => {
    const pills = [...new Set(HOUSE.map((patch) => patch.key))]
      .filter((key) => key.includes('|'))
      .filter((key) => {
        const [name = '', tone = ''] = key.split('|');
        const one = houseLook(name, tone);
        return one.outline && one.outlineState !== undefined && one.fill !== undefined && one.pill;
      });
    expect(
      pills,
      'no rule in theme.css fills a state, outlines it all round and gives it the pill corner, ' +
        'so every rule below would judge these screens against an empty sheet — the chip could ' +
        'lose its shape in one line of the sheet and nothing here would say so.',
    ).not.toEqual([]);
  });

  /** Oba ekrany, ktore niosa chipy, zasiane i wyrenderowane. */
  function seeded(): string {
    useMemory.setState({ notes: [WAITING] });
    useSkills.setState({ installed: [PLACED], pending: PENDING });
    return (
      renderToStaticMarkup(<NotesShelf store={useMemory} />) +
      renderToStaticMarkup(<SkillsShelf store={useSkills} />)
    );
  }

  it('read some chips at all', () => {
    const chips = elements(seeded()).filter(([, classes, tone]) => {
      const one = look(classes, tone);
      return one.outline && one.outlineState !== undefined;
    });
    expect(
      chips.length,
      'not one fully outlined state element was read out of the two screens that carry them, ' +
        'so every rule below would sweep an empty list',
    ).toBeGreaterThan(0);
  });

  it('makes every filled state element a pill', () => {
    const square = elements(seeded()).filter(([, classes, tone]) => {
      const one = look(classes, tone);
      return one.outline && one.outlineState !== undefined && one.fill !== undefined && !one.pill;
    });
    expect(
      square,
      'these state elements are filled and outlined all round, which is what a chip is, and they ' +
        'do not take the pill corner: ' +
        JSON.stringify(square) +
        '. A chip is read at a glance from its silhouette before its colour is read at all.',
    ).toEqual([]);
  });

  it('never mixes one state with another', () => {
    const mixed = elements(seeded()).filter(([, classes, tone]) => {
      const one = look(classes, tone);
      return (
        one.outlineState !== undefined && one.fill !== undefined && one.outlineState !== one.fill
      );
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
    const filled = elements(seeded()).filter(([tag, classes, tone]) => {
      const one = look(classes, tone);
      return tag === 'button' && one.outlineState !== undefined && one.fill !== undefined;
    });
    expect(
      filled,
      'these buttons carry a state colour AND a fill: ' +
        JSON.stringify(filled) +
        '. A filled button is the most prominent thing on a screen, and prominence belongs to ' +
        'the one interactive colour — a filled warning competes with the action a person came for.',
    ).toEqual([]);
  });

  /* Cichy chip to dzis `class="chip"` BEZ `data-tone` — ksztalt pigulki i obrys `--line` niesie
   * baza z arkusza. Pytanie jest to samo, co bylo: czy istnieje chip, ktory nie mowi o stanie. */
  it('keeps a quiet chip for the things that are in no state at all', () => {
    const quiet = elements(seeded()).filter(([, classes, tone]) => {
      const one = look(classes, tone);
      return one.pill && one.outline && one.outlineState === undefined && one.fill === undefined;
    });
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
 * mieszkaja tez w stalych i w mapach, wiec `className=` szukane wprost widzi mniej niz polowe.
 *
 * DRUGI ODCZYT, 2026-08-31: `data-tone`. Po przebudowie stan przestal byc wylacznie napisem
 * klasowym — w czterech sekcjach nosi go dzis atrybut. Napis klasowy i ton to dwa sposoby
 * powiedzenia jednej rzeczy, wiec ochrona przed pusta lista liczy OBA; inaczej swiecilaby na
 * czerwono za poprawna migracje, dokladnie tak samo, jak swiecilaby, gdyby barwa zniknela. */
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

/** Kazdy ton nazwany w pliku, takze ten podany warunkiem (`data-tone={x ? 'fail' : undefined}`). */
function toneLiterals(source: string): readonly string[] {
  const out: string[] = [];
  for (const hit of source.matchAll(/data-tone=(?:\x22([a-z]+)\x22|\{([^}]*)\})/g)) {
    const plain = hit[1];
    if (plain !== undefined) {
      out.push(plain);
      continue;
    }
    for (const inner of (hit[2] ?? '').matchAll(/[\x27\x22]([a-z]+)[\x27\x22]/g)) {
      out.push(inner[1] ?? '');
    }
  }
  return out;
}

/** Kazdy ton, ktory arkusz naprawde maluje. Ton spoza tej listy nie zmienia ani jednego piksela. */
const TONES_IN_SHEET: ReadonlySet<string> = new Set(
  [...SHEET.matchAll(/\[data-tone=\x22([a-z]+)\x22\]/g)].map((hit) => hit[1] ?? ''),
);

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
  const read = sources();
  const written = read.flatMap(([path, text]) =>
    classLiterals(text).map((classes) => [path, classes] as const),
  );
  const toned = read.flatMap(([path, text]) =>
    toneLiterals(text).map((tone) => [path, tone] as const),
  );

  it('read the class lists out of all four sections', () => {
    expect(
      written.length,
      'almost no class lists were read out of the four sections, so both rules below would sweep ' +
        'an empty list',
    ).toBeGreaterThan(60);
    const spoken =
      written.filter(([, classes]) => outlineState(classes) !== undefined).length +
      toned.filter(([, tone]) => (STATES as readonly string[]).includes(tone)).length;
    expect(
      spoken,
      'not one state colour was read, in either of the two ways a section can state one — a class ' +
        'list that names the edge, or a tone the sheet paints. The four sections carry them in ' +
        'seven places; reading zero means the reader stopped seeing constants, maps and tones.',
    ).toBeGreaterThan(4);
  });

  /* Ton, ktorego arkusz nie maluje, jest dokladnie ta sama wada, co kontrolka bez handlera:
   * napis, ktory wyglada na rozstrzygniecie i nie robi nic. Nie pada, nie ostrzega, a element
   * zostaje w barwie neutralnej — czyli stan, ktory ktos chcial pokazac, po prostu nie dojezdza
   * na ekran. Nazwa tonu nie ma tu zadnego innego sposobu, zeby sie zwalidowac. */
  it('every tone a section names is a tone the sheet paints', () => {
    expect(
      [...TONES_IN_SHEET],
      'theme.css defines no tone at all, so the rule below would pass on an empty set and any ' +
        'name a section writes would read as painted.',
    ).not.toEqual([]);
    const unpainted = toned.filter(([, tone]) => !TONES_IN_SHEET.has(tone));
    expect(
      unpainted,
      'these places name a tone that theme.css never paints: ' +
        JSON.stringify(unpainted) +
        '. The element keeps the neutral look and the state a person was meant to see never ' +
        'reaches the screen — silently, because a name nothing matches cannot fail.',
    ).toEqual([]);
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
