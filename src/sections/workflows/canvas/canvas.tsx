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
  MarkerType,
  Panel,
  Position,
  ReactFlow,
  ReactFlowProvider,
  applyEdgeChanges,
  applyNodeChanges,
  useReactFlow,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
/* PO arkuszu biblioteki, nigdy przed. Ten plik nadpisuje zmienne, które `dist/style.css`
 * przypisuje swoim `*-default`; zaimportowany wcześniej byłby martwym kodem wyglądającym
 * na działający (powód w całości stoi w nagłówku tamtego pliku). */
import './react-flow-tokens.css';
import type { ReactElement } from 'react';
import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import type { Agent } from '../../../state/agents';
import type { Note, Step, WorkflowFile } from '../../../state/workflows';
import { GRID } from '../../../state/workflows';
import { addStep, isValidConnection, onConnect, onConnectEnd } from './connect';
import type { CanvasNode } from './map';
import { onNodeDragStop, toCanvas, toFile } from './map';
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

/** Biblioteka agentów dla kafelków — tą samą drogą i z tego samego powodu co dokument.
 *
 * Kafelek pokazuje chip z NAZWĄ i kolorem agenta, a krok trzyma tylko jego identyfikator, więc
 * bez tej listy chip byłby albo identyfikatorem na ekranie (niezmiennik 14), albo wypełniaczem
 * (niezmiennik 17). Rozwiązanie stoi tutaj, jeden raz, a nie w każdym kafelku. */
const Library = createContext<readonly Agent[]>([]);

/** Kafelek na płótnie: dwa uchwyty i karta z `tile.tsx`.
 *
 * Uchwyty są TUTAJ, a nie w `StepTile`: `<Handle>` czyta magazyn React Flow, więc kafelek
 * z uchwytem nie dałby się wyrenderować poza `<ReactFlow>` — czyli nie dałby się sprawdzić
 * w środowisku `node`, w którym biegną wszystkie kryteria tego zadania. */
function CanvasTile({ id, selected }: NodeProps<StepNode>): ReactElement | null {
  const file = useContext(Opened);
  const agents = useContext(Library);
  const step = file?.steps.find((one) => one.id === id);
  if (file === null || step === undefined) return null;

  /* `undefined` znaczy „ten krok nie nazywa nikogo z biblioteki" i kafelek nie rysuje wtedy
   * chipu. Krok agenta z pustym `agent` (tak wychodzi z `＋ Add step`) trafia tu też — i to
   * jest poprawne: brak chipu jest tym, jak widać z płótna, że kroku nie da się jeszcze
   * uruchomić. */
  const agent = step.kind === 'agent' ? agents.find((one) => one.id === step.agent) : undefined;

  return (
    <>
      <Handle type="target" position={Position.Top} />
      <StepTile
        step={step}
        steps={file.steps}
        links={file.links}
        selected={selected}
        {...(agent === undefined ? {} : { agent })}
      />
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
  /** Biblioteka agentów: kafelek nazywa agenta kroku po imieniu, a krok trzyma id. */
  agents: readonly Agent[];
  /** Uwagi z walidatora Rusta (T-12). Płótno ich nie liczy i nie tłumaczy. */
  notes: Note[];
  /** Jedyne wyjście z tego komponentu: nowy dokument po decyzji użytkownika. */
  onChange: (next: WorkflowFile) => void;
  onRun: () => void;
  /** Otwiera panel kroku. Panel mieszka w ekranie obok płótna (makieta 536-570). */
  onOpenPanel: (stepId: string) => void;
}

const BUTTON = 'h-8 rounded-sm border border-line bg-raised px-3 text-ui text-ink';

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

/** Grot strzałki, z makiety (`<marker id="ar">`, `docs/mockup/index.html:558`).
 *
 * Kolor bierze `--xy-edge-stroke` z `react-flow-tokens.css`: `dist/style.css` maluje
 * `.react-flow__arrowhead` tą samą zmienną, którą maluje samą linię, więc grot nie ma prawa
 * rozjechać się ze strzałką, do której należy.
 *
 * Poza komponentem, bo to stała: nowy obiekt na każdy render przemontowywałby wszystkie
 * krawędzie. */
const ARROW = { type: MarkerType.ArrowClosed } as const;

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
    /* Grot dokładamy TUTAJ, a nie w `map.ts`: tamten plik jest mapperem PLIKU i wszystko, co
     * do niego dopiszemy, jest kandydatem do wjechania na dysk. Strzałka w pliku to `from`
     * i `to`, i nic poza tym (T3 §3.1). */
    arrows: view.edges.map((arrow) => ({ ...arrow, markerEnd: ARROW })),
  };
}

function Canvas({
  document: file,
  agents,
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

  /* Zapisuje TYLKO wtedy, kiedy dokument naprawdę się zmienił.
   *
   * 2026-08-19 — TO NIE JEST OSZCZĘDZANIE WYWOŁAŃ, tylko naprawa kasowania świeżej pracy.
   * `onConnect` i `onConnectEnd` biegną w JEDNYM zdarzeniu wskaźnika, więc React zbiera je
   * w jedną paczkę i `file` w drugim z nich jest tym sprzed pierwszego. `onConnectEnd` na
   * upuszczeniu nad kafelkiem nie ma nic do roboty i oddaje dokument NIETKNIĘTY — czyli ten
   * sprzed strzałki, którą `onConnect` właśnie dorysował. Podanie go dalej cofało tę strzałkę
   * w tej samej chwili, w której powstała.
   *
   * Porównanie po referencji, nie po treści: obie funkcje z `connect.ts` oddają DOKŁADNIE ten
   * sam obiekt, kiedy nie mają nic do zrobienia, i to jest ich udokumentowana umowa. Porównanie
   * głębokie kosztowałoby obchód całego dokumentu przy każdym ruchu myszy i odpowiadałoby na
   * inne pytanie — „czy wyszło to samo", a nie „czy ta funkcja czegokolwiek chciała". */
  const changed = useCallback(
    (next: WorkflowFile) => {
      if (next !== file) onChange(next);
    },
    [file, onChange],
  );

  const add = useCallback(
    (kind: Step['kind']) => {
      /* Decyzja „gdzie stanie i z czym się połączy" mieszka w `connect.ts` jako funkcja czysta
       * i tam jest sprawdzana. Tutaj zostaje samo wywołanie: to jest ten sam podział, którym
       * stoi cały ten plik (nagłówek), a przy okazji jedyny sposób, żeby napisać kryterium
       * na cichą utratę pracy bez przeglądarki. */
      const added = addStep(kind, file);
      onChange(added.file);
      /* Nowy kafelek jest pusty i nie ma nic do pokazania, więc panel otwiera się od razu:
       * inaczej użytkownik dostaje kartę bez treści i musi zgadnąć, że trzeba w nią kliknąć. */
      onOpenPanel(added.step.id);
    },
    [file, onChange, onOpenPanel],
  );

  return (
    <Library.Provider value={agents}>
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
              /* SUFIT POWIĘKSZENIA 1. Domyślny `maxZoom` React Flow to 2, a `fitView` na świeżym
               * workflow z jednym kafelkiem sięga po ten sufit — u właściciela cały edytor
               * rysował się dwukrotnie za duży i to była najbardziej rzucająca się w oczy
               * rozbieżność z makietą (skala 1.0, kafelek 246 px). Powiększenie WYŻEJ niż 1:1 nie
               * pokazuje niczego więcej, bo kafelek nie ma drugiego poziomu szczegółu. */
              fitViewOptions={{ maxZoom: 1 }}
              /* Plakietka „React Flow" jest linkiem NA ZEWNĄTRZ aplikacji desktopowej: jedyne
               * wyjście z okna, którego nikt nie zaprojektował, w prawym dolnym rogu ekranu,
               * na którym człowiek układa swoją pracę. */
              proOptions={{ hideAttribution: true }}
              isValidConnection={(candidate) => isValidConnection(candidate, file)}
              onConnect={(connection) => {
                changed(onConnect(connection, file));
              }}
              onConnectEnd={(event, connection) => {
                changed(
                  onConnectEnd(
                    { at: screenToFlowPosition(pointerAt(event)) },
                    {
                      isValid: connection.isValid ?? false,
                      fromNode:
                        connection.fromNode === null ? null : { id: connection.fromNode.id },
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
    </Library.Provider>
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
