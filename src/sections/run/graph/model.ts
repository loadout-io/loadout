/* Cała polityka płótna biegu, w funkcjach czystych, wołalnych bez okna.
 *
 * DLACZEGO OSOBNY PLIK. To repo nie ma jsdom, a React Flow pod `renderToStaticMarkup` oddaje
 * ramę płótna z PUSTYMI pojemnikami na kafelki i strzałki — mierzy je dopiero w przeglądarce.
 * Wszystko, co dałoby się schować za tym pomiarem, stoi więc tutaj: co wolno narysować, gdzie
 * stoi kafelek, którą strzałką idzie teraz praca i co mówi czwarta linia. Ten sam podział, na
 * którym stoi edytor (`workflows/canvas/map.ts`), i z tego samego powodu.
 *
 * REGUŁA 17 JEST TREŚCIĄ TEGO PLIKU, nie komentarzem nad nim. Płótno jest legalne WYŁĄCZNIE
 * dlatego, że kroki, strzałki i pozycje stoją w pliku workflow. Kiedy ich nie ma — a plan
 * jednego kroku, który okno składa dla wpisanego pytania, ich nie ma i mieć nie może
 * (`../io.ts` stawia tam krok z trzech pól) — brak pola znaczy „nie wiemy", nigdy „nie ma".
 * Odpowiedzią jest wtedy lista kroków, a nie zgadnięty układ: pozycja wyliczona przez nas
 * wygląda dokładnie tak samo jak pozycja, którą człowiek ustawił, a różnicę widać dopiero
 * wtedy, gdy ktoś na niej oprze decyzję.
 */
import type { Link, Point } from '../../../state/workflows';
/* Napis powrotu bierzemy Z EDYTORA, a nie piszemy drugi raz. Graf biegu ma być TYM SAMYM
 * obiektem, co graf edytora — dwa zdania o jednej pętli rozjechałyby się przy pierwszej
 * zmianie liczby pojedynczej („up to 1 tries" czyta się jak usterka narzędzia). */
import { triesLabel } from '../../workflows/canvas/canvas';
import type { Question } from '../feed/model';
import type { AgentStatus } from '../rail/card';

/** Kto robi ten krok. `square` jest NAZWĄ tokenu tożsamości, nigdy hexem [DESIGN §9]. */
export interface Who {
  readonly name: string;
  readonly square: string;
}

/**
 * Krok biegu tak, jak widzi go płótno — i ani jednego pola więcej.
 *
 * `status` jest stanem AGENTA (`../rail/card.ts`), nie stanem kroku z `state/run.ts`: to
 * pierwsze ma sześć wartości i mówi, na czym stoi ten, kto pracuje; drugie ma siedem i mówi,
 * na czym stoi kafelek w silniku. Kafelek pokazuje człowieka przy pracy, więc bierze pierwsze.
 *
 * `who`, `doing` i `at` są opcjonalne, bo każde z nich potrafi być NIEZNANE, a nie puste:
 * agent, który jeszcze nic nie nadał, nie ma zdania; krok spoza pliku workflow nie ma pozycji.
 * Kafelek milczy o tym, czego nie dostał, zamiast rysować wypełniacz (niezmiennik 17).
 */
export interface GraphStep {
  readonly id: string;
  readonly name: string;
  readonly status: AgentStatus;
  readonly who?: Who;
  readonly doing?: string;
  readonly at?: Point;
  /**
   * Pytanie bez odpowiedzi, na którym stoi TEN krok — albo brak pola, kiedy stoi na żadnym.
   *
   * 2026-08-31 — DOSZŁO, BO PYTANIE MUSI MIEĆ MIEJSCE, NIE PODPIS. Karta „Needs your answer"
   * stała do dziś na dole kolumny strumienia, czyli po drugiej stronie ekranu od kafelka, który
   * zapytał; przy czterech krokach naraz jedyną rzeczą mówiącą, KTÓRY z nich czeka, był napis
   * małym stopniem. Miejsce odpowiada na to pytanie, podpis nie.
   *
   * TO JEST TA SAMA KARTA, nie druga: przewożone tędy `Question` jest obiektem z modelu
   * strumienia (`../feed/model.ts`), rysuje je ten sam komponent i odpowiedź jedzie tą samą
   * drogą. Dwa komplety przycisków na jedno pytanie to dwa miejsca, z których da się puścić
   * bieg, a pierwszy rozjazd między nimi jest cichy (niezmiennik 13).
   *
   * BRAK POLA ZNACZY „TEN KROK NIE PYTA", nigdy „nikt nie pyta". Pytanie, którego nie da się
   * przypisać do żadnego kroku — od lidera albo od pod-agenta rozpuszczonego w biegu — zostaje
   * tam, gdzie stało, a rozstrzyga to ekran, bo to on widzi oba miejsca naraz.
   */
  readonly asked?: Question;
}

/** Co płótno dostaje: kroki biegu i strzałki z pliku workflow. */
export interface Plan {
  readonly steps: readonly GraphStep[];
  readonly links: readonly Link[];
}

/** Dane kafelka dla React Flow.
 *
 * Alias typu, nie interfejs, i to nie jest gust: `Node<T>` żąda typu z indeksem łańcuchowym,
 * a interfejs go nie dostaje — `Node<GraphStep>` po prostu się nie kompiluje. Ten sam powód
 * i to samo opakowanie, co w `workflows/canvas/canvas.tsx`. */
export type TileData = { step: GraphStep };

export interface GraphTile {
  readonly id: string;
  /** Jeden rodzaj kafelka. Rodzaj kroku należy do pliku; bieg pokazuje pracę, nie kształt. */
  readonly type: 'step';
  readonly position: Point;
  readonly data: TileData;
}

export interface GraphArrow {
  readonly id: string;
  readonly source: string;
  readonly target: string;
  /**
   * Co ta strzałka o sobie mówi — powrót, praca, oba naraz albo nic.
   *
   * 2026-08-31 — POLE `animated` STĄD ZNIKŁO. Niosło jedyny fakt, jaki ta strzałka miała
   * o pracy, i niosło go RUCHEM. Powód zejścia stoi przy `LIVE_ARROW`.
   */
  readonly className?: string;
  readonly style?: { readonly strokeDasharray: string };
  readonly label?: string;
}

/** Czwarta linia kafelka: co ten krok czyta i czy cokolwiek czyta jego. */
export interface Measure {
  readonly waits: string;
  readonly handsOn: boolean;
}

/** Klasa powrotu. Regułę niesie `workflows/canvas/react-flow-tokens.css`, który to płótno
 * importuje — kolory mają jedno miejsce (niezmiennik 13), także dla dwóch płócien. */
const WAY_BACK = 'loadout-way-back';

/**
 * Klasa strzałki, którą praca właśnie przyszła. Regułę niesie `./graph.css`.
 *
 * 2026-08-31 — WYSTAWIONA, BO ZASTĄPIŁA RUCH. Do dziś tę strzałkę wyróżniał `animated`, czyli
 * płynąca kreska z arkusza biblioteki. Ruch na tym ekranie ma sufit dwóch rodzajów
 * [ARCHITECTURE §7] i oba są wydane: kropka żywej karty w tle i kropka kroku, który pracuje.
 * Strzałka i kropka odpowiadały przy tym na TO SAMO pytanie, a limit żywych regionów na jeden
 * fakt wynosi 1 (niezmiennik 13) — więc barwa została, ruch zszedł. Nazwa jest wystawiona,
 * żeby kryterium pytało o tę samą klasę, którą rysujący dostaje, a nie o jej kopię.
 */
export const LIVE_ARROW = 'loadout-edge-live';

/** Przerywanie linii powrotu — kształt, nie kolor, więc wolno mu stać w kodzie. Ta sama para
 * liczb, co w edytorze: dwa różne wzory kreski na jedną pętlę to dwa różne obiekty. */
const DASHED = { strokeDasharray: '6 4' } as const;

/**
 * Czy ten plan niesie UKŁAD — miejsce dla każdego kroku i choć jedną prawdziwą strzałkę.
 *
 * TRZY WARUNKI, KAŻDY Z INNEGO POWODU. Pusty plan nie ma czego pokazać. Krok bez pozycji
 * musiałby ją dostać od nas, a wtedy połowa obrazu byłaby nasza, nie człowieka. Plan bez ani
 * jednej strzałki nie niesie ŻADNEJ relacji — kafelki leżące luzem są na płótnie edytora
 * legalnym szkicem (niezmiennik 12), ale w widoku biegu byłyby obrazem, który obiecuje
 * kolejność i jej nie ma. Lista mówi w tej sytuacji dokładnie tyle, ile wiemy.
 */
export function hasLayout(plan: Plan): boolean {
  if (plan.steps.length === 0) return false;
  if (plan.steps.some((step) => step.at === undefined)) return false;
  return arrowsOf(plan).length > 0;
}

/**
 * Kafelki na płótnie — po jednym na krok, KTÓRY MA POZYCJĘ.
 *
 * Krok bez pozycji wypada, zamiast stanąć w miejscu wymyślonym przez tę funkcję. Zbiór, dla
 * którego `hasLayout` oddaje prawdę, nie ma takich kroków ani jednego; filtr stoi tu na drugą
 * drogę wywołania, której jeszcze nie ma — i po to, żeby dało się ją osądzić.
 */
export function tilesOf(plan: Plan): readonly GraphTile[] {
  return plan.steps.flatMap((step) =>
    step.at === undefined
      ? []
      : [{ id: step.id, type: 'step' as const, position: step.at, data: { step } }],
  );
}

/**
 * Strzałki na płótnie, w kolejności pliku.
 *
 * TOŻSAMOŚCIĄ STRZAŁKI JEST PARA `from->to`, więc ta sama para zapisana dwa razy jest jedną
 * strzałką. Powtórka daje w React Flow dwie krawędzie o jednym kluczu, a React na to odpowiada
 * ostrzeżeniem i prawem do pominięcia jednej z nich — strzałka, która czasem się nie rysuje,
 * jest w widoku biegu awarią. Ta sama para stron, co w `workflows/canvas/map.ts`: pliki leżące
 * już na dysku bywają poprawione ręcznie i zmergowane gitem.
 *
 * KTÓRĄ STRZAŁKĄ PRZYSZŁA PRACA. Tą, której daleki koniec pracuje TERAZ. Nie zgadujemy „ważnej"
 * ścieżki i nie wybieramy jednej z kilku: bieg równoległy jest zwykłym biegiem (niezmiennik 11),
 * a wskazanie „głównej" gałęzi byłoby relacją, której w danych nie ma.
 */
export function arrowsOf(plan: Plan): readonly GraphArrow[] {
  const standing = new Map(plan.steps.map((step) => [step.id, step]));
  const seen = new Set<string>();
  const out: GraphArrow[] = [];

  for (const link of plan.links) {
    const id = `${link.from}->${link.to}`;
    const target = standing.get(link.to);
    /* Strzałka bez obu końców nie jest rysowana: celuje w krok, którego człowiek nie widzi. */
    if (target === undefined || !standing.has(link.from) || seen.has(id)) continue;
    seen.add(id);
    /* DWA FAKTY, JEDNA KLASA, więc składane z listy, nie wybierane jednym warunkiem: powrót,
     * którego cel właśnie pracuje, jest i powrotem, i drogą pracy. Kolejność w arkuszu
     * rozstrzyga barwę i jest właściwa — reguła pracy stoi w `./graph.css`, czyli PO regule
     * powrotu, więc „dzieje się teraz" bije „to jest ścieżka wyjątkowa". */
    const marks = [
      link.max_turns === undefined ? '' : WAY_BACK,
      target.status === 'working' ? LIVE_ARROW : '',
    ].filter((one) => one !== '');
    out.push({
      id,
      source: link.from,
      target: link.to,
      ...(marks.length === 0 ? {} : { className: marks.join(' ') }),
      /* PRZERYWANA I PODPISANA, bo powrót znaczy co innego niż „po". Bez tego dwie strzałki
       * między tymi samymi kafelkami wyglądają jak pomyłka w rysowaniu. */
      ...(link.max_turns === undefined ? {} : { style: DASHED, label: triesLabel(link.max_turns) }),
    });
  }

  return out;
}

/** Nazwa kroku o tym kluczu.
 *
 * Kiedy strzałka wskazuje krok spoza planu, mówimy „another step" zamiast pokazać `s_plan`:
 * klucz jest nazwą z drutu i nie ma prawa trafić na ekran (niezmiennik 14). */
function nameOf(plan: Plan, id: string): string {
  return plan.steps.find((step) => step.id === id)?.name ?? 'another step';
}

/**
 * Czwarta linia kafelka, WYLICZONA ze strzałek (niezmiennik 17).
 *
 * DLACZEGO TO, A NIE CZAS I KOSZT. Miara, po którą sięga się odruchowo — „2m 04s · $0.31" —
 * mieszka na pasku loadoutu i nigdzie indziej (niezmiennik 13). Druga jej kopia na kafelku
 * dawałaby dwa liczniki jednego faktu, w dwóch miejscach, aktualizowane osobno.
 *
 * Trzy zdania, bo trzy różne fakty: nikt przede mną, jeden krok przede mną, kilka kroków
 * przede mną. Jedno „reads N handoffs" dla wszystkich trzech kłamie na pierwszym kroku
 * każdego biegu, a nazwanie trzech poprzedników po imieniu nie mieści się w kafelku.
 */
export function measureOf(step: GraphStep, plan: Plan): Measure {
  const incoming = plan.links.filter((link) => link.to === step.id);
  const first = incoming[0];
  const waits =
    first === undefined
      ? 'first step'
      : incoming.length === 1
        ? `after ${nameOf(plan, first.from)}`
        : `reads ${String(incoming.length)} handoffs`;

  return { waits, handsOn: plan.links.some((link) => link.from === step.id) };
}
