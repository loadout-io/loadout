import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { useRun } from '../../state/run';
import Run from '../../sections/run/index';

/* AC-6 dla T-45: stopien nadoczka ma NOSNIKI, a wersaliki nie sa wpisane z palca.
 *
 * TO KRYTERIUM ISTNIEJE, BO ZNALAZLA JE DRUGA OPINIA, A NIE ZADNE Z PIECIU POZOSTALYCH.
 * Zmierzone 2026-08-19 na gotowym, ZIELONYM zestawie: `theme.css` skasowal
 * `.text-label { text-transform: uppercase }` spod 42 uzyc `text-label`, a `--text-eyebrow`
 * mial ZERO nosnikow w `src/**\/*.tsx`. Skutek: makieta dalej zadala AGENTS, aplikacja
 * rysowala Agents, bramka byla zielona, i nic tego nie zglaszalo.
 *
 * To jest dokladnie awaria z niezmiennika 25, ktora plik zadania cytuje trzy razy: deklaracja
 * skasowana spod niezmigrowanych powierzchni, ktora nie rzuca wyjatku i nie pojawia sie
 * w zadnym logu. Pasmo promieni dostalo na to nazwe zastepcza na czas migracji; zniknela w T-50, kiedy
 * ostatnia powierzchnia byla juz przeniesiona.
 * Rozszczepienie drabinki aliasu dostac NIE MOZE — z klasy `text-label` nie da sie odczytac,
 * czy stoi na nadoczku sekcji, czy na etykiecie pola — wiec zamiast aliasu ma to kryterium.
 *
 * DLACZEGO JEDEN PUNKT RENDERUJE, A NIE CZYTA ZRODLA. Skan tekstu mowi, co jest NAPISANE.
 * Punkt renderujacy mowi, co WYCHODZI — a to jest inne pytanie i to ono odpowiada na defekt:
 * naglowek szyny stracil wersaliki w WYNIKU, nie w zrodle. `rail-shows-agents.test.tsx` nazywa
 * import samego `Rail` slaba wersja SWOJEGO kryterium i ma racje: tamto pyta „czy to jest
 * zamontowane". Nasze pyta „czy ten naglowek nosi wlasciwy stopien", a na to render samego
 * komponentu jest jednostka dokladna, nie slabsza.
 *
 * DLACZEGO PUNKT (c) JEST NAJWAZNIEJSZY. To on lapie prawdziwy defekt: `rail.tsx:87` niosl
 * `<h2 className="... text-label ...">Agents</h2>`. Naglowek NIE JEST etykieta, wiec stopien
 * etykiety nie ma prawa na nim stac — niezaleznie od tego, jak wyglada w danym tygodniu.
 *
 * 2026-08-31 — TEN PUNKT RENDERUJE DZIS CALY EKRAN PRACY. Kolumna z lista agentow zniknela,
 * a razem z nia jej naglowek; naglowek nadoczka na tym ekranie nosi teraz kolumna z obrazem
 * planu. Pytanie zostalo to samo, zmienil sie renderowany komponent — i przy okazji zrobilo
 * sie mocniejsze, bo naglowek jest sadzony tam, gdzie czlowiek go widzi (niezmiennik 29).
 * Kroki trzeba zasiac: naglowek nad pustka nie istnieje z premedytacja (niezmiennik 17).
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const SRC = resolve(ROOT, 'src');

/** Pliki produkcyjne pod `src/`: bez testow, bez fikstur. */
function productionFiles(): readonly string[] {
  const out: string[] = [];
  const walk = (dir: string): void => {
    if (!existsSync(dir)) return;
    for (const name of readdirSync(dir).sort()) {
      const full = join(dir, name);
      if (statSync(full).isDirectory()) {
        if (name !== 'fixtures') walk(full);
        continue;
      }
      if (!/\.(ts|tsx)$/.test(name)) continue;
      if (/\.(test|spec)\.[jt]sx?$/.test(name)) continue;
      out.push(full);
    }
  };
  walk(SRC);
  return out;
}

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

/** Zdejmuje komentarze: cytat reguly w prozie nie jest regula (lekcja z tej samej fali). */
function withoutComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/^\s*\/\/.*$/gm, ' ');
}

/** Stale klas w pliku: `const NAZWA = '...'` -> tresc. Bez tego `className={ZONE_TITLE}`
 *  jest dla skanera pusty, a wlasnie tam siedzialy trzy naglowki strefy w polce notatek. */
function classConstants(src: string): Map<string, string> {
  const table = new Map<string, string>();
  /* Oba cudzyslowy zapisane heksadecymalnie (\x22 i \x27), a nie doslownie. Powod jest ten sam
   * co przy naglowku ponizej: literal wyrazenia regularnego o NIEPARZYSTEJ liczbie cudzyslowow
   * rozsynchronizowuje skaner `checks/quick-vocabulary.sh`, ktory od tego miejsca czyta kod
   * jako tekst widoczny dla czlowieka. Zapis heksadecymalny ma ich zero i przy okazji obsluguje
   * takze stala w cudzyslowie podwojnym, ktorej stara wersja nie widziala. */
  const DECL = /const\s+([A-Za-z_]\w*)\s*(?::\s*string\s*)?=\s*([\x22\x27])([^\x22\x27]*)\2/g;
  for (const hit of src.matchAll(DECL)) {
    table.set(hit[1] ?? '', hit[3] ?? '');
  }
  return table;
}

/** Klasy naglowkow `<h2>`/`<h3>` w pliku, z rozwinieciem stalych. */
function headingClasses(src: string): readonly string[] {
  const consts = classConstants(src);
  const out: string[] = [];
  for (const hit of src.matchAll(/<h[23]\b([\s\S]{0,240}?)>/g)) {
    const open = hit[1] ?? '';
    /* Cudzyslowy zapisane heksadecymalnie (\x22, \x27). Literal wyrazenia regularnego
     * o NIEPARZYSTEJ liczbie cudzyslowow rozsynchronizowuje skaner
     * `checks/quick-vocabulary.sh`, ktory od tego miejsca czyta KOD jako tekst widoczny
     * dla czlowieka i zglasza slowa, ktorych w zadnym napisie nie ma. */
    const literal = /className=\x22([^\x22]*)\x22/.exec(open)?.[1];
    if (literal !== undefined) {
      out.push(literal);
      continue;
    }
    const named = /className=\{([A-Za-z_]\w*)\}/.exec(open)?.[1];
    if (named !== undefined) out.push(consts.get(named) ?? '');
  }
  return out;
}

describe('stopien nadoczka ma nosniki', () => {
  const files = productionFiles();

  it('scanned enough production files to be measuring anything', () => {
    expect(
      files.length,
      'the walk over src/ found almost no production file, so every point below would loop over ' +
        'an empty list and pass on nothing',
    ).toBeGreaterThan(20);
  });

  it('has at least one carrier, so the new rung is not inert', () => {
    const carriers = files.filter((f) => withoutComments(text(f)).includes('text-eyebrow'));
    expect(
      carriers.map((f) => relative(ROOT, f)),
      'no production file uses the eyebrow rung. A rung nothing carries is inert: the sheet says ' +
        'capitals, the app says nothing, and the point that measures the sheet is satisfied by a ' +
        'class no screen is written in.',
    ).not.toEqual([]);
  });

  it('never spells the capitals out in a component', () => {
    const guilty: string[] = [];
    for (const f of files) {
      const src = withoutComments(text(f));
      if (/\buppercase\b/.test(src)) guilty.push(relative(ROOT, f));
    }
    expect(
      guilty,
      'these components spell the capitals out instead of naming the rung. A second copy of one ' +
        'fact (invariant 13) survives a change to the rung silently — which is exactly how three ' +
        'headings kept their capitals through this task while two others lost them and nothing ' +
        'went red.',
    ).toEqual([]);
  });

  it('never puts the LABEL rung on a heading, because a heading is not a label', () => {
    const guilty: string[] = [];
    for (const f of files) {
      const src = withoutComments(text(f));
      for (const cls of headingClasses(src)) {
        if (/\btext-label\b/.test(cls)) guilty.push(relative(ROOT, f) + ' -> ' + cls);
      }
    }
    expect(
      guilty,
      'these headings are drawn with the field-label rung. `--t-label` is documented as "field ' +
        'label" and `--t-eyebrow` as "section eyebrow"; a heading wearing the label rung reads ' +
        'correctly only for as long as the two rungs happen to look the same, and the moment ' +
        'they split it changes appearance with nothing to say so.',
    ).toEqual([]);
  });

  it('renders the work-view heading on the eyebrow rung, not the label rung', () => {
    useRun.setState({
      steps: [{ id: 's_build', name: 'Build', state: 'running' }],
    });
    const markup = renderToStaticMarkup(createElement(Run));
    /* Sprawdzamy CALY znacznik otwierajacy, a nie wartosc atrybutu wyciagnieta wzorcem.
     * Powod jest praktyczny i warto go zapisac: literal wyrazenia regularnego z NIEPARZYSTA
     * liczba cudzyslowow rozsynchronizowuje skaner `checks/quick-vocabulary.sh` — jego wzorzec
     * na literaly bierze wtedy KOD za tekst widoczny dla czlowieka i zglasza slowa, ktorych
     * w zadnym napisie nie ma. Wersja bez cudzyslowow jest przy tym rownie mocna: pytamy
     * o obecnosc jednego stopnia i nieobecnosc drugiego, a nie o dokladna tresc atrybutu. */
    /* PIERWSZY `h2`, KTORY NIE JEST NAZWA SEKCJI — poprawka celowania z 2026-08-31, nie
     * zluzowanie. Do przebudowy pierwszym `h2` widoku pracy byl jego wlasny naglowek i tylko
     * on; dzis stoi przed nim nazwa sekcji z paska (`data-section-name`), ktora nadoczkiem
     * byc NIE MOZE: rung nadoczka wersalikuje tresc arkuszem, a `e2e/tests/sections-mount.spec.ts`
     * porownuje ten napis z rejestrem sekcji wprost i szlo na czerwono na samej wielkosci liter.
     * Dwa naglowki, dwa rozne pytania — to kryterium pyta o naglowek WIDOKU, co ma w nazwie.
     * Obie asercje nizej zostaja co do znaku. */
    const headings = [...markup.matchAll(/<h2[^>]*>/g)].map((hit) => hit[0]);
    const heading = headings.find((one) => !one.includes('data-section-name')) ?? '';
    expect(
      heading,
      'the work view rendered no <h2> at all, so the two assertions below would run against ' +
        'an empty string and say nothing about what the screen draws',
    ).not.toBe('');
    expect(
      heading,
      'the rendered heading does not carry the eyebrow rung. The mockup rule for it ' +
        'asks for capitals and the sheet puts them on the eyebrow rung only, so a heading ' +
        'without it draws sentence-case while the oracle still says AGENTS.',
    ).toContain('text-eyebrow');
    expect(
      heading,
      'the rendered heading still carries the field-label rung as well. Two rungs on one ' +
        'element is two answers to one question about its size and case.',
    ).not.toContain('text-label');
  });

  it('resolves class constants, or the point above has a hole the size of the notes shelf', () => {
    /* Kontrola samego skanera. `memory/shelf.tsx` trzyma klasy naglowkow w stalej
     * (`const ZONE_TITLE = '...'`), wiec skaner czytajacy wylacznie literaly `className="..."`
     * przepuscilby trzy naglowki strefy i punkt wyzej bylby zielony na dziurze. */
    const sample = "const ZONE_TITLE = 'text-label text-muted';\n<h2 className={ZONE_TITLE}>x</h2>";
    expect(
      headingClasses(sample),
      'the scanner no longer resolves a class held in a constant, so every heading written that ' +
        'way is invisible to the point above',
    ).toEqual(['text-label text-muted']);
  });
});
