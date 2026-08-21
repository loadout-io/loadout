/* Mapper płótno → plik. Piętnaście linii, które decydują, czy historia workflow da się czytać.
 *
 * `rfInstance.toObject()` jest kuszące i błędne: `NodeBase` w `@xyflow/system@0.0.80` niesie
 * `selected`, `dragging` i `measured`, a `toObject()` dokłada `viewport` [T3 §3.3]. Zapisanie
 * tego na dysk znaczy, że każde najechanie myszą i każde przesunięcie widoku brudzi plik —
 * a po roku nikt nie odczyta z historii ani jednej decyzji, bo każde spojrzenie na ekran
 * zostawiło w niej wiersz.
 *
 * Dlaczego własne `CanvasNode`/`CanvasEdge`, a nie `Node`/`Edge` z `@xyflow/react`: mapper ma
 * być wołalny bez okna, a test biegnie w środowisku `node`. Kształty niżej są PODZBIOREM tamtych
 * — pola kosmetyczne są w nich wypisane z nazwy właśnie po to, żeby dało się dowieść, że nie
 * przechodzą dalej.
 */
import type { Link, Point, Step, WorkflowFile } from '../../../state/workflows';
import { GRID } from '../../../state/workflows';

export type LinkCondition =
  | { source: 'check'; outcome: 'passed' | 'failed' }
  | { source: 'checkpoint'; choice: string }
  | { source: 'handoff'; field: string; equals: string };

interface ConditionalLink {
  from: string;
  to: string;
  when: LinkCondition;
}

type ImportedWorkflow = WorkflowFile & { linkConditions?: ConditionalLink[] };

export function importedConditionLabel(condition: LinkCondition): string {
  if (condition.source === 'check')
    return condition.outcome === 'passed' ? 'When checks pass' : 'When checks fail';
  if (condition.source === 'checkpoint') return `When you choose ${condition.choice}`;
  return `When ${condition.field} is ${condition.equals}`;
}

/** Tyle z `Node`, ile mapper widzi. Cztery ostatnie pola są tu wyłącznie po to, żeby test mógł
 * je podać i sprawdzić, że w pliku ich nie ma. */
export interface CanvasNode {
  id: string;
  position: Point;
  /** Który komponent rysuje ten kafelek: `nodeTypes` ma dokładnie klucze `agent`
   * i `checkpoint` (TASK.md, rozstrzygnięcie 1). Opcjonalny, bo mapper w drugą stronę
   * go nie czyta — rodzaj kroku niesie `data.kind`. */
  type?: Step['kind'];
  /** Krok JEST danymi kafelka [T3 §3.3] — i dlatego `selected` w `data` wjechałoby do pliku. */
  data: Step;
  selected?: boolean;
  dragging?: boolean;
  measured?: { width?: number; height?: number };
}

export interface CanvasEdge {
  id: string;
  source: string;
  target: string;
  /**
   * Limit rund, jeżeli ta krawędź jest POWROTEM.
   *
   * 2026-08-19 — POLE JEST TU, BO INACZEJ GINIE, i to jest naprawa utraty danych, nie wygoda.
   * `toFile` odbudowuje `links` **z krawędzi** (`edges.map(...)`), więc wszystko, czego krawędź nie
   * zna, przestaje istnieć w pliku. Zmierzone na ścieżkach, które przez `toFile` przechodzą:
   * skasowanie kafelka i skasowanie strzałki. Skutek nie był cichy, był gorszy — plik zostawał
   * z nieoznaczonym cyklem, walidator dawał `Problem`, a `save` odmawiał przed zapisem. Czyli
   * **skasowanie jednej niepowiązanej strzałki zamieniało workflow z pętlą w plik, którego nie da
   * się zapisać**, a płótno rozjeżdżało się z dyskiem.
   *
   * Nie jest to kosmetyka w rozumieniu `viewOf` w `canvas.tsx`: tamten nagłówek mówi, że wygląd
   * nie ma prawa wejść do mappera, i to zostaje w mocy. `maxTurns` jest ZNACZENIEM strzałki —
   * jedziemy nim tędy dokładnie z tego powodu.
   */
  maxTurns?: number;
  condition?: LinkCondition;
}

/** Najbliższa całkowita wielokrotność `GRID` na jednej osi.
 *
 * `Math.round` rozstrzyga połowę skoku W GÓRĘ i to jest decyzja, nie przypadek: `12 → 24`,
 * `11.9 → 0`. Musi zapaść raz, tutaj, bo obie drogi zapisu (przeciągnięcie i mapper) wołają
 * tę samą funkcję — dwie różne zaokrąglałyby o jedną linię siatki w różne strony.
 *
 * `+ 0` na końcu nie jest ozdobą: `Math.round(-0.4)` daje `-0`, a `-0` przeżywa mnożenie
 * i porównuje się z `0` inaczej niż `0` (`Object.is`), więc pozycja tuż nad lewą krawędzią
 * potrafiłaby zerwać porównanie dokumentów, którego cały ten plik broni. `-0 + 0` to `0`. */
function toGrid(value: number): number {
  return Math.round(value / GRID) * GRID + 0;
}

/** Najbliższa całkowita wielokrotność `GRID`.
 *
 * `240.00000001` brudzi diff przy każdym najechaniu myszą, `240` nie brudzi go nigdy. */
export function snap(point: Point): Point {
  return { x: toGrid(point.x), y: toGrid(point.y) };
}

/** Płótno → plik. `prev` niesie wszystko, czego płótno nie dotyka (`format`, `id`, `name`).
 *
 * Pozycję przyciągamy TUTAJ, a nie w handlerze przeciągania: handler jest jedną z kilku dróg
 * zapisu, a mapper jest wszystkimi. */
/* Pola, które należą do PŁÓTNA i nie mają prawa dojechać do pliku. Trzy pierwsze siedzą
 * w `NodeBase` w `@xyflow/system@0.0.80`, `viewport` dokłada `toObject()`, a `position` jest
 * nazwą, której plik nie zna — pozycja nazywa się w nim `at` [T3 §3.3]. */
const CANVAS_ONLY = ['selected', 'dragging', 'measured', 'position', 'viewport'] as const;

/** Sam krok, bez tego, co płótno wie o sobie, postawiony na siatce.
 *
 * Kasowanie po nazwach klucza, a nie przepisywanie znanych pól: plik może nieść klucz, którego
 * TA wersja nie zna (Rust trzyma go w `extra`, T3 §3.2), a przepisywanie po liście skasowałoby
 * po cichu pracę nowszego builda. Znamy pola, których nie chcemy; nie znamy wszystkich, które
 * chcemy. Typ mówi, że tych pięciu tu nie ma — obiekt przychodzi z React Flow i bywa inaczej,
 * i dokładnie o tym jest to kryterium. */
function onlyTheStep<S extends Step>(data: S, at: Point): S {
  const step = { ...data, at };
  /* `Reflect.deleteProperty`, nie `delete` — `delete` na polu, którego typ nie deklaruje,
   * wymaga rzutowania kroku na mapę, a rzutowanie unii na `Record<string, unknown>` odrzuca
   * kompilator. Ta forma pyta o to samo bez ani jednego `as`. */
  for (const key of CANVAS_ONLY) Reflect.deleteProperty(step, key);
  return step;
}

/** Kafelki w kolejności, w jakiej stoją w PLIKU; świeże na końcu, w kolejności płótna.
 *
 * React Flow domyślnie podnosi zaznaczony kafelek na koniec tablicy (`elevateNodesOnSelect`),
 * żeby rysował się nad sąsiadami. Bez tej funkcji samo kliknięcie przestawiałoby `steps`
 * i autosave zapisywałby przetasowany plik — czyli dokładnie tę zmianę bez decyzji, przed którą
 * stoi to kryterium, tylko o jedno pole wyżej [T3 §8.2 reguła 2]. */
function inFileOrder(prev: WorkflowFile, nodes: CanvasNode[]): CanvasNode[] {
  const rank = new Map(prev.steps.map((step, at) => [step.id, at]));
  /* `MAX_SAFE_INTEGER`, nie `Infinity`: różnica dwóch nieskończoności to `NaN`, a komparator
   * zwracający `NaN` sortuje w sposób zależny od implementacji. */
  const rankOf = (node: CanvasNode) => rank.get(node.id) ?? Number.MAX_SAFE_INTEGER;
  return [...nodes].sort((one, other) => rankOf(one) - rankOf(other));
}

/** Strzałki bez powtórzeń, w kolejności pierwszego wystąpienia.
 *
 * TOŻSAMOŚCIĄ STRZAŁKI JEST PARA `from->to` i nic poza nią — plik nie ma na strzałce ani jednego
 * innego pola, więc dwie pozycje o tej samej parze nie są dwiema strzałkami, tylko jedną
 * zapisaną dwa razy. Do 2026-08-19 nikt tego nie egzekwował po żadnej stronie i skutek leżał
 * na dysku właściciela: `new-workflow.json` niósł `s_2->s_3` dwa razy i `s_4->s_5` trzy razy.
 *
 * Kosztem nie był bałagan w pliku, tylko rozbite płótno. `toCanvas` nadaje krawędzi
 * identyfikator `from->to`, więc powtórzona pozycja daje DWIE krawędzie o jednym identyfikatorze,
 * a React na to odpowiada „Encountered two children with the same key" i ostrzega, że dzieci
 * mogą zostać zdublowane albo pominięte. Strzałka, która czasem się nie rysuje, jest w edytorze
 * grafu awarią, a nie kosmetyką.
 *
 * Zwężenie stoi w OBU mapperach, nie w jednym: `toCanvas` leczy pliki, które już leżą na dysku
 * (także poprawione ręcznie albo zmergowane gitem), a `toFile` pilnuje, żeby płótno nigdy
 * takiego pliku nie wyprodukowało. Jedna strona wystarczyłaby do zieleni testu i zostawiła
 * drugą połowę awarii. */
function eachArrowOnce(links: readonly Link[]): Link[] {
  const seen = new Set<string>();
  return links.filter((link) => {
    const identity = `${link.from}->${link.to}`;
    if (seen.has(identity)) return false;
    seen.add(identity);
    return true;
  });
}

export function toFile(prev: WorkflowFile, nodes: CanvasNode[], edges: CanvasEdge[]): WorkflowFile {
  const conditions = edges.flatMap((edge) =>
    edge.condition === undefined
      ? []
      : [{ from: edge.source, to: edge.target, when: edge.condition }],
  );
  const next: ImportedWorkflow = {
    ...prev,
    steps: inFileOrder(prev, nodes).map((node) => onlyTheStep(node.data, snap(node.position))),
    /* POWRÓT PRZEŻYWA PODRÓŻ, i to jest cała treść tych dwóch linii. Klucz `max_turns` jedzie
     * tylko wtedy, gdy krawędź go niesie: dopisanie `max_turns: undefined` do każdej zwykłej
     * strzałki wpisałoby do pliku pole, którego tam nie było, i przepisało każdy istniejący
     * workflow przy pierwszym zapisie (T3 §8.2). */
    links: eachArrowOnce(
      edges.map((edge) =>
        edge.maxTurns === undefined
          ? { from: edge.source, to: edge.target }
          : { from: edge.source, to: edge.target, max_turns: edge.maxTurns },
      ),
    ),
  };
  if (conditions.length === 0) delete next.linkConditions;
  else next.linkConditions = conditions;
  return next;
}

/** Plik → płótno. Druga połowa mappera i jedyne miejsce, w którym powstają identyfikatory
 * krawędzi: `from->to` jest funkcją samej strzałki, więc dwa razy narysowana ta sama strzałka
 * to jedna krawędź, a nie dwie różne o tym samym znaczeniu. */
export function toCanvas(file: WorkflowFile): {
  /* `type` jest tu WYMAGANY, choć w `CanvasNode` jest opcjonalny: ten mapper zawsze wie, który
   * z dwóch komponentów rysuje kafelek, a płótno nie ma sensownego zachowania dla „nie wiem". */
  nodes: Array<CanvasNode & { type: Step['kind'] }>;
  edges: CanvasEdge[];
} {
  return {
    nodes: file.steps.map((step) => ({
      id: step.id,
      type: step.kind,
      position: step.at,
      data: step,
    })),
    /* Klucz tylko wtedy, gdy plik go niesie — `exactOptionalPropertyTypes` tego pilnuje i ma
     * rację: krawędź z `maxTurns: undefined` wróciłaby przez `toFile` jako strzałka z pustym
     * polem, a stąd do przepisania każdego workflow przy pierwszym zapisie jest jeden krok. */
    edges: eachArrowOnce(file.links).map((link) => {
      const condition = (file as ImportedWorkflow).linkConditions?.find(
        (candidate) => candidate.from === link.from && candidate.to === link.to,
      )?.when;
      return {
        id: `${link.from}->${link.to}`,
        source: link.from,
        target: link.to,
        ...(link.max_turns === undefined ? {} : { maxTurns: link.max_turns }),
        ...(condition === undefined ? {} : { condition }),
      };
    }),
  };
}

/** Ten sam krok, postawiony gdzie indziej.
 *
 * Generyk, a nie `Step`: rozłożenie unii przez `{ ...step }` daje typ, który nie jest już ani
 * jednym, ani drugim wariantem, i kompilator słusznie odmawia przypisania go z powrotem. */
function movedTo<S extends Step>(step: S, at: Point): S {
  return { ...step, at };
}

/** Koniec przeciągnięcia kafelka: pozycja ląduje na siatce, w dokumencie, nie tylko na ekranie. */
export function onNodeDragStop(
  node: { id: string; position: Point },
  file: WorkflowFile,
): WorkflowFile {
  const at = snap(node.position);
  return {
    ...file,
    steps: file.steps.map((step) => (step.id === node.id ? movedTo(step, at) : step)),
  };
}
