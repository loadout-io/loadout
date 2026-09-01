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
   * Jawny fakt schedulera: ten krok padł, ale wykonał politykę „jedź dalej”.
   *
   * Opcjonalny, bo starszy strumień może go nie znać. Nie wyliczamy go ze strzałek ani
   * z żywego stanu potomka: oba mogą opisywać równoległą pracę, nie decyzję o tej porażce.
   */
  readonly carriedOn?: boolean;
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

/**
 * POZIOM każdego kroku, liczony ze strzałek — czyli jedyna liczba, którą wolno postawić przy
 * kroku na ekranie.
 *
 * Krok, na który nie wskazuje ani jedna strzałka, stoi na poziomie 1. Każdy inny stoi o jeden
 * niżej od NAJDALSZEGO ze swoich poprzedników. Dwa kroki na jednym poziomie nie mają między sobą
 * kolejności i mogą ruszyć razem.
 *
 * CO TO NAPRAWIA, ZMIERZONE 2026-08-31 (zgłoszenie właściciela: „w sumie te numerki kłamią bo
 * kilka może iść na raz cnie"). Kolumna kroków numerowała je POZYCJĄ W TABLICY (`path.tsx`,
 * `at + 1`) i nie patrzyła na strzałki ani razu. Dwa kroki wiszące na tym samym poprzedniku
 * dostawały przez to „2" i „3" — czyli obietnicę, że jeden idzie przed drugim. W pliku tej
 * relacji nie ma, a bieg puszcza oba naraz: `engine::scheduler` wypuszcza w pierwszym obrocie
 * WSZYSTKIE kroki o zerowym stopniu wejściowym, a semafor ogranicza tylko, ile z nich naprawdę
 * ruszy w tej chwili. Numer był więc relacją, której w danych nie ma (niezmiennik 17).
 *
 * SUFIT „ILE NARAZ" NIE MA Z TĄ LICZBĄ NIC WSPÓLNEGO i nie ma prawa mieć. Poziom mówi, co WOLNO
 * puścić razem; suwak „How many agents at once?" mówi, ile Loadout naprawdę uruchomi. Wpisanie
 * sufitu w tę liczbę byłoby nowym kłamstwem w miejsce starego — ekran odpowiada tu na pytanie
 * o zależności, nie o przepustowość.
 *
 * DROGA POWROTNA NIE JEST KOLEJNOŚCIĄ i dlatego wypada. Strzałka z `max_turns` znaczy „spróbuj
 * jeszcze raz" (`state/workflows.ts`, `Link`): wraca do kroku, który już był, i domyka koło
 * z rozmysłu. Policzona jako „potem" robi z grafu cykl, a w cyklu poziom przestaje istnieć.
 * Rust czyta ją dokładnie tak samo — `workflow::check` liczy koło wyłącznie na strzałkach bez
 * powrotów, a `workflow::unroll` rozwija pętlę na literalne rundy, zanim planista zobaczy graf.
 *
 * STRZAŁKA BEZ OBU KOŃCÓW W PLANIE TEŻ WYPADA. Ten sam warunek, co w [`arrowsOf`] i z tego
 * samego powodu: krok, którego człowiek nie widzi, nie ma prawa przesuwać numeru krokowi,
 * którego widzi.
 *
 * DLACZEGO ODPRĘŻANIE W PĘTLI, A NIE OBCHÓD W GŁĄB. Obchód rekurencyjny nad danymi z drutu
 * zapętla się na cyklu, którego walidator nie widział — a plan przyjeżdża tu z okna i „nigdy nie
 * wywalaj biegu na nieznanym zdarzeniu" (niezmiennik 5) obowiązuje także rysującego. Liczba
 * przebiegów jest ograniczona liczbą kroków, więc łańcuch dowolnej długości zdąży się ustawić,
 * a graf, który mimo wszystko przyjechał z kołem, kończy się skończonymi liczbami zamiast
 * zawieszonym oknem.
 */
export function levelsOf(plan: Plan): ReadonlyMap<string, number> {
  const standing = new Set(plan.steps.map((step) => step.id));
  const parents = new Map<string, string[]>();
  for (const link of plan.links) {
    if (link.max_turns !== undefined) continue;
    if (!standing.has(link.from) || !standing.has(link.to)) continue;
    const mine = parents.get(link.to);
    if (mine === undefined) parents.set(link.to, [link.from]);
    else mine.push(link.from);
  }

  const level = new Map(plan.steps.map((step) => [step.id, 1]));
  for (let pass = 0; pass < plan.steps.length; pass += 1) {
    let moved = false;
    for (const step of plan.steps) {
      const mine = level.get(step.id) ?? 1;
      let want = mine;
      for (const parent of parents.get(step.id) ?? []) {
        want = Math.max(want, (level.get(parent) ?? 1) + 1);
      }
      if (want > mine) {
        level.set(step.id, want);
        moved = true;
      }
    }
    if (!moved) break;
  }
  return level;
}
