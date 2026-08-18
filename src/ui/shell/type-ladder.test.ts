/* Wygląd mierzony na SKOMPILOWANYM arkuszu, z wartością oczekiwaną czytaną z makiety.
 *
 * DLACZEGO NIE NA TREŚCI PLIKU. Słabą wersją każdego punktu poniżej jest
 * `expect(themeCss).toContain('uppercase')`. Przechodzi ona w trzech przypadkach, w których nic
 * nie działa: gdy słowo stoi w komentarzu, gdy stoi w regule, której Tailwind nie wypuszcza, i gdy
 * `global.css` przestaje sięgać do `theme.css` — a wtedy aplikacja ładuje arkusz bez ani jednej
 * naszej reguły i NIC nie wygląda na zepsute, bo domyślny Tailwind rysuje wszystko dalej. Dlatego
 * tu kompilujemy `src/styles/global.css` — plik, który `main.tsx` naprawdę importuje — i czytamy
 * reguły z wyniku, tak samo jak `palette.test.ts`.
 *
 * DLACZEGO WARTOŚĆ OCZEKIWANA JEST CZYTANA. Punkt wpisujący `12px` z palca przechodzi także
 * wtedy, gdy makieta mówi 14. `docs/mockup/index.html` jest jedyną wyrocznią wyglądu (commit
 * 6bc74b6), więc każda liczba niżej jest z niej wyjmowana w TYM SAMYM biegu testu, a obie strony
 * porównania są najpierw rozwijane przez własne tablice zmiennych: makieta pisze `var(--well)`,
 * Tailwind `var(--color-well)`, i bez rozwinięcia porównywalibyśmy dwa różne napisy o tym samym
 * kolorze.
 *
 * KONTROLA PRZECIW PUSTEMU PORÓWNANIU. Parser, który cicho nic nie dopasował, daje dwa puste
 * napisy i porównanie przechodzi na niczym. Każdy odczyt ma więc osobną asercję na to, że coś
 * realnie znalazł.
 *
 * Ładowarka arkusza jest tu druga (pierwsza stoi w `palette.test.ts`) i to jest świadome:
 * `compile()` nie zna `node_modules`, a dwa punkty kontrolne, które padają razem, kiedy padnie
 * jedna wspólna ładowarka, mierzą mniej niż dwa niezależne. To rusztowanie testu, nie fakt
 * o aplikacji — niezmiennik 13 dotyczy odpowiedzi, nie sposobu ich zdobycia.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compile } from 'tailwindcss';
import { beforeAll, describe, expect, it } from 'vitest';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const ENTRY = resolve(ROOT, 'src', 'styles', 'global.css');
const MOCKUP = resolve(ROOT, 'docs', 'mockup', 'index.html');
const TAILWIND = resolve(ROOT, 'node_modules', 'tailwindcss', 'index.css');

/* Klasy, o które pytamy Tailwinda. Bez tej listy nie wypuszcza ani jednej — buduje wyłącznie
 * to, co ktoś napisał. `animate-pulse` jest tu jako kontrola NEGATYWNA. */
const WANTED = [
  'text-label',
  'text-note',
  'text-subhead',
  'text-meta',
  'animate-blip',
  'animate-pulse',
  'normal-case',
];

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

async function loadStylesheet(id: string, base: string) {
  const own = id === 'tailwindcss' || id.startsWith('tailwindcss/');
  const path = own ? TAILWIND : resolve(base, id);
  return { path, base: dirname(path), content: fileText(path) };
}

/** Spłaszcza odstępy, żeby `1px  solid  #5a6d76` i `1px solid #5a6d76` były równe. */
function tight(value: string): string {
  return value.replace(/\s+/g, ' ').replace(/,\s+/g, ',').trim().toLowerCase();
}

/* Selektor musi stać na POCZĄTKU reguły, więc przed nim wolno stać tylko końcowi poprzedniej
 * reguły albo nowej linii — nigdy przecinkowi. Bez tego `.fld textarea` dopasowuje się do
 * `.fld input,.fld select,.fld textarea{...}` i czyta wysokość pola jednowierszowego. */
function ruleFinder(selector: string): RegExp {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp('(?:^|[\\n};])[ \\t]*' + escaped + '\\s*\\{([^}]*)\\}', 'g');
}

/** Ciała WSZYSTKICH reguł o podanym selektorze, w kolejności wystąpienia. */
function ruleBodies(css: string, selector: string): readonly string[] {
  return [...css.matchAll(ruleFinder(selector))].map((hit) => hit[1] ?? '');
}

/** Ciało pierwszej reguły o podanym selektorze. */
function ruleBody(css: string, selector: string): string {
  return ruleBodies(css, selector)[0] ?? '';
}

/** Wartość jednej właściwości z ciała reguły. */
function property(body: string, name: string): string {
  const found = new RegExp('(?:^|;)\\s*' + name + '\\s*:([^;]*)').exec(body);
  return tight(found?.[1] ?? '');
}

/** Tablica zmiennych CSS zadeklarowanych w arkuszu: `--nazwa` → wartość. */
function variables(css: string): Map<string, string> {
  const table = new Map<string, string>();
  for (const hit of css.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;{}]+);/g)) {
    table.set(hit[1] ?? '', tight(hit[2] ?? ''));
  }
  return table;
}

/* Makieta nazywa token `--well`, Tailwind ten sam token `--color-well`. Prefiks jest szczegółem
 * przestrzeni nazw Tailwinda, nie różnicą wyglądu, więc szukamy najpierw dosłownie, potem po
 * prefiksie. */
const NAMESPACES = ['--color-', '--radius-', '--height-', '--text-', '--font-'];

function lookup(table: Map<string, string>, name: string): string | undefined {
  const direct = table.get(name);
  if (direct !== undefined) return direct;
  const bare = name.replace(/^--/, '');
  for (const prefix of NAMESPACES) {
    const hit = table.get(prefix + bare);
    if (hit !== undefined) return hit;
  }
  return undefined;
}

/** Rozwija `var(--x)` przez tablicę zmiennych. Cztery przejścia, bo tokeny wskazują na tokeny. */
function expand(value: string, table: Map<string, string>): string {
  let out = tight(value);
  for (let round = 0; round < 4; round += 1) {
    const next = out.replace(/var\((--[a-z0-9-]+)\)/g, (whole, name: string) => {
      return lookup(table, name) ?? whole;
    });
    if (next === out) break;
    out = next;
  }
  return out;
}

let css = '';
let compiled = false;
const html = fileText(MOCKUP);
const pageVars = variables(html);
let sheetVars = new Map<string, string>();

beforeAll(async () => {
  try {
    const sheet = await compile(fileText(ENTRY), {
      base: dirname(ENTRY),
      from: ENTRY,
      loadStylesheet,
    });
    css = sheet.build(WANTED);
    compiled = true;
    sheetVars = variables(css);
  } catch {
    css = '';
    compiled = false;
  }
});

/** Ta sama właściwość po obu stronach, każda rozwinięta przez własną tablicę zmiennych. */
function bothSides(ours: string, theirs: string, name: string): readonly [string, string] {
  return [
    expand(property(ours, name), sheetVars),
    expand(property(theirs, name), pageVars),
  ] as const;
}

describe('the compiled style sheet says what the mockup says', () => {
  it('compiles at all, and the mockup is there to be read', () => {
    expect(
      compiled,
      'src/styles/global.css did not compile, so every point below would read rules out of an ' +
        'empty string and pass on nothing',
    ).toBe(true);
    expect(
      pageVars.size,
      'no CSS variables were read out of docs/mockup/index.html, so every expected value below ' +
        'would be an empty string. Either the file moved or its :root block is gone.',
    ).toBeGreaterThan(10);
  });

  it('puts the label rung in capitals, because the mockup does it in five places', () => {
    /* Pięć selektorów, które audyt wymienia. Czytamy z makiety, że NAPRAWDĘ są wersalikami —
     * bez tego punkt niżej egzekwowałby wersaliki, których wyrocznia wcale nie chce. */
    const five = ['.fld label', '.card .role', '.side h3', '.rail h2', '.ctx .ch'];
    const notUpper = five.filter(
      (selector) => property(ruleBody(html, selector), 'text-transform') !== 'uppercase',
    );
    expect(
      notUpper,
      'these mockup rules no longer say text-transform:uppercase, so the mockup stopped asking ' +
        'for capitals and this point is measuring something it no longer wants',
    ).toEqual([]);

    /* Wszystkie reguły `.text-label` w arkuszu — Tailwind wypuszcza jedną (rozmiar, interlinia,
     * rozstrzelenie, waga), nasza jest drugą. Pytamy o KAŻDĄ deklarację `text-transform`: ma być
     * dokładnie jedna i ma brzmieć `uppercase`. Dwie znaczyłyby dwa miejsca na jeden fakt. */
    const declared = ruleBodies(css, '.text-label')
      .map((body) => property(body, 'text-transform'))
      .filter((value) => value !== '');
    expect(
      declared,
      'the label rung does not carry capitals exactly once. DESIGN §4 calls --t-label "etykieta ' +
        'pola, WERSALIKI" and the mockup does it in five rules; the ladder rung is the one ' +
        'place that fixes all 34 uses of text-label at once, including the 35th nobody wrote yet.',
    ).toEqual(['uppercase']);
  });

  it('leaves the capitals overridable, by keeping the rule below the utilities layer', () => {
    /* Ta reguła musi dać się ZNIEŚĆ. Makieta ma stopień 11 px także bez wersalików (`.foot`,
     * `.tile .meta`), a komponent znosi je klasą `normal-case`. Gdyby wersaliki stały w warstwie
     * `utilities`, trafiłyby w wyniku PO `.normal-case` i przy równej specyficzności wygrałyby —
     * czyli nie dałoby się ich zdjąć nigdzie. */
    expect(
      tight(css).includes('@layer theme,base,components,utilities'),
      'the sheet no longer declares the layer order, so which layer wins is decided by source ' +
        'order and the point below stops meaning anything',
    ).toBe(true);

    const components = /@layer components\s*\{([\s\S]*?)\n\}/.exec(css)?.[1] ?? '';
    expect(
      components,
      'nothing was read out of the @layer components block, so the assertion below would pass ' +
        'on an empty string',
    ).not.toBe('');
    expect(
      property(ruleBody(components, '.text-label'), 'text-transform'),
      'the capitals do not live in the components layer. Below the utilities layer they can be ' +
        'lifted with `normal-case` where the mockup has no capitals; inside it they cannot be ' +
        'lifted anywhere.',
    ).toBe('uppercase');
    expect(
      ruleBody(css, '.normal-case'),
      'the escape hatch itself is gone from the sheet — `normal-case` produces no rule, so ' +
        'nothing can lift the capitals even from the components layer',
    ).not.toBe('');
  });

  it('carries the three rungs the mockup uses and the ladder was missing', () => {
    /* Stopień → reguła makiety, z której czytamy jego rozmiar. To są te trzy, których DESIGN §4
     * nie miał, a makieta używa ich na każdym ekranie; bez tokenu wychodzi z nich literał. */
    const rungs = [
      ['--text-note', '.tile p'],
      ['--text-subhead', '.tile .th b'],
      ['--text-meta', '.nav .foot'],
    ] as const;

    for (const [rung, selector] of rungs) {
      const wanted = property(ruleBody(html, selector), 'font-size');
      expect(
        wanted,
        'no font-size was read out of the mockup rule ' +
          selector +
          ', so the comparison for ' +
          rung +
          ' would run between two empty strings',
      ).not.toBe('');
      expect(
        sheetVars.get(rung),
        rung +
          ' has to be the size the mockup rule ' +
          selector +
          ' states. A rung the ladder does not have does not disappear from the screen — it ' +
          'comes back as a hard-coded pixel size in a component.',
      ).toBe(wanted);
    }
  });

  it('defines the field exactly once, and it matches the mockup property for property', () => {
    const ours = ruleBody(css, '.field');
    const theirs = ruleBody(html, '.fld input,.fld select,.fld textarea');

    expect(
      ours,
      'there is no .field rule in the compiled sheet. DESIGN §6 has one field; the code had two ' +
        'contradictory ones (--line plus Inter 13 in the step panel, --line-strong plus mono 12 ' +
        'in Skills), which reads as two states rather than two fields.',
    ).not.toBe('');
    expect(
      theirs,
      'nothing was read out of the mockup field rule, so every comparison below would pass on ' +
        'two empty strings',
    ).not.toBe('');

    /* Rozmiar czcionki, kolory i obwódka są w makiecie i u nas zapisane INNYMI nazwami tego
     * samego tokenu, więc obie strony rozwijamy przez własne tablice i porównujemy wartości. */
    for (const name of [
      'width',
      'height',
      'padding',
      'border',
      'border-radius',
      'background',
      'color',
      'font-family',
      'font-size',
    ]) {
      const [mine, wanted] = bothSides(ours, theirs, name);
      expect(
        wanted,
        'the mockup field rule no longer declares ' + name + ', so this comparison is empty',
      ).not.toBe('');
      expect(
        mine,
        'the shared .field class disagrees with the mockup on ' +
          name +
          '. The mockup is the only oracle for looks, so a field that looks different from it is ' +
          'a third definition, not a fix.',
      ).toBe(wanted);
    }

    expect(
      property(ruleBody(css, 'textarea.field'), 'height'),
      'the multi-line field has to be the height the mockup gives `.fld textarea`; without it ' +
        'a textarea is one line tall and the class is unusable for a prompt',
    ).toBe(property(ruleBody(html, '.fld textarea'), 'height'));
  });

  it('pulses in steps, with the timing the mockup states, and nothing pulses smoothly', () => {
    const wanted = property(ruleBody(html, '.dot.live'), 'animation');
    expect(
      wanted,
      'no animation was read out of the mockup `.dot.live` rule, so the comparison below would ' +
        'run between two empty strings',
    ).not.toBe('');

    /* Nazwa klatek jest własna (`loadout-blip` vs `blip` w makiecie), więc porównujemy WSZYSTKO
     * poza nazwą: czas, funkcję kroków i nieskończone powtarzanie. */
    const timing = (value: string): string => value.split(' ').slice(1).join(' ');
    expect(
      timing(expand(sheetVars.get('--animate-blip') ?? '', sheetVars)),
      'the one animation in this system has to have the timing the mockup states. DESIGN §7 ' +
        'wants a JUMP from opacity 1 to 0.35 — steps(2), not a curve — because smooth pulsing ' +
        'reads as breathing and the eye chases it instead of reading the line.',
    ).toBe(timing(wanted));

    const frames = /@keyframes loadout-blip\s*\{([\s\S]*?)\n\}/.exec(css)?.[1] ?? '';
    const dim = (body: string): string => tight(body).replace(/0?\.35/, '0.35');
    expect(
      dim(frames),
      'the keyframes are gone from the compiled sheet, so `animate-blip` names an animation ' +
        'that does not exist and the dot simply sits still',
    ).toContain('opacity: 0.35');

    /* Kontrola NEGATYWNA. Domyślny Tailwind daje `animate-pulse` — opacity 1 → 0.5 w 2 s krzywą,
     * czyli dokładnie to płynne oddychanie, którego DESIGN §7 zabrania. Zamknięta przestrzeń
     * `--animate-*` znaczy, że ta klasa nie produkuje ani jednej reguły. */
    expect(
      /\.animate-pulse[\s,{:]/.test(css),
      'animate-pulse compiles again, so the smooth two-second breathing DESIGN §7 forbids is ' +
        'one autocompletion away and nothing looks wrong when someone takes it',
    ).toBe(false);
  });

  it('turns every animation off when the user asked for less motion', () => {
    const reduced = /@media \(prefers-reduced-motion: reduce\)\s*\{([\s\S]*?)\n\}/.exec(css)?.[1];
    expect(
      reduced,
      'the sheet has no prefers-reduced-motion block at all. The mockup ends with one and it is ' +
        'not decoration: a dot that blinks forever is exactly what that setting exists to stop.',
    ).toBeDefined();
    expect(
      tight(reduced ?? ''),
      'the reduced-motion block does not neutralise animation duration with !important, so our ' +
        'own animation outlives the setting',
    ).toContain('animation-duration: 0.001ms !important');
    expect(
      tight(reduced ?? ''),
      'the reduced-motion block lets animations repeat, so `infinite` still means forever',
    ).toContain('animation-iteration-count: 1 !important');
  });
});
