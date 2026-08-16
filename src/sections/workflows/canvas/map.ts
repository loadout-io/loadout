/* Mapper płótno → plik. Piętnaście linii, które decydują, czy historia workflow da się czytać.
 *
 * SZKIELET — ciała rzucają `not implemented` (AGENTS.md §2a, odpowiednik `todo!()`).
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
import type { Point, Step, WorkflowFile } from '../../../state/workflows';

/** Tyle z `Node`, ile mapper widzi. Cztery ostatnie pola są tu wyłącznie po to, żeby test mógł
 * je podać i sprawdzić, że w pliku ich nie ma. */
export interface CanvasNode {
  id: string;
  position: Point;
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
}

/** Najbliższa całkowita wielokrotność `GRID`.
 *
 * `240.00000001` brudzi diff przy każdym najechaniu myszą, `240` nie brudzi go nigdy. */
export function snap(_point: Point): Point {
  throw new Error('not implemented');
}

/** Płótno → plik. `prev` niesie wszystko, czego płótno nie dotyka (`format`, `id`, `name`).
 *
 * Pozycję przyciągamy TUTAJ, a nie w handlerze przeciągania: handler jest jedną z kilku dróg
 * zapisu, a mapper jest wszystkimi. */
export function toFile(
  _prev: WorkflowFile,
  _nodes: CanvasNode[],
  _edges: CanvasEdge[],
): WorkflowFile {
  throw new Error('not implemented');
}

/** Koniec przeciągnięcia kafelka: pozycja ląduje na siatce, w dokumencie, nie tylko na ekranie. */
export function onNodeDragStop(
  _node: { id: string; position: Point },
  _file: WorkflowFile,
): WorkflowFile {
  throw new Error('not implemented');
}
