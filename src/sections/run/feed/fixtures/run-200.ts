/* Skrypt biegu: 200 zdarzeń, czterech agentów — scena, na której mierzy się widok pracy.
 *
 * Dlaczego skrypt, a nie prawdziwe zdarzenia z Rusta: model musi dać się przetestować bez okna
 * i bez agenta. Kanał IPC dostarcza T-07; gdyby kryteria tego zadania czekały na żywy bieg,
 * mierzyłyby dwie rzeczy naraz i milczały o tym, która z nich padła.
 *
 * Trzy rzeczy w tym skrypcie są WIĄŻĄCE, bo kryteria się o nie opierają:
 *   - komplet rodzajów i ich liczności (kryterium 1 wypisuje je co do sztuki),
 *   - wszyscy czterej agenci pojawiają się w pierwszej dziesiątce, więc strefa TERAZ ma cztery
 *     wiersze już po pierwszej paczce — inaczej „cztery po każdej z dwudziestu paczek" mierzyłoby
 *     tempo pojawiania się agentów, a nie kształt strefy,
 *   - jedyne pytanie do człowieka stoi na pozycji `ASKED_AT`, żeby kryterium 7 mogło przepuścić
 *     przez widok 160 dalszych zdarzeń i wymagać, że przypięcie się nie ruszyło.
 *
 * Kolejność reszty jest rozłożona równomiernie i DETERMINISTYCZNIE: każda linia dostaje pozycję
 * `(i + 0.5) / n` w swoim rodzaju i wszystko sortuje się po tej pozycji. Rzadki rodzaj ląduje
 * wtedy w środku biegu, a nie na jego końcu — gdyby `problem` wypadło zdarzeniem 200, kryterium 7
 * przepuszczałoby przez widok 160 zdarzeń, wśród których nie ma ani jednego wartego przykrycia
 * pytania, i przechodziłoby dla implementacji, która przypina po prostu ostatnią linię.
 * Losowania tu nie ma i być nie może: scena, która zmienia się między biegami, zamienia czerwień
 * w rzut monetą.
 */
import type { FeedLine } from '../../../../state/run';
import { line } from './lines';

/** Czterej agenci tego biegu. Kolory tożsamości nadaje widok, nie skrypt. */
export const AGENTS = ['Forge', 'Needle', 'Rivet', 'Anvil'] as const;

/** Którym z kolei zdarzeniem jest pytanie do człowieka (licząc od jednego). */
export const ASKED_AT = 40;

/** Odstęp między zdarzeniami. Wystarczająco gęsto, żeby okno sklejania miało co robić. */
const TICK_MS = 137;

/** Sześć pierwszych zdarzeń stoi na sztywno: nagłówek, czterech agentów, pierwszy krok. */
const PREFIX = 6;

type ScriptKind =
  | 'thinking'
  | 'read'
  | 'search'
  | 'edit'
  | 'ran'
  | 'note'
  | 'handoff'
  | 'step'
  | 'memory'
  | 'asked'
  | 'problem';

/** Reszta skryptu: rodzaj i ile go jeszcze zostało po prefiksie. Razem 194. */
const TAIL: ReadonlyArray<readonly [ScriptKind, number]> = [
  ['thinking', 63],
  ['read', 60],
  ['search', 20],
  ['edit', 18],
  ['ran', 12],
  ['note', 10],
  ['handoff', 4],
  ['step', 2],
  ['memory', 3],
  ['asked', 1],
  ['problem', 1],
];

const PATHS = ['src/parser.rs', 'src/main.rs', 'tests/csv.rs', 'src/header.rs', 'src/quote.rs'];
const QUERIES = ['the login check', 'the CSV header', 'quoted commas'];
const STEPS = ['Build', 'Check'];
const NOTES = [
  'The parser trips on a quoted comma.',
  'Two rows carry a stray quote.',
  'I will keep the header row as it is.',
];
const MEMOS = ['csv-conventions.md', 'parser-notes.md', 'header-rules.md'];
const RUNS = ['Ran tests', 'Ran the build', 'Ran the parser on a sample'];

/** Wybór z listy, który zawsze coś oddaje — indeks liczony w kółko. */
function pick(items: readonly string[], i: number): string {
  return items[i % items.length] ?? '';
}

/** Agent numer `i`, liczony w kółko. Sceny testowe biorą nazwy stąd, a nie z indeksu tablicy. */
export function agentAt(i: number): string {
  return pick(AGENTS, i);
}

const MAKE: Readonly<Record<ScriptKind, (id: number, at: number, agent: string) => FeedLine>> = {
  thinking: (id, at, agent) => line.thinking(id, at, agent),
  read: (id, at, agent) => line.read(id, at, agent, pick(PATHS, id)),
  search: (id, at, agent) => line.search(id, at, agent, pick(QUERIES, id), 12),
  edit: (id, at, agent) => line.edit(id, at, agent, pick(PATHS, id), 12, 4),
  ran: (id, at, agent) => line.ran(id, at, agent, pick(RUNS, id), true, ['40 rows, no problems']),
  note: (id, at, agent) => line.note(id, at, agent, pick(NOTES, id)),
  handoff: (id, at, agent) => line.handoff(id, at, agent, agent + ' → ' + pick(AGENTS, id)),
  step: (id, at, agent) => line.step(id, at, agent, pick(STEPS, id)),
  memory: (id, at, agent) => line.memory(id, at, agent, pick(MEMOS, id)),
  asked: (id, at, agent) =>
    line.asked(id, at, agent, 'Which database should I use?', ['Postgres', 'SQLite']),
  problem: (id, at, agent) => line.problem(id, at, agent, 'Could not reach the API'),
};

interface Planned {
  readonly kind: ScriptKind;
  readonly place: number;
  readonly rank: number;
}

/** Rodzaje zdarzeń 7…200, rozłożone równomiernie, z pytaniem wstawionym na swoje miejsce. */
function tailKinds(): ScriptKind[] {
  const planned: Planned[] = [];
  TAIL.forEach(([kind, n], rank) => {
    for (let i = 0; i < n; i += 1) planned.push({ kind, place: (i + 0.5) / n, rank });
  });
  planned.sort((a, b) => a.place - b.place || a.rank - b.rank);

  const order = planned.map((entry) => entry.kind);
  /* Pytanie wyjmujemy z równomiernego rozłożenia i wbijamy na twardą pozycję: kryterium 7
   * liczy 160 zdarzeń PO nim, więc ta liczba nie może zależeć od arytmetyki sortowania. */
  order.splice(order.indexOf('asked'), 1);
  order.splice(ASKED_AT - 1 - PREFIX, 0, 'asked');
  return order;
}

/** Świeży skrypt. Funkcja, nie stała: dwa testy porównujące tożsamość obiektów nie mogą ich dzielić. */
export function run200(): readonly FeedLine[] {
  const script: FeedLine[] = [];
  const at = (id: number): number => (id - 1) * TICK_MS;

  script.push(line.run(1, at(1), AGENTS[0], 'Fix the CSV parser'));
  AGENTS.forEach((name, i) => script.push(line.agent(i + 2, at(i + 2), name)));
  script.push(line.step(PREFIX, at(PREFIX), AGENTS[0], 'Research'));

  tailKinds().forEach((kind, i) => {
    const id = PREFIX + 1 + i;
    script.push(MAKE[kind](id, at(id), pick(AGENTS, i)));
  });

  return script;
}
