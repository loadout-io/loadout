import { renderToStaticMarkup } from 'react-dom/server';
import { beforeAll, describe, expect, it } from 'vitest';
import { App } from '../App';
import type { Section } from '../ui/sections';
import { createAgentsStore } from '../state/agents';
import { useMemory } from '../state/memory';
import { useSkills } from '../state/skills';
import AgentsScreen from './agents';
import KnowledgeScreen from './knowledge';
import WorkflowsScreen from './workflows';
import { createWorkflowListStore } from './workflows/list/store';

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
 * Knowledge i Workflows ja maja i pilnuja tego wlasne testy, a notatek nie pisze czlowiek, tylko
 * AGENT — przycisk „dodaj notatke" dopisany po to, zeby kryterium zzieleniało, bylby kontrolka
 * bez czynnosci.
 *
 * SLABA WERSJA: asercja na obecnosc `data-empty`. Przechodzi dzis, kiedy trescia oznaczonego
 * elementu jest cztero-czlonowe „glif zdanie zdanie przycisk".
 */

/* Nazwa stalej jest starsza niz liczba, ktora nazywa, i to nie jest przeoczenie: lista rosla
 * i malala, a nazwa zostawala. 2026-08-31 Skills i Memory zeszly sie w Knowledge, wiec sekcji
 * jest szesc. Wypisane na sztywno — petla po rejestrze sadzilaby rejestr samym soba. */
const FIVE = [
  /* KOLEJNOŚĆ ZA REJESTREM, 2026-08-31: droga zaczyna się od Agents, bo workflow to agenci
     w rzędzie, a bez rzędu nie ma czego uruchomić. Wyrocznią kolejności jest makieta
     (`shell-matches-mockup.test.tsx`); ta lista stoi na sztywno, bo pętla po rejestrze
     sądziłaby rejestr samym sobą. */
  'agents',
  'workflows',
  'run',
  'triggers',
  /* Jeden identyfikator zamiast dwóch od 2026-08-31: Skills i Memory zeszły się w Knowledge. */
  'knowledge',
  'lab',
  'settings',
] as const satisfies readonly Section[];

/* ── DLACZEGO TRZY SEKCJE NIE IDĄ TU PRZEZ `<App/>`. 2026-08-31. ────────────────────────────
 *
 * Agents, Workflows i Knowledge czytają swoje katalogi w EFEKCIE po zamontowaniu, a
 * `renderToStaticMarkup` efektów nie uruchamia — więc `<App section="agents"/>` to ekran,
 * którego magazyn nigdy nie dostał odpowiedzi z dysku. Do 2026-08-31 wypisywał wtedy „No agents
 * yet.", czyli zdanie o katalogu, w który nikt nie zajrzał; od naprawy zgłoszonej przez
 * właściciela wypisuje, że CZYTA. Zaproszenie jest tam dalej i dalej jest jednym zdaniem
 * z jednym znacznikiem — tylko dochodzi się do niego po odpowiedzi katalogu, a nie przed nią.
 *
 * Ta wyrocznia pyta, GDZIE SIEDZI ZNACZNIK pustego ekranu, więc ekran musi być naprawdę pusty.
 * Dla tych trzech sekcji stawiamy je z magazynem, który JUŻ dostał odpowiedź „nic tam nie ma";
 * że montują się naprawdę przez powłokę, dowodzą ich własne `mounted.test.tsx` i pierwszy `it`
 * niżej, który dalej idzie przez `<App/>` po wszystkich siedmiu. */
function noAgents() {
  return createAgentsStore({
    list: () => Promise.resolve([]),
    newId: () => Promise.resolve('a-new'),
    save: () => Promise.resolve('rev'),
    remove: () => Promise.resolve(),
  });
}

function noWorkflows() {
  return createWorkflowListStore({
    list: () => Promise.resolve([]),
    newId: () => Promise.resolve('wf-new'),
    write: () => Promise.resolve('rev'),
    remove: () => Promise.resolve(),
  });
}

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
  const throughTheShell = FIVE.map(
    (section) => [section, renderToStaticMarkup(<App section={section} />)] as const,
  );

  /** Te same siedem sekcji, każda NAPRAWDĘ pusta — patrz akapit nad `noAgents`. */
  let screens: readonly (readonly [Section, string])[] = [];

  beforeAll(async () => {
    const agents = noAgents();
    await agents.getState().load();
    const workflows = noWorkflows();
    await workflows.getState().load();
    /* Knowledge czyta katalogi przez `invoke`, którego w vitest nie ma, więc odpowiedź „nic tam
     * nie ma" wstawiamy wprost — dla obu jego magazynów. */
    useSkills.setState({ folders: 'read', installed: [], pending: null, message: null });

    screens = throughTheShell.map(([section, markup]) => {
      if (section === 'agents') {
        return [
          section,
          renderToStaticMarkup(<AgentsScreen store={agents} usage={null} />),
        ] as const;
      }
      if (section === 'workflows') {
        return [section, renderToStaticMarkup(<WorkflowsScreen store={workflows} />)] as const;
      }
      if (section === 'knowledge') {
        /* Obie połowy sekcji muszą mieć ODPOWIEDŹ, bo dopiero wtedy ekran ma prawo powiedzieć
           „nie ma nic". Notatki czyta się przez `invoke`, którego w vitest nie ma, więc
           odpowiedź „nic tam nie ma" wstawiamy wprost — to jest ZIARNO, nie obejście: pytanie
           tej wyroczni brzmi „gdzie siedzi znacznik", a nie „skąd wzięła się pustka". */
        useMemory.setState({
          notes: [],
          passed: [],
          message: null,
          passedProblem: null,
          read: true,
        });
        return [section, renderToStaticMarkup(<KnowledgeScreen />)] as const;
      }
      return [section, markup] as const;
    });
  });

  it('reaches the real sections, not the shell on its own', () => {
    for (const [section, markup] of throughTheShell) {
      expect(markup.length, 'nothing at all was rendered for ' + section).toBeGreaterThan(400);
    }
    /* Kontrola: ekrany musza sie od siebie ROZNIC. Szesc identycznych znaczy, ze zadna sekcja
     * sie nie zamontowala i wszystko nizej mierzy sama powloke. */
    expect(
      new Set(throughTheShell.map(([, markup]) => markup)).size,
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
