/* AC-3 dla T-43: TRZECIE wejście do tej sekcji — jedno zdanie człowieka, tekst od modelu,
 * i człowiek, który ten tekst CZYTA przed zapisem.
 *
 * DWIE DROGI, KTÓRE JUŻ SĄ, WYMAGAJĄ, ŻEBY CZŁOWIEK NAPISAŁ TREŚĆ SAM. Adres (`review_skill`,
 * T-19) i formularz trzech pytań (`author_skill`, T-42) przyjmują wyłącznie gotowy tekst.
 * Loadout ma przy tym dwa sterowniki agentów, żywy nadzór procesów i dowód śmierci grupy —
 * i ani jednej drogi, która zamienia zdanie człowieka w tekst od modelu. Ta jest trzecia
 * i mieszka w TYM SAMYM panelu: „dodaj umiejętność" jest jedną decyzją z trzema odpowiedziami,
 * a nie trzema decyzjami (niezmiennik 13).
 *
 * SŁABA WERSJA TEGO KRYTERIUM, i to ona wyznacza połowę asercji niżej: „w markupie jest przycisk
 * z napisem o pisaniu". Przechodzi na przycisku BEZ HANDLERA i na stanie „pisze", którego nie da
 * się zatrzymać — czyli na kontrolce, która kłamie (niezmiennik 16), i to w jedynym miejscu tej
 * sekcji, gdzie kłamstwo kosztuje pieniądze: proces vendora pisze dalej i dalej pali limit
 * dostawcy (niezmienniki 6 i 10). Rozstrzygają dwie rzeczy: POLICZENIE DWÓCH WYWOŁAŃ na atrapie
 * granicy (pisz, zatrzymaj) i asercja o NIEOBECNOŚCI kontrolki „napisz mi to" w stanie pisania.
 *
 * DLACZEGO NAZWY KOMEND NIE SĄ TU WPISANE. Czytamy je z `src-tauri/commands.golden.txt`, a zbiór
 * nazw ARGUMENTÓW z `src-tauri/src/ipc.rs` — w tym samym biegu testu. Tauri dopasowuje argumenty
 * PO NAZWIE i deserializuje je ZANIM wejdzie w ciało komendy, więc klucz, który się nie zgadza,
 * nie daje mniejszego wywołania: daje odrzucone, przy każdym kliknięciu, z odmową w postaci
 * surowego napisu, którego nikt nie widzi. Tak był zepsuty Start 2026-08-17.
 *
 * NAZW VENDORÓW W TEJ SEKCJI NIE MA I MIEĆ NIE MOŻE. Wybór jest wyborem AGENTA ZAPISANEGO na
 * dysku — pozycje pochodzą z magazynu, nie z listy nazw narzędzi wpisanej w kod. Powód jest
 * zmierzony i mieszka w `src/sections/skills/mounted.test.tsx`: informacja o tym, które narzędzie
 * widzi umiejętność, ginie po tamtej stronie granicy, więc każde zdanie o vendorze w tej sekcji
 * jest zdaniem o czymś, czego nikt tu nie wie. Lista zakazanych nazw jest CZYTANA
 * z `src-tauri/src/skills/mod.rs`, nie przepisana.
 *
 * RENDER JEST STATYCZNY, bo w repo nie ma `jsdom` — stąd dwie konsekwencje, obie widoczne niżej:
 * treść panelu i stan „pisze" muszą dać się ZASIAĆ w magazynie, a „oddanie pytania" jest
 * wywołaniem akcji magazynu, nie kliknięciem. Pierwsza z nich nie jest ustępstwem na rzecz testu:
 * odmowa musi zostawić zdanie człowieka na ekranie, więc pole i tak musi leżeć tam, gdzie ląduje
 * odmowa (niezmiennik 13).
 *
 * KONTRAKT NA MARKUP, żeby następny czytelnik nie musiał go zgadywać:
 *   data-add-panel            panel, który otwiera `data-create`. Jeden na ekran (T-42).
 *   data-what-you-want        pole na zdanie „czego chcesz". Jedno, z etykietą.
 *   data-pick-an-agent        wybór agenta. Jeden.
 *   data-agent="<id>"         jedna pozycja wyboru per ZAPISANY agent, jego identyfikatorem.
 *   data-ask-an-agent         kontrolka, która oddaje pytanie. Jedna — i NIE MA JEJ w stanie
 *                             pisania (podmiana, jak `Start`/`Stop` w `run/start.tsx`).
 *   data-writing              zdanie o tym, co się teraz dzieje. Jedno, bez animacji.
 *   data-stop-writing         kontrolka zatrzymania. Jedna, tylko w stanie pisania.
 * Trzy pola formularza zostają pod swoimi `data-question="<klucz>"` z T-42: draft ląduje
 * DOKŁADNIE w nich, bo zapis idzie tą samą drogą co tekst wpisany ręką.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { AddPanel, Authored, SavedAgent } from '../../state/skills';
import { useSkills } from '../../state/skills';
import { ipcSource, windowSideArguments } from '../ipc-signature';
import SkillsShelf from './shelf';

/* Atrapa granicy: liczy wywołania i odpowiada tym, o co poprosił dany test. Żadnego żywego
 * Tauri — kryterium, które go wymaga, nie umie być czerwone z właściwego powodu, bo
 * `Failed to launch` stoi na liście `NOT_A_REAL_RED` w bramce.
 *
 * ODMOWA JEST NAPISEM, NIE `Error`-em, i to nie jest drobiazg: skorupy komend robią
 * `.map_err(|e| e.to_string())` (`src-tauri/src/ipc.rs`), Tauri woła `reject(e)` z tym napisem,
 * a `@tauri-apps/api/core` przekazuje go dalej bez opakowania. Atrapa rzucająca `Error` sądziłaby
 * kształt, którego na tej granicy nie ma — dokładnie ta pomyłka kazała siedmiu miejscom
 * produkcyjnym pisać `error instanceof Error ? … : ''`, warunek zawsze fałszywy
 * (`src/ipc/why.ts`). */
const { invoked, answerWith, refuseWith } = vi.hoisted(() => {
  const answer = { gave: undefined as unknown, refusal: null as string | null };
  return {
    invoked: vi.fn((..._sent: unknown[]) =>
      answer.refusal === null ? Promise.resolve(answer.gave) : Promise.reject(answer.refusal),
    ),
    answerWith: (gave: unknown): void => {
      answer.gave = gave;
    },
    refuseWith: (said: string | null): void => {
      answer.refusal = said;
    },
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');

/** Pliki czytamy tak, żeby test padał na asercji o treści, nigdy na otwarciu pliku. */
function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Ta sama lista, którą po drugiej stronie granicy czyta `ipc_commands_registered.rs`. */
const known = new Set(
  fileText(resolve(ROOT, 'src-tauri/commands.golden.txt'))
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('#')),
);

/** `ipc.rs` w całości — jedyne miejsce, w którym stoją nazwy argumentów komend. */
const rust = ipcSource();

/**
 * Nazwy komend, które woła sekcja Praca — czytane z JEJ krawędzi, nie przepisane.
 *
 * Po co: `stop_run(state)` i `stop_draft(state)` mają na drucie identyczny kształt (sam `State`,
 * zero argumentów od okna), więc żadna asercja o nazwach argumentów nie odróżni Stopu draftu od
 * Stopu biegu. A pomyłka jest tu jednokierunkowa i głośna: Stop w panelu umiejętności ubija bieg
 * w sąsiedniej karcie. Po stronie Rusta te uchwyty są rozdzielone z tego samego powodu —
 * `AppState.live` jest PODMIENIANY przy każdym Starcie, więc draft trzymający się tamtego tokena
 * traci go, gdy człowiek zacznie bieg gdzie indziej.
 */
const WORK_SECTION = new Set(
  [...fileText(resolve(ROOT, 'src/sections/run/io.ts')).matchAll(/invoke[^(]*\(\s*'([a-z_]+)'/g)]
    .map((found) => found[1] ?? '')
    .filter((name) => name !== ''),
);

/**
 * Nazwy narzędzi agentowych, o których ta sekcja nie ma prawa nic twierdzić.
 *
 * Czytane z `VENDORS` w `src-tauri/src/skills/mod.rs`, pierwszym słowem każdej pozycji — tak
 * samo, jak robi to `mounted.test.tsx`. Przepisana lista rozjechałaby się w dniu, w którym
 * dołożymy vendora, i rozjechałaby się w stronę CISZY: zakaz przestałby dotyczyć tego jednego,
 * o który właśnie chodzi.
 */
const VENDORS: readonly string[] = (
  /pub const VENDORS[^=]*=\s*\[([\s\S]*?)\]\s*;/.exec(
    fileText(resolve(ROOT, 'src-tauri/src/skills/mod.rs')),
  )?.[1] ?? ''
)
  /* Pozycje stoją w cudzysłowach, więc po rozcięciu po nich treścią są kawałki NIEPARZYSTE.
     „Claude Code" schodzi do „Claude": zakazane jest twierdzenie o narzędziu, a nie słowo
     „Code", które w tym repo znaczy też zwykły kod. */
  .split('"')
  .filter((_piece, at) => at % 2 === 1)
  .map((vendor) => vendor.split(' ')[0] ?? '')
  .filter((word) => word !== '');

/** Dwaj agenci zapisani na dysku. Ani jedno imię nie jest nazwą vendora — patrz asercja niżej. */
const FORGE: SavedAgent = { id: '0198a1f2-3b4c-7d5e-8f60-112233445566', name: 'Forge' };
const SCRIBE: SavedAgent = { id: '0198a1f2-3b4c-7d5e-8f60-99887766aabb', name: 'Scribe' };
const SAVED: readonly SavedAgent[] = [FORGE, SCRIBE];

/**
 * Zdanie, które napisał człowiek.
 *
 * ANI JEDNEGO APOSTROFU ANI CUDZYSŁOWU w tym zdaniu i we wszystkich niżej, i to nie jest
 * przypadek: React ucieka `'` na `&#x27;` we wszystkim, co renderuje, więc `toContain` na tekście
 * z apostrofem byłby czerwony także wtedy, gdy ekran pokazuje dokładnie to, co trzeba. Klasa
 * pułapki jest ta sama, co „test sprawdza obecność stringa": mierzy kodowanie, nie zachowanie.
 */
const WANT = 'A skill that reviews a pull request and says in one paragraph what to fix.';

/** Trzy pola, które oddał model. Tego samego kształtu, co trzy odpowiedzi z formularza T-42. */
const DRAFTED: Authored = {
  name: 'Review pull requests',
  whenToUse: 'Use this when somebody asks for a second look at a pull request.',
  whatToDo: 'Read the change first, then say in one paragraph what to fix.',
};

/** Trzy pytania, kluczami magazynu — tymi samymi, którymi markup je nazywa. */
const QUESTIONS = ['name', 'whenToUse', 'whatToDo'] as const;

/** Panel otwarty i jeszcze pusty: draft ma trafić w PUSTE pola, nie dopisać się do czegoś. */
const NOTHING_TYPED: AddPanel = { link: '', name: '', whenToUse: '', whatToDo: '' };

/** Odmowa napisana po tamtej stronie granicy. Walidatorowa w kształcie i bez apostrofu. */
const REFUSED = 'The agent came back with something that is not a skill: no name in it';

/**
 * Co pytanie ma nieść przez granicę, klucz w klucz.
 *
 * Klucze są tu WYPISANE, i to jest jedyne miejsce w tym pliku, gdzie cokolwiek jest przepisane
 * z `ipc.rs`. Dlatego test porównuje tę połowę z podpisem komendy w tym samym biegu: parowanie,
 * którego klucze nikt nie sprawdza, przechodzi także wtedy, gdy Rust dawno przemianował argument.
 */
const CARRIES: readonly [string, string][] = [
  ['want', WANT],
  ['agent', FORGE.id],
];

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/**
 * Otwierający znacznik elementu niosącego ten atrybut, albo `''`.
 *
 * Pusty napis, a nie wyjątek: wołający ma o niego zapytać JAWNIE i powiedzieć, czego zabrakło.
 * `expect(undefined).not.toContain(…)` przechodzi dla elementu, którego nie ma.
 */
function tagWith(markup: string, attribute: string): string {
  const at = markup.indexOf(attribute);
  if (at < 0) return '';
  const opens = markup.lastIndexOf('<', at);
  const closes = markup.indexOf('>', at);
  return opens < 0 || closes < 0 ? '' : markup.slice(opens, closes + 1);
}

function attributeOf(tag: string, name: string): string {
  return new RegExp(name + '="([^"]*)"').exec(tag)?.[1] ?? '';
}

/** Tekst etykiety wskazującej ten `id`, bez odstępów po brzegach. */
function labelFor(markup: string, id: string): string {
  const found = new RegExp('<label[^>]*for="' + id + '"[^>]*>([^<]*)<', 'i').exec(markup);
  return (found?.[1] ?? '').trim();
}

/** Sam tekst tego kawałka markupu, bez znaczników i bez nadmiarowych odstępów. */
function words(part: string): string {
  return part
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * To, co człowiek naprawdę CZYTA: bez znaczników i z rozkodowanymi encjami.
 *
 * DLACZEGO ENCJE. React ucieka `'` na `&#x27;` i `"` na `&quot;` we wszystkim, co renderuje, więc
 * `toContain` na zdaniu z apostrofem jest czerwone także wtedy, gdy ekran pokazuje dokładnie to
 * zdanie. Własne napisy tego pliku apostrofów nie mają z rozmysłu — ale zdanie odmowy pisze
 * IMPLEMENTACJA, a kryterium, które zabrania apostrofu w zdaniu dla człowieka, mierzy kodowanie,
 * nie zachowanie (niezmiennik 20).
 */
function visible(markup: string): string {
  return words(markup)
    .replaceAll('&#x27;', "'")
    .replaceAll('&quot;', '"')
    .replaceAll('&lt;', '<')
    .replaceAll('&gt;', '>')
    .replaceAll('&amp;', '&');
}

/** Ile słów. Zdanie ma ich kilka; kod błędu albo jedno słowo statusu — jedno. */
function howManyWords(text: string): number {
  return text.split(' ').filter((word) => word !== '').length;
}

/** Tekst WEWNĄTRZ elementu niosącego ten atrybut. `''`, kiedy elementu nie ma. */
function textIn(markup: string, attribute: string): string {
  const tag = tagWith(markup, attribute);
  if (tag === '') return '';
  const from = markup.indexOf(tag) + tag.length;
  const to = markup.indexOf('</', from);
  return to < 0 ? '' : words(markup.slice(from, to));
}

/**
 * Co stoi w kontrolce niosącej ten atrybut — jej WARTOŚĆ, nie „gdzieś w markupie".
 *
 * DWIE GAŁĘZIE, BO SĄ DWA RODZAJE KONTROLEK. `<input>` niesie wartość atrybutem `value`,
 * a `<textarea>` — TREŚCIĄ, bo React przepisuje `value` na dzieci. Bez drugiej gałęzi pole na
 * akapit wyglądałoby na puste dokładnie wtedy, gdy jest pełne, a kryterium byłoby czerwone przy
 * poprawnym ekranie. Wybór między `<input>` a `<textarea>` należy do ekranu i ma należeć: „co
 * zrobić" jest akapitem, a zdanie „czego chcesz" bywa jednym i drugim.
 */
function valueIn(markup: string, attribute: string): string {
  const tag = tagWith(markup, attribute);
  if (tag === '') return '';
  /* Element pusty (`<input … />`) nie ma treści, więc jego wartością jest atrybut — także wtedy,
     gdy atrybut jest pusty. Czytanie „dzieci" takiego znacznika złapałoby tekst SĄSIADA. */
  if (tag.endsWith('/>')) return attributeOf(tag, 'value');
  const written = attributeOf(tag, 'value');
  if (written !== '') return written;
  const from = markup.indexOf(tag) + tag.length;
  const to = markup.indexOf('</', from);
  return to < 0 ? '' : markup.slice(from, to);
}

/** Co stoi w kontrolce tego z trzech pytań formularza (T-42). */
function answerFor(markup: string, key: string): string {
  return valueIn(markup, 'data-question="' + key + '"');
}

/**
 * Każda para (klucz, wartość) w środku, na dowolnym poziomie zagnieżdżenia.
 *
 * PARY, NIE SAME WARTOŚCI, i to jest cała treść tej funkcji. Zbiór samych wartości odpowiada
 * wyłącznie na pytanie „czy ten napis gdziekolwiek pojechał" — a wtedy wywołanie, które wysłało
 * zdanie człowieka pod kluczem agenta i identyfikator agenta pod kluczem zdania, przechodzi:
 * oba napisy są w ładunku, oba pod właściwie NAZWANYMI kluczami, tylko zamienione. Po drugiej
 * stronie granicy to jest odmowa „no agent saved here has the id …" przy każdym pytaniu, i to
 * odmowa, która wygląda na zepsuty wybór agenta, a nie na zamienione klucze.
 */
function pairs(value: unknown, into: [string, unknown][]): [string, unknown][] {
  if (Array.isArray(value)) {
    for (const item of value as unknown[]) pairs(item, into);
  } else if (typeof value === 'object' && value !== null) {
    for (const [key, item] of Object.entries(value as Record<string, unknown>)) {
      into.push([key, item]);
      pairs(item, into);
    }
  }
  return into;
}

function screen(): string {
  return renderToStaticMarkup(<SkillsShelf store={useSkills} />);
}

beforeEach(() => {
  /* Magazyn umiejętności jest singletonem, więc zasianie go w jednym teście dojechałoby do
   * następnego. Stan pusty przed każdym: kolejność testów przestaje mieć znaczenie. */
  useSkills.setState({
    pending: null,
    acknowledged: [],
    message: null,
    installed: [],
    adding: null,
    agents: [],
    want: '',
    chosenAgent: '',
    writing: false,
  });
  answerWith(undefined);
  refuseWith(null);
  invoked.mockClear();
});

describe('a person says what they want, an agent writes it, and the person reads it first', () => {
  it('the panel carries a third way in: one sentence and a choice of a SAVED agent', () => {
    const closed = screen();
    expect(
      occurrences(closed, 'data-what-you-want'),
      'the third entry is in the document before anybody asked to add anything. It belongs INSIDE ' +
        'the panel that data-create opens — a form standing open on the empty screen is a second ' +
        'invitation, and then the screen has two answers to one question (invariant 13)',
    ).toBe(0);

    useSkills.setState({ adding: NOTHING_TYPED, agents: [...SAVED] });
    const markup = screen();

    expect(
      occurrences(markup, 'data-add-panel'),
      'the panel a person types into is not in the document, or it is there twice. The third ' +
        'entry lives in the SAME panel as the link and the form: adding a skill is one decision ' +
        'with three answers, not three decisions',
    ).toBe(1);

    const field = tagWith(markup, 'data-what-you-want');
    expect(
      field,
      'the panel carries no field for the sentence a person writes. This is the state the section ' +
        'is in today: two ways in, both of which need the person to write the whole skill ' +
        'themselves, while Loadout drives two agent vendors and can already prove the group it ' +
        'started is dead',
    ).not.toBe('');

    const id = attributeOf(field, 'id');
    expect(id, 'the field for the sentence has no id, so no label can point at it').not.toBe('');
    expect(
      labelFor(markup, id),
      'the field for the sentence has no label with words in it. A field whose meaning a person ' +
        'has to guess is a field they fill in wrong once and never again',
    ).not.toBe('');

    expect(
      occurrences(markup, 'data-pick-an-agent'),
      'there is no way to pick WHO writes it, or there are two. The model, the system prompt and ' +
        'the safety dial all come from the saved definition of the chosen agent ' +
        '(library::agents::resolve), so the choice is the whole of what this entry needs besides ' +
        'the sentence',
    ).toBe(1);

    for (const agent of SAVED) {
      expect(
        markup,
        'the choice does not offer the saved agent ' +
          agent.name +
          ' under its own id. The id is what travels to Rust: a choice keyed by name would ' +
          'refuse for every person who renamed an agent, and a choice keyed by position would ' +
          'silently ask a different one',
      ).toContain('data-agent="' + agent.id + '"');
      expect(
        words(markup),
        'the choice carries the id of ' +
          agent.name +
          ' and not its name, so a person picks between two identifiers. The name is the only ' +
          'part of a saved agent a person recognises',
      ).toContain(agent.name);
    }

    expect(
      occurrences(markup, 'data-agent="'),
      'the choice offers a different number of agents than the store knows about. The entries ' +
        'come from the store — a list written into the code is a list that is wrong on the first ' +
        'machine where somebody saved a third agent, and it is how a vendor name gets onto this ' +
        'screen',
    ).toBe(SAVED.length);

    /* KONTROLA PRZECIW PUSTEJ ASERCJI, i bez niej ta negatywna niżej przechodzi na liście
       vendorów, której nie udało się przeczytać — czyli zawsze. */
    expect(
      VENDORS.length,
      'the VENDORS list could not be read out of src-tauri/src/skills/mod.rs, so "no vendor is ' +
        'named on this screen" would pass for every name there is',
    ).toBeGreaterThan(0);

    const named = VENDORS.filter((vendor) => words(markup).includes(vendor));
    expect(
      named,
      'this section named a tool: ' +
        named.join(', ') +
        '. The choice is a choice of a SAVED AGENT, and which tool an agent runs on is Rust ' +
        'business (runsWith in the saved definition). mounted.test.tsx freezes the absence of ' +
        'these names in this markup and has a measured reason: on the owner disk the old "Ready ' +
        'for Claude and Codex" was false for all ten skills',
    ).toEqual([]);
  });

  it('handing the sentence over reaches Rust once, under a name off the golden list', async () => {
    /* Dwie kontrole przeciw porównaniu niczego z niczym. Obie stoją TU, a nie w osobnym teście:
       oracle przeczytany w innym `it` niczego nie mówi o tym, w którym stoi asercja. */
    expect(
      known.size,
      'src-tauri/commands.golden.txt could not be read, so "the window asked for a name off the ' +
        'list" would pass for every name there is',
    ).toBeGreaterThan(0);
    expect(
      rust,
      'src-tauri/src/ipc.rs could not be read, so the expected set of argument names would come ' +
        'from nowhere and the comparison would pass on two empty lists',
    ).not.toBe('');

    useSkills.setState({
      adding: NOTHING_TYPED,
      agents: [...SAVED],
      want: WANT,
      chosenAgent: FORGE.id,
    });
    answerWith(DRAFTED);

    await useSkills.getState().askAnAgent();

    expect(
      invoked.mock.calls.length,
      'the question never left the window. Every way of running an agent in this application goes ' +
        'through a workflow file, a run folder, a graph of steps and the scheduler — and a skill ' +
        'a person ' +
        'wants is not a run: it is one turn, one prompt, one answer. Exactly one call, because ' +
        'more than one means the same sentence is paid for twice',
    ).toBe(1);

    const sent = invoked.mock.calls.at(0);
    if (sent === undefined) {
      throw new Error('the question never reached Rust at all');
    }

    const asked = sent.at(0);
    expect(
      typeof asked === 'string' && known.has(asked),
      'the window asked Rust for ' +
        String(asked) +
        ', which is not on src-tauri/commands.golden.txt — so nothing on the Rust side keeps that ' +
        'name alive, and the day it is renamed this call goes quiet. The name is read out of that ' +
        'file here, never typed into this test',
    ).toBe(true);

    const payload = sent.at(1);
    const carried =
      typeof payload === 'object' && payload !== null ? (payload as Record<string, unknown>) : {};
    const wanted = [...windowSideArguments(rust, String(asked))].sort();
    expect(
      wanted.length,
      'no signature for ' +
        String(asked) +
        ' could be parsed out of src-tauri/src/ipc.rs, so the key comparison below would be ' +
        'nothing against nothing — the exact shape of green this criterion exists to end',
    ).toBeGreaterThan(0);
    expect(
      Object.keys(carried).sort(),
      'the window sends ' +
        JSON.stringify(Object.keys(carried).sort()) +
        ' and the command takes ' +
        JSON.stringify(wanted) +
        ' (read out of src-tauri/src/ipc.rs in this run). Tauri matches invoke arguments BY NAME ' +
        'and deserializes them before the command body runs, so a key that does not line up is ' +
        'not a smaller call — it is a rejected one, and the refusal arrives as a raw string ' +
        'nobody sees',
    ).toEqual(wanted);

    /* PAROWANIE JEST WYPISANE TU, A JEGO POŁOWA KLUCZOWA SPRAWDZONA WOBEC `ipc.rs` w tym samym
       biegu. Bez tego porównania pary siedziałyby na nazwach przepisanych z pliku, którego nikt
       nie czytał — a klucz, którego żadna asercja nie pilnuje, jest kluczem, który wolno
       zamienić za darmo. */
    expect(
      CARRIES.map(([key]) => key).sort(),
      'this test pins the pairs under ' +
        JSON.stringify(CARRIES.map(([key]) => key).sort()) +
        ' and ' +
        String(asked) +
        ' takes ' +
        JSON.stringify(wanted) +
        '. One of the two moved, so the pairing below would be watching a key the command no ' +
        'longer has',
    ).toEqual(wanted);

    const sentPairs = pairs(carried, []);
    const lost = CARRIES.filter(
      ([key, value]) => !sentPairs.some(([named, said]) => named === key && said === value),
    ).map(([key]) => key);
    expect(
      lost,
      'the call reached Rust and did not carry what it was given under ' +
        JSON.stringify(lost) +
        '. Either the value is missing — a call that arrives without the sentence is the same ' +
        'silence as no call at all, and the weak version of this criterion (a button with writing ' +
        'on it is in the markup) passes on exactly that — or it travelled under the OTHER key, ' +
        'which reads on screen as a broken choice of agent and is in fact two swapped keys',
    ).toEqual([]);
  });

  it('while an agent writes, the screen says so, offers Stop, and drops the ask', () => {
    useSkills.setState({ adding: NOTHING_TYPED, agents: [...SAVED], want: WANT, writing: true });
    const markup = screen();

    const said = textIn(markup, 'data-writing');
    expect(
      said,
      'nothing on the screen says what is happening while the model writes. Silence after a ' +
        'control looks exactly like a broken control: the person presses it a second time, and ' +
        'here the second press costs another turn of an agent somebody is paying for',
    ).not.toBe('');
    expect(
      howManyWords(said),
      'the live region says "' +
        said +
        '", which is not a sentence. A person reads a sentence and knows to wait; a word reads ' +
        'like a status code, and this section has no jargon by rule (invariant 14)',
    ).toBeGreaterThanOrEqual(4);

    expect(
      occurrences(markup, 'data-stop-writing'),
      'the writing state has no way out, or it has two. A turn that cannot be stopped is a turn ' +
        'that keeps burning the paid allowance of this person while they watch — invariant 6 ' +
        'calls that a financial error, not a hygiene one',
    ).toBe(1);

    expect(
      occurrences(markup, 'data-ask-an-agent'),
      'the control that asks for a draft is still on screen while a draft is being written. It is ' +
        'a SWAP, the same one Start and Stop make in src/sections/run/start.tsx: a second press ' +
        'starts a second turn, and the first one is still running and still being paid for. This ' +
        'assertion is guarded by the one above — a screen that renders neither control fails there',
    ).toBe(0);

    expect(
      /\banimate-/.test(markup),
      'something on this screen animates. DESIGN §7 has exactly one animation in the whole ' +
        'application — the dot of the live run card, animate-blip — and a second one teaches the ' +
        'eye to chase movement instead of reading the sentence next to it',
    ).toBe(false);
  });

  it('Stop leaves the window too, with the second name off the same list', async () => {
    useSkills.setState({
      adding: NOTHING_TYPED,
      agents: [...SAVED],
      want: WANT,
      chosenAgent: FORGE.id,
    });
    answerWith(DRAFTED);

    await useSkills.getState().askAnAgent();
    /* Stan zasiany, żeby zatrzymanie padało na coś, co pisze: gdyby akcja pilnowała `writing`,
       test o wychodzeniu z okna sądziłby tę straż, a nie zatrzymanie. */
    useSkills.setState({ writing: true });
    await useSkills.getState().stopWriting();

    expect(
      invoked.mock.calls.length,
      'writing and stopping are TWO calls across the seam and the window made ' +
        String(invoked.mock.calls.length) +
        '. A Stop that only clears a flag in this store is the control this criterion exists to ' +
        'refuse: the agent on the other side keeps writing and keeps being paid for, and the ' +
        'screen reports it stopped (invariants 6, 10 and 16)',
    ).toBe(2);

    const names = invoked.mock.calls.map((call) => String(call.at(0)));
    const strangers = names.filter((name) => !known.has(name));
    expect(
      strangers,
      'these names are not on src-tauri/commands.golden.txt: ' +
        strangers.join(', ') +
        '. The list is the one place where both sides of the seam agree on a name',
    ).toEqual([]);
    expect(
      new Set(names).size,
      'writing and stopping went out under the same name, ' +
        names.join(' and ') +
        '. Stop here has to be its own command: one command for both would mean Stop in this ' +
        'section kills the run in the next tab',
    ).toBe(2);

    const stopped = invoked.mock.calls.at(1);
    if (stopped === undefined) {
      throw new Error('stopping never reached Rust at all');
    }
    const stopName = String(stopped.at(0));

    /* KONTROLA PRZECIW PUSTEMU ZBIOROWI: bez niej „Stop tutaj nie jest Stopem sekcji Praca"
       przechodzi na pliku, którego nie udało się przeczytać, czyli zawsze. */
    expect(
      WORK_SECTION.size,
      'src/sections/run/io.ts could not be read, so the assertion below — that stopping a draft ' +
        'is not the command that stops a RUN — would pass for every name there is',
    ).toBeGreaterThan(0);
    expect(
      WORK_SECTION.has(stopName),
      'stopping a draft went out as ' +
        stopName +
        ', which is a command the Work section calls (read out of src/sections/run/io.ts). Then ' +
        'Stop in this panel reaches into the run in the next tab and kills it — and the two ' +
        'handles are genuinely separate on the Rust side: AppState.live is REPLACED on every ' +
        'Start, which is the whole reason the draft carries a stop handle of its own',
    ).toBe(false);

    expect(
      Object.keys((stopped.at(1) ?? {}) as Record<string, unknown>).sort(),
      'stopping sends keys ' +
        JSON.stringify(Object.keys((stopped.at(1) ?? {}) as Record<string, unknown>).sort()) +
        ' to ' +
        stopName +
        ', which takes ' +
        JSON.stringify([...windowSideArguments(rust, stopName)].sort()) +
        ' (read out of src-tauri/src/ipc.rs). A key that does not line up does not make a smaller ' +
        'call, it makes a rejected one — and a rejected Stop leaves the agent writing',
    ).toEqual([...windowSideArguments(rust, stopName)].sort());
  });

  it('the draft lands in the three fields of the form, editable, and nothing is saved', async () => {
    useSkills.setState({
      adding: NOTHING_TYPED,
      agents: [...SAVED],
      want: WANT,
      chosenAgent: FORGE.id,
    });
    answerWith(DRAFTED);

    await useSkills.getState().askAnAgent();
    const markup = screen();

    for (const key of QUESTIONS) {
      expect(
        answerFor(markup, key),
        'the answer the model wrote for "' +
          key +
          '" is not in the control that asks that question. The draft lands in the SAME three ' +
          'fields a person types into (T-42): text a person can read but not correct is text they ' +
          'have to retype, and a draft saved without being read is exactly the thing this entry ' +
          'must not be',
      ).toBe(DRAFTED[key]);

      const tag = tagWith(markup, 'data-question="' + key + '"');
      expect(
        /\b(disabled|readonly)\b/i.test(tag),
        'the control for "' +
          key +
          '" carries the draft and will not take a correction. A person reads a draft in order to ' +
          'change it; a read-only field turns the review into a decision between all of it and ' +
          'none of it',
      ).toBe(false);
    }

    expect(
      invoked.mock.calls.length,
      'the window crossed the seam ' +
        String(invoked.mock.calls.length) +
        ' times for one question. One is the question; a second one is a save nobody asked for. ' +
        'The draft is text from a model and it goes to disk the same way hand-typed text does — ' +
        'through the form, so what the person corrected is what gets composed, scanned and kept ' +
        '(invariant 23)',
    ).toBe(1);
    expect(
      useSkills.getState().pending,
      'a review is waiting, so the draft went down the saving road on its own. Nothing is saved ' +
        'until the person hands the three fields over themselves: text scanned before the person ' +
        'corrected it is text nobody scanned',
    ).toBeNull();
    expect(
      useSkills.getState().writing,
      'the draft arrived and the screen still says an agent is writing. A live region that never ' +
        'goes out is a live region nobody believes the next time (invariant 13)',
    ).toBe(false);
  });

  it('a refusal from the other side puts a sentence on screen and keeps the sentence typed', async () => {
    useSkills.setState({
      adding: NOTHING_TYPED,
      agents: [...SAVED],
      want: WANT,
      chosenAgent: FORGE.id,
    });
    refuseWith(REFUSED);

    await useSkills.getState().askAnAgent();
    const markup = screen();

    expect(
      visible(markup),
      'Rust refused with a sentence that says what happened and the screen does not show it. ' +
        'A vendor that is not installed, a model that came back with something that is not a ' +
        'skill — these are different things to do next, and one silence for all of them is the ' +
        'shape of a broken control',
    ).toContain(REFUSED);

    expect(
      valueIn(markup, 'data-what-you-want'),
      'the sentence the person wrote is gone from the field after a refusal. Text lost on a ' +
        'refusal is the same defect as silence, only more expensive: they write the sentence, ' +
        'read one line about a vendor, and have to write the sentence again',
    ).toBe(WANT);

    expect(
      useSkills.getState().writing,
      'the refusal left the screen saying an agent is writing. Then Stop is the only control on ' +
        'screen, it has nothing to stop, and the way back to asking again is gone',
    ).toBe(false);
  });

  it('with no agent saved it refuses with a sentence and still keeps what was typed', async () => {
    useSkills.setState({ adding: NOTHING_TYPED, agents: [], want: WANT });

    await useSkills.getState().askAnAgent();
    const markup = screen();

    const said = useSkills.getState().message;
    expect(
      said,
      'there is nobody saved to ask and the window said nothing about it. This is the first day ' +
        'on a fresh machine: the library of agents is empty, and an entry that answers with ' +
        'silence reads exactly like an entry that is broken',
    ).not.toBeNull();
    expect(
      howManyWords(said ?? ''),
      'the refusal is "' +
        String(said) +
        '", which is not a sentence a person can act on. It has to say what to do next — save an ' +
        'agent — because that is a thing they can go and do',
    ).toBeGreaterThanOrEqual(4);
    expect(
      visible(markup),
      'the refusal is in the store and not on the screen. A store that knows and a screen that ' +
        'does not is the same as not knowing (invariant 13)',
    ).toContain(said);
    expect(
      valueIn(markup, 'data-what-you-want'),
      'the sentence the person wrote is gone from the field. They wrote it before there was ' +
        'anybody to ask; the way out is to save an agent and press the same control again, and ' +
        'that costs nothing only if the sentence is still there',
    ).toBe(WANT);
  });
});
