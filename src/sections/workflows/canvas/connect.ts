/* Rysowanie strzałek: co wolno połączyć i co się dzieje po upuszczeniu na pustym płótnie.
 *
 * SZKIELET — ciała rzucają `not implemented` (AGENTS.md §2a, odpowiednik `todo!()`).
 *
 * Wszystkie trzy funkcje są CZYSTE i biorą dokument ostatnim argumentem. Powód jest testowy
 * i architektoniczny naraz: gest — `pointerdown` na uchwycie, ruch, `pointerup` — nie jest
 * odtwarzalny bez przeglądarki [T3 §2.3, ryzyko 7], więc kryteria wołają dokładnie te funkcje,
 * które gest woła, z syntetycznym stanem połączenia. Wersja domknięta na `getNodes()/getEdges()`
 * (T3 §5.1) byłaby nie do zawołania bez `<ReactFlow>` w drzewie.
 *
 * Odchylenie od prozy TASK.md, świadome: kryterium pisze `isValidConnection({ source, target })`
 * z jednym argumentem, bo w React Flow ta funkcja jest domknięciem nad żywym grafem. Tutaj graf
 * przychodzi jawnie — płótno robi `isValidConnection={(c) => isValidConnection(c, file)}`.
 */
import type { Point, WorkflowFile } from '../../../state/workflows';

/** Tyle z `Connection` z `@xyflow/react`, ile ta warstwa czyta. Portów nie mamy (T3 §3.1). */
export interface Connection {
  source: string;
  target: string;
}

/** Tyle ze stanu, który React Flow oddaje w `onConnectEnd`, ile ta warstwa czyta.
 *
 * `isValid: true` znaczy „upuszczono nad istniejącym kafelkiem" — strzałkę robi wtedy
 * `onConnect` i tworzenie kroku byłoby kafelkiem-widmem przy każdym udanym połączeniu. */
export interface ConnectionEnd {
  isValid: boolean;
  fromNode: { id: string } | null;
}

/** Zdarzenie wskaźnika obcięte do jedynej rzeczy, której potrzebujemy: punktu upuszczenia
 * W UKŁADZIE PŁÓTNA. Przeliczenie z ekranu robi `screenToFlowPosition` w komponencie — tu
 * przychodzi już gotowy punkt, żeby ta funkcja nie potrzebowała ani okna, ani viewportu. */
export interface DropEvent {
  at: Point;
}

/** „Czy da się narysować tę strzałkę?" — jeden bool, jedyne sprawdzenie po stronie TypeScriptu.
 *
 * Cykl jest UNIEMOŻLIWIONY, nie zgłoszony: strzałka po prostu nie ląduje, uchwyt szarzeje
 * i nie ma żadnego komunikatu, bo użytkownik nie zrobił nic złego [T3 §5.1]. Reszta pytań
 * („czy to da się uruchomić?") należy do Rusta i wraca jako `Note[]`. */
export function isValidConnection(_connection: Connection, _file: WorkflowFile): boolean {
  throw new Error('not implemented');
}

/** Dokłada strzałkę, jeżeli [`isValidConnection`] ją przepuszcza; inaczej oddaje dokument
 * bez zmiany. Odmowa jest cicha — tu nie powstaje żadna uwaga. */
export function onConnect(_connection: Connection, _file: WorkflowFile): WorkflowFile {
  throw new Error('not implemented');
}

/** Upuszczenie końca strzałki.
 *
 * Na PUSTYM płótnie (`isValid: false`) powstaje jeden krok rodzaju `agent` w przyciągniętym
 * punkcie upuszczenia i jedna strzałka do niego — „utwórz i połącz jednym ruchem"
 * [T3 §9, „MVP ships" punkt 2]. Nad istniejącym kafelkiem (`isValid: true`) nie powstaje nic.
 *
 * Identyfikator nowego kroku wyprowadzamy z dokumentu, a nie z zegara ani z losowości: funkcja
 * ma być czysta, a plik ma się dać porównać gitem po dwóch takich samych gestach. */
export function onConnectEnd(
  _event: DropEvent,
  _connection: ConnectionEnd,
  _file: WorkflowFile,
): WorkflowFile {
  throw new Error('not implemented');
}
