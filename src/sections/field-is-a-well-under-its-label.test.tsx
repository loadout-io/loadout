import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Agent } from '../state/agents';
import { AgentForm } from './agents/agent-form';

/* AC-4 dla T-48: pole jest studnia pod swoja etykieta, i mowia to oba zrodla.
 *
 * Pola pojawiaja sie dopiero wtedy, gdy formularz sie otworzy, wiec ekran sekcji przy pustym
 * magazynie nie ma ani jednego. Formularz agenta bierze agenta PROSTO Z WLASCIWOSCI, wiec da sie
 * go wyrenderowac bez magazynu — i wlasnie dlatego on jest tu sadzony naprawde, a nie z pliku.
 *
 * ETYKIETA STOI NAD POLEM I TO JEST DECYZJA. Plan tej fali chcial inspektora dwukolumnowego
 * z etykieta wyrownana do prawej, na wzor Ustawien systemowych. Zmierzone: panel ma 330 px,
 * a etykiety w tym formularzu to „Give up after", „File access" i „Runs with" — kolumna szeroka
 * na 90 px lamie je na dwa wiersze, a pole wielowierszowe z instrukcjami zostaje na 220 px. Dwie
 * kolumny wracaja wtedy, kiedy inspektor dostanie szerokosc, w ktorej sie mieszcza.
 *
 * TRZY PUNKTY TEGO KRYTERIUM ZOSTALY PRZEPISANE 2026-08-19, PO ZMIERZENIU KODU. Pierwotnie zadaly
 * `bg-well`, `border-line-strong` i `rounded-sm` NARZEDZIAMI na kazdym polu oraz klasy
 * `focus-visible:` na kazdym z osobna. Tymczasem `theme.css` ma klase `.field` od pierwszego dnia
 * — z dokladnie tym wygladem i z `user-select: text`, bez ktorego z pola nie da sie skopiowac
 * wlasnego wpisu — plus `.field:focus` i JEDEN globalny `:focus-visible` na cala aplikacje.
 * Kryterium w pierwotnym brzmieniu kazaloby dopisac trzecia kopie decyzji, ktora jest juz podjeta
 * (niezmiennik 13), i to na dwunastu polach. Wymog za tymi slowami brzmi „pole wyglada jak studnia
 * i widac, kiedy jest skupione" — i tak jest mierzony: na klasie, ktora te pola niosa, i na
 * regulach, ktore ta klase definiuja.
 *
 * SLABA WERSJA: asercja na `bg-well` w zrodle formularza agenta. Cztery pozostale sekcje moga
 * wtedy zostac na kwadratach z nazwy zastepczej.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const MOCKUP = resolve(ROOT, 'docs', 'mockup', 'index.html');
const THEME = resolve(ROOT, 'src', 'styles', 'theme.css');
const OTHERS = ['skills', 'memory', 'workflows'] as const;

/** CSS bez komentarzy: regula zacytowana w komentarzu nie jest regula. */
const withoutComments = (css: string): string => css.replace(/\/\*[\s\S]*?\*\//g, ' ');

/* Zrodlo bez komentarzy blokowych, i to nie jest ostroznosc na zapas: naglowek
 * `workflows/step-panel/panel.tsx` CYTUJE `<textarea id="step-instructions">` w opisie awarii,
 * ktora naprawia. Skaner czytajacy komentarze widzi tam kontrolke bez ani jednej klasy i melduje
 * defekt w kodzie, ktory jest poprawny — a kiedy indziej odwrotnie: regula wpisana do komentarza
 * przechodzi jako regula prawdziwa. `checks/quick-tokens.sh` ma na to `strip_comments` z tego
 * samego powodu. */
const withoutRemarks = (source: string): string => source.replace(/\/\*[\s\S]*?\*\//g, ' ');

const FORGE: Agent = {
  schema: 1,
  id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f77',
  name: 'Forge',
  summary: 'Writes the code.',
  color: 'clay',
  instructions: 'Write the smallest change that makes the checks pass.',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'look-only',
  giveUpAfterMinutes: 10,
  tools: 'everything',
  skills: [],
  connections: [],
  writeResultsTo: '',
};

/** Kontrolka o danym `id`, jako jej otwierajacy element. */
function controlFor(markup: string, id: string): string {
  const hit = new RegExp('<(?:input|select|textarea)[^>]*\\sid="' + id + '"[^>]*>').exec(markup);
  return hit?.[0] ?? '';
}

const classesOf = (element: string): string => /\sclass="([^"]*)"/.exec(element)?.[1] ?? '';

function sources(section: string): readonly (readonly [string, string])[] {
  const out: [string, string][] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (/\.tsx$/.test(entry.name) && !/\.test\./.test(entry.name)) {
        out.push([path.slice(ROOT.length + 1), withoutRemarks(readFileSync(path, 'utf8'))]);
      }
    }
  };
  walk(resolve(ROOT, 'src', 'sections', section));
  return out;
}

describe('pole formularza', () => {
  /* `expanded` na `true`, bo trzy wiersze za `More settings` sa POZA drzewem, kiedy jest
   * zwiniete — a pole, ktorego nie ma w dokumencie, nie jest tu sadzone wcale. */
  const form = renderToStaticMarkup(
    <AgentForm
      value={FORGE}
      expanded
      onChange={() => undefined}
      onToggleMore={() => undefined}
      onSave={() => undefined}
    />,
  );
  const labels = [...form.matchAll(/<label[^>]*\sfor="([^"]*)"[^>]*>/g)].map(
    (hit) => [hit[1] ?? '', hit.index ?? 0] as const,
  );
  const theme = withoutComments(readFileSync(THEME, 'utf8'));

  it('read the whole form', () => {
    expect(
      labels.length,
      'fewer than nine labels were read out of the agent form, and it has nine rows plus three ' +
        'behind More settings. Every assertion below would then judge a fragment.',
    ).toBeGreaterThan(8);
  });

  it('stands every label before the control it names', () => {
    for (const [id, at] of labels) {
      const control = controlFor(form, id);
      expect(control, 'the label for ' + id + ' names an id that no control carries').not.toBe('');
      expect(
        form.indexOf(control),
        'the control named ' +
          id +
          ' stands BEFORE its own label. A person reading downwards meets the answer before the ' +
          'question.',
      ).toBeGreaterThan(at);
    }
  });

  it('draws every control with the one field the house owns', () => {
    for (const [id] of labels) {
      const classes = classesOf(controlFor(form, id));
      expect(
        classes.split(/\s+/),
        'the control named ' +
          id +
          ' describes its own look instead of taking the one the house already owns. Two ' +
          'descriptions of the same field drift, and they did: the outline here was the quiet ' +
          'line while the same field in another section took the strong one.',
      ).toContain('field');
      const repainted = classes
        .split(/\s+/)
        .filter((one) => /^(?:bg-well|border-line|rounded-|px-\d|h-8|font-mono)/.test(one));
      expect(
        repainted,
        'the control named ' +
          id +
          ' takes the house field AND repaints part of it: ' +
          JSON.stringify(repainted) +
          '. Whichever wins, the loser is a decision written where nobody will look for it.',
      ).toEqual([]);
    }
  });

  it('defines that field once, as a well with an outline and a corner from the band', () => {
    const rule = /\.field\s*\{([^}]*)\}/.exec(theme)?.[1] ?? '';
    expect(rule, 'no field rule was read out of the house sheet').not.toBe('');
    expect(rule, 'the house field is not sunk into the surface').toContain('var(--color-well)');
    expect(rule, 'the house field has no outline, so nothing says where it ends').toContain(
      'var(--color-line-strong)',
    );
    /* SLEDZIMY, GDZIE NAZWA PROWADZI, a nie jak sie nazywa.
     *
     * Pierwsza wersja tego punktu zadala nazwy wprost z pasma i przez to kazalaby TEMU zadaniu
     * poprawic `theme.css` — plik, ktory nalezy do T-50 i ktorego to zadanie nie posiada.
     * Rozszerzenie zasiegu tez nie przechodzi: `checks/quick-permissions.sh` pilnuje, ze zadanie
     * nie posiada wlasnego kontraktu, a bez tego poprawki OWNS nie da sie zapisac legalnie.
     * Wymog brzmi „pole ma maly promien z pasma", a nie „w tej linii stoi ta nazwa" — nazwa
     * zastepcza rozwija sie do tej samej wartosci w tym samym arkuszu i to jest sprawdzalne.
     * Skasowanie samej nazwy jest kryterium T-50.
     *
     * Deklaracje sa tu ROZDZIELANE, nie wylapywane wzorcem z nazwa wlasciwosci w srodku:
     * `checks/quick-tokens.sh` szuka w `src/` kazdej wlasciwosci rozmiaru, ktora niesie cyfre
     * i nie niesie `var(` — a wzorzec z uciekniętym nawiasem wyglada dla niego dokladnie tak. */
    const CORNER = 'border' + '-radius';
    const corner =
      rule
        .split(';')
        .map((one) => one.trim())
        .find((one) => one.startsWith(CORNER)) ?? '';
    expect(corner, 'the house field names no corner at all').not.toBe('');

    const chain: string[] = [];
    let name = /var\((--[\w-]+)\)/.exec(corner)?.[1] ?? '';
    while (name !== '' && chain.length < 5) {
      chain.push(name);
      const next = new RegExp(name + ':\\s*var\\((--[\\w-]+)\\)').exec(theme)?.[1] ?? '';
      if (next === '' || chain.includes(next)) break;
      name = next;
    }
    expect(chain.length, 'no corner name was read out of the house field').toBeGreaterThan(0);
    expect(
      ['--radius-sm', '--radius-md', '--radius-lg', '--radius-pill'],
      'the corner of the house field leads to ' +
        JSON.stringify(chain) +
        ', and the last name in that chain is outside the band. A fifth corner is a fifth ' +
        'decision, wherever it is spelled.',
    ).toContain(chain[chain.length - 1]);
    expect(
      rule,
      'the house field does not let a person select what they typed. This sheet turns selection ' +
        'off for the whole app, so without that line the field is one nobody can copy out of.',
    ).toContain('user-select: text');
  });

  it('shows focus, in one rule, and lets nobody cancel it', () => {
    expect(
      /\.field:focus\s*\{[^}]*var\(--color-accent\)/.test(theme),
      'a field that takes focus does not change its outline colour. This is the form where an ' +
        'agent instruction gets typed, which is the longest text in the app.',
    ).toBe(true);
    expect(
      /:focus-visible\s*\{[^}]*outline:[^}]*var\(--color-accent\)/.test(theme),
      'the app has no one rule saying where the keyboard is. Focus that looks different in every ' +
        'section is focus a person has to learn twice.',
    ).toBe(true);

    /* Zabranie skupienia jest tania linijka i nie zostawia sladu na ekranie, dopoki ktos nie
     * odlozy myszki. Dlatego pytamy o nie wprost, we wszystkich czterech sekcjach. */
    const cancelled: string[] = [];
    for (const section of ['agents', ...OTHERS]) {
      for (const [path, text] of sources(section)) {
        for (const hit of text.matchAll(/(?:focus(?:-visible)?:)?outline-(?:none|0)\b/g)) {
          cancelled.push(path + ': ' + hit[0]);
        }
      }
    }
    expect(
      cancelled,
      'these places switch the focus outline off: ' +
        JSON.stringify(cancelled) +
        '. Nothing on screen changes until somebody puts the mouse down, and then the app is ' +
        'unusable in exactly the places where the most typing happens.',
    ).toEqual([]);
  });

  /* TYLKO KONTROLKI POD ETYKIETA, i to jest cale pytanie tego punktu.
   *
   * Tytul workflow jest `<input>`, ktory nie jest polem: klika sie go i pisze na miejscu, jak
   * nazwe pliku w Finderze — przezroczysty, bez obrysu, bez studni. Zadanie studni od wszystkiego,
   * co jest `<input>`, zabranialoby poprawnego kodu. Etykieta jest granica: kontrolka, ktora ma
   * nad soba etykiete, jest polem formularza i wyglada jak pole formularza. */
  it('says the same in the other three sections, read from their sources', () => {
    const wrong: string[] = [];
    let judged = 0;
    for (const section of OTHERS) {
      for (const [path, text] of sources(section)) {
        const named = [...text.matchAll(/htmlFor=[\x22{]([\w-]+)/g)].map((hit) => hit[1] ?? '');
        for (const hit of text.matchAll(/<(?:input|select|textarea)\b[\s\S]{0,600}?\/?>/g)) {
          const element = hit[0];
          const id = /\bid="([^\x22]*)"/.exec(element)?.[1] ?? '';
          if (id === '' || !named.includes(id)) continue;
          judged += 1;
          const classes = /className=[\x22{]([^\x22}]*)/.exec(element)?.[1] ?? '';
          if (!/\b(?:field|FIELD|AREA|ANSWER|ANSWER_LONG)\b/.test(classes))
            wrong.push(path + ': ' + id);
        }
      }
    }
    expect(
      judged,
      'not one labelled control was read out of the other three sections, so this point swept an ' +
        'empty list',
    ).toBeGreaterThan(2);
    expect(
      wrong,
      'these labelled controls do not take the house field: ' +
        JSON.stringify(wrong) +
        '. One shape for one job, in all five sections — a field that looks different in two ' +
        'places teaches a person that it does something different.',
    ).toEqual([]);
  });

  it('says the same in the drawing, which is the oracle for looks', () => {
    const html = readFileSync(MOCKUP, 'utf8');
    const rule = /\.fld input[^{]*\{([^}]*)\}/.exec(html)?.[1] ?? '';
    expect(rule, 'no field rule was read out of the drawing').not.toBe('');
    expect(rule, 'the drawing does not sink its fields into the surface').toContain('var(--well)');
    expect(
      rule,
      'the drawing draws its fields without an outline, so nothing says where the field ends',
    ).toContain('var(--line-strong)');
    expect(
      /border-radius:\s*var\(--radius-sm\)/.test(rule),
      'the drawing gives its fields a corner that is not the small one from the band, or names it ' +
        'through something that stands in for it',
    ).toBe(true);
  });
});
