/* Kryterium 4 dla T-01: paleta jest zamknięta w tym CSS-ie, który aplikacja NAPRAWDĘ ładuje.
 *
 * Kompilujemy src/styles/global.css — plik importowany przez main.tsx — a nie theme.css z ręki.
 * `expect(themeCss).toContain("--color-*: initial")` przechodzi na repo, w którym global.css
 * nigdy nie importuje theme.css: aplikacja ładuje wtedy pełną domyślną paletę Tailwinda,
 * `bg-slate-800` znów się kompiluje, i nic nie wygląda źle, bo nasze wartości też są na miejscu.
 * T8 §6.4 nazywa zamknięcie palety „the enforcement mechanism", nie dokumentacją — więc
 * sprawdzamy je egzekucją: klasa spoza listy ma NIE wyprodukować reguły.
 *
 * 2026-08-19, T-45: obie listy urosły razem z paletą Quiet Glass. Historia zmiany jest przy
 * `THEIRS`, bo tam mieszka jedyna decyzja, która wygląda na cofnięcie ochrony, a nią nie jest.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compile } from 'tailwindcss';
import { beforeAll, describe, expect, it } from 'vitest';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const ENTRY = resolve(ROOT, 'src', 'styles', 'global.css');
const MAIN = resolve(ROOT, 'src', 'main.tsx');
const TAILWIND = resolve(ROOT, 'node_modules', 'tailwindcss', 'index.css');

/** Klasy z naszej listy. Każda musi wyprodukować regułę. */
const OURS = [
  'bg-panel',
  'text-title',
  'rounded-md',
  'font-mono',
  /* Cztery dołożone 2026-08-19 (T-45): powierzchnia stanu „teraz", nowy stopień drabinki,
   * nowy promień z pasma domu i cień. Bez nich lista pozytywna sądziła paletę, której
   * połowa jest młodsza niż ona sama. */
  'bg-live',
  'text-eyebrow',
  'rounded-md',
  'shadow-md',
];
/** Klasy spoza listy. Żadna nie ma prawa wyprodukować reguły.
 *
 * DWIE NAZWY PRZESZŁY STĄD NA LISTĘ POWYŻEJ 2026-08-19 i to nie jest osłabienie strażnika.
 * `rounded-lg` i `shadow-lg` były tu jako domyślne Tailwinda; T-45 wprowadza pasmo promieni
 * i cieni z systemu meetnotes, a ono używa DOKŁADNIE tych nazw (`--radius-lg: 18px`,
 * `--shadow-lg`). Nazwa, którą sami definiujemy, nie jest już obca — pytanie tego punktu
 * brzmi „czy kompiluje się coś, czego NIE zadeklarowaliśmy", a nie „czy ta konkretna nazwa
 * milczy". W ich miejsce wchodzą dwie, które obce pozostają: `--radius-*: initial`
 * i `--shadow-*: initial` czyszczą całe przestrzenie, a my deklarujemy z nich cztery i trzy
 * stopnie, więc `rounded-3xl` i `shadow-2xl` nie mają skąd się wziąć. */
const THEIRS = ['bg-slate-800', 'text-3xl', 'rounded-3xl', 'shadow-2xl', 'font-sans'];

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/* Rozwiązywanie importów robimy sami, bo `compile()` nie zna node_modules. `@import "tailwindcss"`
 * wskazuje na wejście pakietu, wszystko inne liczy się względem pliku, który to napisał.
 */
async function loadStylesheet(id: string, base: string) {
  const own = id === 'tailwindcss' || id.startsWith('tailwindcss/');
  const path = own ? TAILWIND : resolve(base, id);
  return { path, base: dirname(path), content: fileText(path) };
}

const ESCAPE = /[.*+?^${}()|[\]\\]/g;

function hasRule(built: string, className: string): boolean {
  const selector = '\\.' + className.replace(ESCAPE, '\\$&') + '[\\s,{:]';
  return new RegExp(selector).test(built);
}

let css = '';
let compiled = false;

beforeAll(async () => {
  try {
    const sheet = await compile(fileText(ENTRY), {
      base: dirname(ENTRY),
      from: ENTRY,
      loadStylesheet,
    });
    css = sheet.build([...OURS, ...THEIRS]);
    compiled = true;
  } catch {
    css = '';
    compiled = false;
  }
});

describe('the palette is closed in the style sheet this app actually loads', () => {
  it('loads exactly one style sheet, and it is the one measured here', () => {
    const imports = [...fileText(MAIN).matchAll(/^\s*import\s+['"]([^'"]+\.css)['"]/gm)].map(
      (hit) => hit[1] ?? '',
    );
    expect(
      imports.length,
      'src/main.tsx has to bring in exactly one style sheet. Two, and the second one can quietly ' +
        'be the whole default palette; it brings in: ' +
        JSON.stringify(imports),
    ).toBe(1);
    expect(
      (imports[0] ?? '').replace(/^@\//, './'),
      'that one style sheet has to be ./styles/global.css — the file compiled below. Point ' +
        'main.tsx anywhere else and this whole measurement is about a file nobody loads',
    ).toBe('./styles/global.css');
  });

  it('builds our own classes', () => {
    expect(compiled, 'the style sheet has to compile before anything can be read out of it').toBe(
      true,
    );
    for (const name of OURS) {
      expect(
        hasRule(css, name),
        name +
          ' has to produce a rule. These are the surfaces, the ladder rungs, the shapes, the ' +
          'depth and the machine-value family that DESIGN.md defines, and the shell is written ' +
          'in them',
      ).toBe(true);
    }
  });

  it('builds nothing outside our own list', () => {
    for (const name of THEIRS) {
      expect(
        hasRule(css, name),
        name +
          ' has to produce no rule at all. It compiles again the moment global.css stops ' +
          'reaching theme.css, and nothing looks wrong when it does — our own values still ' +
          'work, so the closed palette dies quietly and twenty later tasks inherit it',
      ).toBe(false);
    }
    expect(
      css.includes('slate'),
      'the default palette has to be gone from the result entirely, not merely unused: a name ' +
        'that is still declared is a name someone can still type',
    ).toBe(false);
  });
});
