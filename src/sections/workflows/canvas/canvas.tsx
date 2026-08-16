/* Płótno: React Flow, dwa rodzaje kafelka i cztery przyciski, z których każdy ma handler.
 *
 * Ten plik jest MONTAŻEM, nie logiką: wszystko, co decyduje, mieszka w czystych funkcjach obok
 * (`connect.ts`, `map.ts`, `tidy.ts`, `problems.tsx`) i tam jest sprawdzane. Powód jest
 * mechaniczny, nie estetyczny: gestu — `pointerdown` na uchwycie, ruch, `pointerup` — nie da
 * się odtworzyć bez przeglądarki, a w repo nie ma ani `jsdom`, ani Playwrighta [T3 §2.3,
 * ryzyko 7]. Wszystko, co dałoby się tu ukryć przed testem, jest więc na zewnątrz.
 *
 * DOKUMENT JEST PRAWDĄ, stan React Flow jest jego widokiem. Kafelki i strzałki żyją w `useState`
 * tego komponentu tylko po to, żeby przeciąganie było płynne; każda DECYZJA (upuszczenie
 * strzałki, koniec przeciągnięcia, skasowanie, przycisk) natychmiast wychodzi przez `onChange`
 * i wraca nowym dokumentem, z którego widok jest odbudowywany. Zaznaczenie i wymiary nie są
 * decyzją i nie wychodzą nigdy — to jest kryterium `to-file` postawione w kodzie aplikacji,
 * a nie tylko w teście.
 *
 * `Run` MIESZKA TUTAJ (`RunBar`, razem z paskiem uwag), bo blokada uruchomienia i zdanie o tym,
 * dlaczego nie da się uruchomić, są jednym faktem (niezmiennik 13). Ekran, który to płótno
 * zamontuje, nie ma prawa dołożyć drugiego `Run` w nagłówku — makieta rysuje go w nagłówku,
 * ale dwa przyciski o tej samej nazwie i różnym stanie to dokładnie ta awaria, przed którą
 * stoi ten niezmiennik.
 */
import type { Edge, EdgeChange, Node, NodeChange, NodeProps } from '@xyflow/react';
import {
  Background,
  Handle,
  Panel,
  Position,
  ReactFlow,
  ReactFlowProvider,
  applyEdgeChanges,
  applyNodeChanges,
  useReactFlow,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import type { ReactElement } from 'react';
import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import type { Note, Point, Step, WorkflowFile } from '../../../state/workflows';
import { GRID } from '../../../state/workflows';
import { freshId, freshStep, isValidConnection, onConnect, onConnectEnd } from './connect';
import type { CanvasNode } from './map';
import { onNodeDragStop, snap, toCanvas, toFile } from './map';
import { RunBar, focusNote } from './problems';
import { StepTile } from './tile';
import { tidyUp } from './tidy';

/** Dane kafelka dla React Flow.
 *
 * Alias typu, nie interfejs: `Node<T>` wymaga typu z indeksem łańcuchowym, a interfejs go nie
 * dostaje — `Node<AgentStep>` po prostu się nie kompiluje. Opakowanie kroku w jedno pole
 * kosztuje jedną linię i zdejmuje potrzebę rzutowania w drugą stronę. */
type TileData = { step: Step };
type StepNode = Node<TileData>;
type TileChanges = NodeChange<StepNode>[];
type ArrowChanges = EdgeChange<Edge>[];

/** Otwarty dokument dla kafelków.
 *
 * Kafelek liczy stopkę ze WSZYSTKICH strzałek i nazywa poprzednika po nazwie, więc potrzebuje
 * całego dokumentu, a nie swojego kawałka. Kontekst zamiast przepisania dokumentu do `data`
 * każdego kafelka: piętnaście kopii tego samego pliku rozjeżdża się dokładnie wtedy, gdy panel
 * zmieni krok, a płótno jeszcze o tym nie wie. */
const Opened = createContext<WorkflowFile | null>(null);

/** Kafelek na płótnie: dwa uchwyty i karta z `tile.tsx`.
 *
 * Uchwyty są TUTAJ, a nie w `StepTile`: `<Handle>` czyta magazyn React Flow, więc kafelek
 * z uchwytem nie dałby się wyrenderować poza `<ReactFlow>` — czyli nie dałby się sprawdzić
 * w środowisku `node`, w którym biegną wszystkie kryteria tego zadania. */
function CanvasTile({ id, selected }: NodeProps<StepNode>): ReactElement | null {
  const file = useContext(Opened);
  const step = file?.steps.find((one) => one.id === id);
  if (file === null || step === undefined) return null;

  return (
    <>
      <Handle type="target" position={Position.Top} />
      <StepTile step={step} steps={file.steps} links={file.links} selected={selected} />
      <Handle type="source" position={Position.Bottom} />
    </>
  );
}

/* Dwa rodzaje kafelka. Tylko dwa — trzeci wymaga prawdziwej skargi użytkownika, nie hipotezy
 * [T3 §10 ryzyko 6]. Oba rysuje ta sama karta, bo różnica między nimi jest w danych (punkt
 * kontrolny nie ma agenta ani kopii), a nie w kształcie kafelka.
 *
 * Poza komponentem, bo React Flow przy każdej nowej referencji `nodeTypes` przemontowuje
 * wszystkie kafelki — czyli gubi zaznaczenie i przerywa przeciąganie. */
const NODE_TYPES = { agent: CanvasTile, checkpoint: CanvasTile };

export interface WorkflowCanvasProps {
  /** Otwarty dokument. Płótno go nie trzyma — pokazuje. */
  document: WorkflowFile;
  /** Uwagi z walidatora Rusta (T-12). Płótno ich nie liczy i nie tłumaczy. */
  notes: Note[];
  /** Jedyne wyjście z tego komponentu: nowy dokument po decyzji użytkownika. */
  onChange: (next: WorkflowFile) => void;
  onRun: () => void;
  /** Otwiera panel kroku. Panel mieszka w ekranie obok płótna (makieta 536-570). */
  onOpenPanel: (stepId: string) => void;
}

const BUTTON = 'h-8 rounded-sq border border-line bg-raised px-3 text-ui text-ink';

/** Punkt zdarzenia wskaźnika w układzie EKRANU. Dotyk daje współrzędne w innym miejscu niż
 * mysz, a `onConnectEnd` dostaje jedno i drugie. */
function pointerAt(event: MouseEvent | TouchEvent): { x: number; y: number } {
  if (!('changedTouches' in event)) return { x: event.clientX, y: event.clientY };

  const touch = event.changedTouches[0];
  return touch === undefined ? { x: 0, y: 0 } : { x: touch.clientX, y: touch.clientY };
}

/** Kafelek React Flow → tyle, ile czyta mapper.
 *
 * `selected`, `dragging` i `measured` nie są tu przepisywane: mapper i tak je kasuje, ale
 * najtańszym sposobem, żeby nie dojechały do pliku, jest ich nie podawać. */
function asCanvasNode(node: StepNode): CanvasNode {
  return { id: node.id, position: node.position, data: node.data.step };
}

/** Kafelki i strzałki dla React Flow, zbudowane z dokumentu. */
function viewOf(file: WorkflowFile): { tiles: StepNode[]; arrows: Edge[] } {
  const view = toCanvas(file);
  return {
    tiles: view.nodes.map((tile) => ({
      id: tile.id,
      type: tile.type,
      position: tile.position,
      data: { step: tile.data },
    })),
    arrows: view.edges,
  };
}

/** Wolne miejsce pod najniższym kafelkiem — tam ląduje krok dodany przyciskiem.
 *
 * Nowy kafelek dokładnie na innym wygląda jak zgubiony, a płótno nie ma jak zapytać, gdzie
 * użytkownik go chciał: przycisk nie niesie punktu, w przeciwieństwie do upuszczenia strzałki. */
function roomBelow(file: WorkflowFile): Point {
  const lowest = file.steps.reduce((deepest, step) => Math.max(deepest, step.at.y), 0);
  return snap({ x: GRID, y: file.steps.length === 0 ? GRID : lowest + 6 * GRID });
}

function Canvas({
  document: file,
  notes,
  onChange,
  onRun,
  onOpenPanel,
}: WorkflowCanvasProps): ReactElement {
  const { screenToFlowPosition, fitView } = useReactFlow();
  const view = useMemo(() => viewOf(file), [file]);
  /* `tiles` i `arrows`, nie `nodes` i `edges`: to są nazwy, którymi ta aplikacja mówi
   * o kafelkach i strzałkach (niezmiennik 14), a przy okazji jedyne, które nie wpadają
   * w `checks/quick-vocabulary.sh`. Prop React Flow nazywa się jak nazywa i tego nie zmienimy. */
  const [tiles, setTiles] = useState(view.tiles);
  const [arrows, setArrows] = useState(view.arrows);

  /* Dokument zmienił się poza płótnem — przez panel kroku, „Tidy up" albo otwarcie innego
   * pliku. Zaznaczenie przenosimy ręcznie, bo należy do płótna i w pliku go nie ma; bez tej
   * linii każda zmiana w panelu odznaczałaby kafelek, nad którym użytkownik właśnie pracuje. */
  useEffect(() => {
    setTiles((now) => {
      const chosen = new Set(now.filter((one) => one.selected === true).map((one) => one.id));
      return view.tiles.map((tile) => (chosen.has(tile.id) ? { ...tile, selected: true } : tile));
    });
    setArrows(view.arrows);
  }, [view]);

  const tilesChanged = useCallback(
    (changes: TileChanges) => {
      const next = applyNodeChanges(changes, tiles);
      setTiles(next);
      /* Skasowanie kafelka jest decyzją i musi dojść do pliku. Zaznaczenie, najechanie
       * i zmierzone wymiary decyzją nie są i plik ich nie zobaczy. */
      if (changes.some((one) => one.type === 'remove')) {
        onChange(toFile(file, next.map(asCanvasNode), arrows));
      }
    },
    [tiles, arrows, file, onChange],
  );

  const arrowsChanged = useCallback(
    (changes: ArrowChanges) => {
      const next = applyEdgeChanges(changes, arrows);
      setArrows(next);
      if (changes.some((one) => one.type === 'remove')) {
        onChange(toFile(file, tiles.map(asCanvasNode), next));
      }
    },
    [tiles, arrows, file, onChange],
  );

  const add = useCallback(
    (kind: Step['kind']) => {
      const step = freshStep(kind, freshId(file), roomBelow(file));
      onChange({ ...file, steps: [...file.steps, step] });
      /* Nowy kafelek jest pusty i nie ma nic do pokazania, więc panel otwiera się od razu:
       * inaczej użytkownik dostaje kartę bez treści i musi zgadnąć, że trzeba w nią kliknąć. */
      onOpenPanel(step.id);
    },
    [file, onChange, onOpenPanel],
  );

  return (
    <Opened.Provider value={file}>
      <div className="flex h-full min-h-0 flex-col gap-2">
        <div className="min-h-0 flex-1">
          <ReactFlow
            nodes={tiles}
            edges={arrows}
            nodeTypes={NODE_TYPES}
            onNodesChange={tilesChanged}
            onEdgesChange={arrowsChanged}
            /* Siatka jest jedna i ta sama co w pliku: kafelek stoi tam, gdzie plik mówi,
             * że stoi, także w trakcie przeciągania [T3 §8.2 reguła 1]. */
            snapToGrid
            snapGrid={[GRID, GRID]}
            fitView
            isValidConnection={(candidate) => isValidConnection(candidate, file)}
            onConnect={(connection) => {
              onChange(onConnect(connection, file));
            }}
            onConnectEnd={(event, connection) => {
              onChange(
                onConnectEnd(
                  { at: screenToFlowPosition(pointerAt(event)) },
                  {
                    isValid: connection.isValid ?? false,
                    fromNode: connection.fromNode === null ? null : { id: connection.fromNode.id },
                  },
                  file,
                ),
              );
            }}
            onNodeDragStop={(_event, node) => {
              onChange(onNodeDragStop({ id: node.id, position: node.position }, file));
            }}
            onNodeClick={(_event, node) => {
              onOpenPanel(node.id);
            }}
          >
            <Background gap={GRID} />
            <Panel position="top-right">
              <RunBar
                notes={notes}
                onRun={onRun}
                onFocusNote={(note) => {
                  focusNote(note, { fitView, openPanel: onOpenPanel });
                }}
              />
            </Panel>
          </ReactFlow>
        </div>

        {/* Dokładnie dwa przyciski tworzące (makieta 528-529). „Tidy up" stoi obok nich, a nie
            w nagłówku ekranu: układ jest własnością płótna, a nagłówek należy do ekranu, który
            to płótno montuje. */}
        <div className="flex gap-2">
          <button
            type="button"
            className={BUTTON}
            onClick={() => {
              add('agent');
            }}
          >
            ＋ Add step
          </button>
          <button
            type="button"
            className={BUTTON}
            onClick={() => {
              add('checkpoint');
            }}
          >
            ＋ Add a checkpoint
          </button>
          <button
            type="button"
            className={BUTTON}
            onClick={() => {
              onChange(tidyUp(file));
            }}
          >
            Tidy up
          </button>
        </div>
      </div>
    </Opened.Provider>
  );
}

/** Płótno gotowe do zamontowania.
 *
 * `ReactFlowProvider` jest tu, a nie w ekranie: `screenToFlowPosition` i `fitView` przychodzą
 * z `useReactFlow()`, więc komponent, który je woła, musi stać wewnątrz dostawcy. Zostawienie
 * tego ekranowi znaczyłoby, że płótno da się zamontować w sposób, w który nie działa. */
export function WorkflowCanvas(props: WorkflowCanvasProps): ReactElement {
  return (
    <ReactFlowProvider>
      <Canvas {...props} />
    </ReactFlowProvider>
  );
}
