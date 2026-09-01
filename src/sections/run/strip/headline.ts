/* Nagłówek ekranu biegu — reguła `.rhead` z `docs/mockup/index.html`, policzona bez okna.
 *
 * CO RYSUJE MAKIETA, co do członu: nadoczko w akcencie („Running · started 2 min ago"), tytuł
 * biegu w stopniu bohatera (`h1.sm`, 34 px), jeden wiersz metadanej pod nim (workspace, liczba
 * kroków, liczba agentów, godzina startu) i grupa po prawej — wydatek z paskiem postępu, `Pause`,
 * `Stop`. Ten plik odpowiada za pierwsze trzy; `./head.tsx` je rysuje.
 *
 * DLACZEGO TO JEST MODEL, A NIE JSX. To repo nie ma jsdom, więc wszystko, co da się rozstrzygnąć
 * bez okna, ma się dać rozstrzygnąć bez okna: „co ekran mówi o tym biegu" jest wtedy funkcją,
 * którą kryterium woła wprost i porównuje napis do napisu (niezmiennik 15).
 *
 * TRZY RZECZY, KTÓRYCH TU NIE MA, I KAŻDA MA POWÓD:
 *
 *   1. „started 2 MIN AGO" z nadoczka makiety. Czas WZGLĘDNY jest prawdziwy wyłącznie w chwili
 *      renderu, a ten ekran przerysowuje się na wiersz strumienia — więc w sekundzie, w której
 *      bieg schodzi, napis zamarza i od tej chwili kłamie tym bardziej, im dłużej człowiek na
 *      niego patrzy. Godzina startu jest tym samym faktem bez zegara: stoi w nadoczku jako
 *      `started 09:41` i nie starzeje się nigdy.
 *   2. Godzina startu W DWÓCH miejscach. Makieta pisze ją i w nadoczku, i w wierszu metadanej;
 *      tutaj stoi raz (niezmiennik 13).
 *   3. Cokolwiek, kiedy nie wiadomo. Bieg, którego początku już nie mamy w oknie linii
 *      (`droppedBefore > 0`), nie ma godziny startu — i wtedy jej nie ma, zamiast podać godzinę
 *      najstarszej linii, która została (niezmiennik 17).
 */
import type { FeedLine, Step } from '../../../state/run';
import { clockOf } from '../feed/who';
import { spendFor, stepPhrase, stripFor } from './model';

/**
 * W jakim stanie jest ten bieg — trzy odpowiedzi, bo trzy różne rzeczy widzi człowiek.
 *
 * `live` bije, `ended` już nie, `idle` mówi o planie, który dopiero ruszy. Nadoczko bierze
 * z tego BARWĘ, a nie tylko słowo: kropka pulsująca nad biegiem, który zszedł, jest zdaniem
 * o pracy, której nikt nie wykonuje.
 */
export type RunTone = 'live' | 'ended' | 'idle';

export interface Headline {
  /** Stan biegu — nośnik barwy i pulsu, nie drugi napis. */
  readonly tone: RunTone;
  /** Nadoczko: „Running · started 09:41". Puste, kiedy nie ma o czym. */
  readonly eyebrow: string;
  /** Tytuł ekranu: nazwa biegu (albo workflow, który ruszy). Puste = nagłówka nie ma wcale. */
  readonly title: string;
  /** Jeden wiersz metadanej pod tytułem. Puste, kiedy nie wiadomo o tym biegu nic poza nazwą. */
  readonly meta: string;
  /** `4m 12s · $3.41 of $75` — dokładnie to, co dotąd stało w chipie paska. */
  readonly spend: string;
  /** Ile z sufitu wydatku poszło, 0..1. `null`, kiedy sufitu nie ma albo nikt nie podał ceny. */
  readonly used: number | null;
}

/** Czym karmimy nagłówek — same fakty, żadnej decyzji o wyglądzie. */
export interface RunFacts {
  /** Nazwa biegu, który idzie. Puste, kiedy nic nie biegnie. */
  readonly workflow: string;
  /** Nazwa workflow, który ruszy po naciśnięciu `Run`. Puste, kiedy nie ma czego uruchomić. */
  readonly nextUp: string;
  /** Kroki biegu w kolejności grafu — z magazynu biegu albo z pliku workflow. */
  readonly steps: readonly Step[];
  /** Okno linii strumienia, najstarsza pierwsza. */
  readonly lines: readonly FeedLine[];
  /** Ile linii wypadło z głowy okna: powyżej zera początku biegu już nie mamy. */
  readonly droppedBefore: number;
  /** Nazwa workspace, w którym ten bieg pracuje. `null`, kiedy nie wiadomo. */
  readonly workspace: string | null;
  /** Ilu agentów odezwało się w tym biegu. */
  readonly agents: number;
  /** Sufit wydatku tego biegu w dolarach; `null`, kiedy człowiek go nie postawił. */
  readonly budgetUsd: number | null;
}

/** Rozdzielacz członów, jeden na cały plik — makieta używa go i w nadoczku, i w metadanej. */
const DOT = ' · ';

/**
 * Godzina startu biegu — `09:41`, czasu lokalnego.
 *
 * `clockOf` z `../feed/who.ts`, a nie własne składanie: tamta funkcja jest jedynym miejscem,
 * które rozstrzyga, że zegar tej aplikacji jest lokalny i składany ręcznie (a nie przez
 * `toLocaleTimeString`, który oddaje inny napis przy innych ustawieniach systemu). Drugie
 * składanie tutaj byłoby drugą kopią tej decyzji (niezmiennik 13).
 *
 * SEKUNDY SCHODZĄ. Kolumna strumienia porównuje wiersze MIĘDZY SOBĄ i tam sekunda jest treścią;
 * nagłówek odpowiada na „o której to ruszyło" i sekunda jest w nim szumem.
 */
const HOUR_AND_MINUTE = 5;

function startedAt(facts: RunFacts): string {
  /* Początku, który wypadł z okna, nie zgadujemy: godzina najstarszej linii, jaka została, nie
   * jest godziną startu i nic na ekranie nie mówiłoby, że to nie to samo. */
  if (facts.droppedBefore > 0) return '';
  const first = facts.lines[0];
  if (first === undefined) return '';
  return clockOf(first.at).slice(0, HOUR_AND_MINUTE);
}

/**
 * Nagłówek dla tego biegu.
 *
 * TYTUŁEM JEST BIEG, NIE SEKCJA. Nazwa sekcji („Run") odpowiada na pytanie, w której części
 * aplikacji stoisz, i została tam, gdzie stała — w pasku, w stopniu 15 px. Ekran biegu ma
 * mówić, KTÓRY bieg pokazuje, a to jest inne pytanie i inna waga.
 *
 * KIEDY NIC NIE BIEGNIE, TYTUŁEM JEST WORKFLOW, KTÓRY RUSZY. Ta sama funkcja odpowiada
 * przyciskowi Start i obrazowi planu (`firstRunnable`), więc nagłówek, obraz i przycisk mówią
 * o jednym pliku. Kiedy nie ma nawet tego — nagłówka nie ma wcale, bo nie ma czego nazwać
 * (niezmiennik 17, DESIGN §6: nagłówek nad pustką obiecuje treść, która nigdy nie wejdzie).
 */
export function headlineFor(facts: RunFacts): Headline {
  const going = facts.workflow !== '';
  const title = going ? facts.workflow : facts.nextUp;
  const spend = spendFor(facts.lines, facts.budgetUsd);

  if (title === '') {
    return { tone: 'idle', eyebrow: '', title: '', meta: '', spend: '', used: null };
  }

  const { blocks } = stripFor(title, facts.steps, spend);
  const running = blocks.some((block) => block.state === 'now');
  /* „Skończony" znaczy: ten bieg ma za sobą kroki, a żaden już nie idzie. Bieg, który jeszcze
   * nie ruszył, ma wszystkie kroki w `todo` i nie jest skończony — dlatego pytamy o ślad
   * pracy, a nie o „nic nie biegnie". */
  const ended = !running && blocks.some((block) => block.state === 'done' || block.ended);

  const tone: RunTone = running ? 'live' : ended ? 'ended' : 'idle';
  const state = running ? 'Running' : ended ? 'Finished' : 'Ready to run';
  const when = startedAt(facts);

  return {
    tone,
    eyebrow: when === '' ? state : state + DOT + 'started ' + when,
    title,
    /* Metadana zawsze w tej samej kolejności, a człony, których nie znamy, po prostu nie
     * wchodzą: wiersz z „workspace —" mówi o folderze mniej niż wiersz bez niego. */
    meta: [
      facts.workspace === null || facts.workspace === '' ? '' : 'workspace ' + facts.workspace,
      stepPhrase(blocks, facts.steps),
      facts.agents === 0 ? '' : String(facts.agents) + (facts.agents === 1 ? ' agent' : ' agents'),
    ]
      .filter((part) => part !== '')
      .join(DOT),
    spend,
    used: usedOfCeiling(facts),
  };
}

/**
 * Ułamek sufitu, który ten bieg już wydał — nośnik paska postępu z makiety (`.meter i`).
 *
 * `null`, a nie zero, w trzech sytuacjach, bo to są trzy różne zdania i żadne z nich nie brzmi
 * „wydano 0%": nie ma sufitu, sufit jest zerowy (nie da się nim niczego podzielić), albo żadna
 * tura nie podała ceny. Pasek narysowany w każdej z nich pokazywałby pomiar, którego nie ma
 * (niezmiennik 17) — a to jest ten sam defekt, dla którego chip nigdy nie pisze `$0.00`.
 *
 * Przycięty do jedynki: bieg, który przekroczył sufit, ma pasek pełny, nie dłuższy od swojego
 * pudełka. Że przekroczył, mówi liczba obok — i mówi to dokładniej.
 */
function usedOfCeiling(facts: RunFacts): number | null {
  const ceiling = facts.budgetUsd;
  if (ceiling === null || ceiling <= 0) return null;

  let cost = 0;
  let priced = false;
  for (const line of facts.lines) {
    if (line.kind !== 'done' || line.costUsd === null) continue;
    cost += line.costUsd;
    priced = true;
  }
  if (!priced) return null;
  return Math.min(1, cost / ceiling);
}
