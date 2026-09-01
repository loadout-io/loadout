import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../App';
import type { Section } from '../ui/sections';

/* AC-3 dla T-48: znacznik pustego ekranu siedzi na ZDANIU, w kazdej sekcji z osobna.
 *
 * `src/App.tsx` mowi to o sobie wprost: „`data-empty` siedzi na elemencie, ktory niesie SAMO
 * zdanie — nie na ramce z zaproszeniem", bo trescia tak oznaczonego elementu ma byc zdanie,
 * a nie „glif zdanie zdanie przycisk". Trzy sekcje z czterech tak robia. Workflows trzyma
 * znacznik na opakowaniu, wiec kazda wyrocznia czytajaca ten znacznik dostaje dla tej jednej
 * sekcji cos innego niz dla pozostalych — a wyrocznia, ktora dla piatej sekcji mierzy inna
 * rzecz, milczy dokladnie tam, gdzie powinna krzyczec.
 *
 * PRAWDZIWE ODKRYWANIE, nie `screens={{}}`. Z pusta mapa ekranow powloka rysuje zdanie
 * z rejestru sekcji i zadnej sekcji nie montuje: kazdy ekran wyglada wtedy identycznie
 * i test przechodzi, nie zobaczywszy ani jednej sekcji. Zmierzone 2026-08-19: piec ekranow,
 * po szesc przyciskow, zero pol, `data-empty` wszedzie — czyli sama powloka.
 *
 * CZEGO TU NIE MA I DLACZEGO. Zargonu nie sadzimy: `checks/quick-vocabulary.sh` sadzi kazdy
 * napis w `src/` przy kazdym biegu, a druga kopia tej tabeli w tescie to dwa zrodla prawdy
 * (niezmiennik 23). Nie ma tez zadania „na kazdym pustym ekranie stoi czynna kontrolka": Agents,
 * Skills i Workflows ja maja i pilnuja tego wlasne testy, a w Memory notatki pisze AGENT, nie
 * czlowiek — przycisk dopisany tam po to, zeby kryterium zzieleniało, bylby kontrolka bez
 * czynnosci.
 *
 * SLABA WERSJA: asercja na obecnosc `data-empty`. Przechodzi dzis, kiedy trescia oznaczonego
 * elementu jest cztero-czlonowe „glif zdanie zdanie przycisk".
 */

const FIVE = [
  'run',
  'workflows',
  'agents',
  'skills',
  'memory',
  'triggers',
  'settings',
] as const satisfies readonly Section[];

/** Tekst bez znacznikow, ze scisnietymi odstepami. */
const plain = (html: string): string =>
  html
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();

/* STREFA PUSTEGO EKRANU, czyli element, w ktorym oznaczone zdanie STOI — nie caly ekran.
 *
 * Zmierzone kontrola negatywna 2026-08-19: warunek „poza znacznikiem jest jeszcze co czytac"
 * postawiony na calym dokumencie przechodzi zawsze, bo sama powloka niesie nazwe sekcji, piec
 * pozycji nawigacji i stopke. Skasowanie zaproszenia w Memory nie ruszalo go ani o krok. Pytanie
 * dotyczy TEJ strefy, wiec liczymy w niej: idziemy po znacznikach do miejsca, w ktorym stoi
 * oznaczone zdanie, i bierzemy najglebszy element, ktory jest jeszcze otwarty. */
function regionAround(markup: string, at: number): string {
  const stack: [string, number][] = [];
  const tag = /<(\/?)([a-zA-Z][\w-]*)([^>]*)>/g;
  let hit = tag.exec(markup);
  while (hit !== null && hit.index < at) {
    const [whole, slash, name] = hit;
    if (slash === '/') stack.pop();
    else if (!whole.endsWith('/>') && !VOID.has(name ?? '')) stack.push([name ?? '', hit.index]);
    hit = tag.exec(markup);
  }
  const parent = stack[stack.length - 1];
  if (parent === undefined) return '';
  const [name, from] = parent;
  let depth = 0;
  const walk = new RegExp('<(/?)' + name + '\\b[^>]*>', 'g');
  walk.lastIndex = from;
  let step = walk.exec(markup);
  while (step !== null) {
    depth += step[1] === '/' ? -1 : 1;
    if (depth === 0) return markup.slice(from, step.index + step[0].length);
    step = walk.exec(markup);
  }
  return markup.slice(from);
}

/** Elementy bez zamkniecia — inaczej stos przodkow rozjezdza sie na pierwszym `<br>`. */
const VOID = new Set(['br', 'hr', 'img', 'input', 'meta', 'link', 'source', 'area', 'col']);

/* Tekst widoczny w oznaczonym elemencie — wyciety PO GLEBOKOSCI, nie leniwym wzorcem.
 *
 * Leniwe `<\/\1>` konczy na PIERWSZYM zamknieciu tej samej nazwy, a nie na zamknieciu TEGO
 * elementu. Opakowanie `<div data-empty>` z pierwszym dzieckiem `<div>` daje wtedy tresc
 * `<div>Zdanie`, ktora po zdjeciu znacznikow jest samym zdaniem i przechodzi kazdy warunek nizej
 * — czyli forma z opakowaniem, ktora to kryterium ma usuwac, zostaje dopuszczalna, jesli tylko
 * pierwsze dziecko ma te sama nazwe co opakowanie. */
function markedSpans(markup: string): readonly string[] {
  const out: string[] = [];
  const open = /<([a-z]+)[^>]*\sdata-empty\b[^>]*>/g;
  let hit = open.exec(markup);
  while (hit !== null) {
    const name = hit[1] ?? '';
    const from = hit.index + hit[0].length;
    const walk = new RegExp('<(/?)' + name + '\\b[^>]*>', 'g');
    walk.lastIndex = from;
    let depth = 1;
    let to = markup.length;
    let step = walk.exec(markup);
    while (step !== null) {
      depth += step[1] === '/' ? -1 : 1;
      if (depth === 0) {
        to = step.index;
        break;
      }
      step = walk.exec(markup);
    }
    out.push(markup.slice(from, to));
    open.lastIndex = to;
    hit = open.exec(markup);
  }
  return out;
}

/** Tekst widoczny w oznaczonym elemencie: bez znacznikow, ze scisnietymi odstepami. */
const markedText = (markup: string): readonly string[] => markedSpans(markup).map(plain);

describe('pusty ekran', () => {
  const screens = FIVE.map(
    (section) => [section, renderToStaticMarkup(<App section={section} />)] as const,
  );

  it('reaches the real sections, not the shell on its own', () => {
    for (const [section, markup] of screens) {
      expect(markup.length, 'nothing at all was rendered for ' + section).toBeGreaterThan(400);
    }
    /* Kontrola: ekrany musza sie od siebie ROZNIC. Szesc identycznych znaczy, ze zadna sekcja
     * sie nie zamontowala i wszystko nizej mierzy sama powloke. */
    expect(
      new Set(screens.map(([, markup]) => markup)).size,
      'two sections rendered the same document, so at least one of them did not mount and ' +
        'every assertion below is about the window frame',
    ).toBe(FIVE.length);
  });

  it('marks exactly one place in each section', () => {
    for (const [section, markup] of screens) {
      expect(
        markedText(markup).length,
        'section ' +
          section +
          ' marks its empty place ' +
          String(markedText(markup).length) +
          ' times. One fact lives in one place, and a reader of that marker has to know which ' +
          'one it is.',
      ).toBe(1);
    }
  });

  it('puts the marker on the sentence, and on nothing else', () => {
    for (const [section, markup] of screens) {
      const [only = ''] = markedText(markup);
      expect(
        only.length,
        'the marked place in ' + section + ' says almost nothing: ' + JSON.stringify(only),
      ).toBeGreaterThan(9);
      expect(
        only,
        'the marked place in ' +
          section +
          ' carries more than its sentence: ' +
          JSON.stringify(only) +
          '. The glyph, the invitation and the button belong beside the sentence, not inside the ' +
          'marker — a reader of this marker wants the sentence and gets a paragraph.',
      ).not.toMatch(/[◇＋+]|\s{2}/);
      expect(
        only.split('.').filter((part) => part.trim() !== '').length,
        'the marked place in ' +
          section +
          ' carries more than one sentence: ' +
          JSON.stringify(only),
      ).toBe(1);
    }
  });

  it('leaves the invitation outside the marker but inside the same place', () => {
    for (const [section, markup] of screens) {
      const [only = ''] = markedText(markup);
      const at = markup.search(/<[a-z]+[^>]*\sdata-empty\b/);
      expect(at, 'the marker was not found in ' + section).toBeGreaterThan(-1);
      const region = regionAround(markup, at);
      expect(region.length, 'no empty place was read out of ' + section).toBeGreaterThan(
        only.length,
      );
      const around = plain(region.replace(/<([a-z]+)[^>]*\sdata-empty\b[^>]*>[\s\S]*?<\/\1>/, ' '));
      /* DWA MIEJSCA, W KTORYCH MOZE STAC WYJSCIE, i to nie jest rozluznienie.
       *
       * W pieciu sekcjach listowych zaproszenie stoi w tej samej strefie, co zdanie: „Add one,
       * and a step in any workflow can be handed to it" plus przycisk. W Run stoi w wierszu
       * wejscia na dole ekranu, ktory jest tam ZAWSZE, takze wtedy, gdy nic nie chodzi — i to on
       * jest cala droga dalej, bo bieg zaczyna sie od napisania, czego chcesz. Zmierzone
       * 2026-08-19: pola tekstowe na pustym ekranie ma WYLACZNIE Run (jedno zywe), a cztery
       * pozostale sekcje zero. T-65 dopisuje piata sekcje listowa i ten sam warunek. Ta druga
       * galaz nie jest wiec dziura dla nich — nie ma czym jej
       * spelnic poza dorobieniem sobie pola do pisania. */
      const typing = [...markup.matchAll(/<(?:textarea|input)\b[^>]*>/g)]
        .map((one) => one[0])
        .filter((one) => !one.includes('disabled'));
      expect(
        around.length > 24 || typing.length > 0,
        'the empty place in ' +
          section +
          ' says ' +
          JSON.stringify(only) +
          ', beside it only ' +
          JSON.stringify(around) +
          ', and the screen has nothing to type into either. An empty screen that reports a lack ' +
          'and offers nothing is a dead end — measured in THIS place, not in the document, ' +
          'because the window frame alone would satisfy any count taken over the whole screen.',
      ).toBe(true);
    }
  });

  it('shows no leftover of a value nobody has', () => {
    for (const [section, markup] of screens) {
      const words = markup
        .replace(/<[^>]*>/g, ' ')
        .replace(/\s+/g, ' ')
        .trim();
      for (const leftover of ['undefined', 'null', 'n/a', 'N/A', 'not reported', 'NaN']) {
        expect(
          words.includes(leftover),
          'section ' +
            section +
            ' shows ' +
            leftover +
            ' to a person. A row with no value simply does not exist; a placeholder standing in ' +
            'for one is the shape of a fact that is not there.',
        ).toBe(false);
      }
    }
  });
});
