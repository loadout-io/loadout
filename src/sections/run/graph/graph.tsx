/* Płótno biegu: ten sam obiekt, co graf edytora, tylko tylko-do-odczytu.
 *
 * DLACZEGO TA SAMA BIBLIOTEKA, A NIE WŁASNE SVG. Teza całej przebudowy brzmi: graf biegu ma
 * BYĆ grafem, który człowiek ułożył, a nie drugim rysunkiem, który wygląda podobnie. Własne
 * SVG dałoby dwa różne obiekty o wspólnym wyglądzie — i rozjechałyby się przy pierwszej
 * zmianie w którymkolwiek z nich, po cichu, bo oba dalej by się rysowały. Biblioteka jest już
 * zależnością (12.11.3) i jedzie z edytorem, więc nie kosztuje ani bajta w paczce.
 *
 * TEN PLIK JEST MONTAŻEM, NIE LOGIKĄ. Co wolno narysować, gdzie stoi kafelek i którą strzałką
 * idzie praca — wszystko to mieszka w `./model.ts` jako funkcje czyste i tam jest sądzone.
 * Powód jest mechaniczny: React Flow pod `renderToStaticMarkup` oddaje ramę płótna z PUSTYMI
 * pojemnikami na kafelki i strzałki, bo mierzy je dopiero w przeglądarce. Wszystko, co dałoby
 * się schować za tym pomiarem, jest więc na zewnątrz — ten sam podział, co w edytorze.
 *
 * PŁÓTNO MILCZY, KIEDY NIE MA UKŁADU (reguła 17). Plan jednego kroku, który okno składa dla
 * wpisanego pytania, nie niesie ani pozycji, ani strzałek — i wtedy nie ma grafu, tylko lista
 * kroków. Zgadnięta pozycja i ozdobna krzywa między dwoma wymyślonymi punktami wyglądają
 * dokładnie tak samo jak zmierzone.
 *
 * CZEGO TU NIE MA: ANI JEDNEJ KONTROLKI. Kafelka nie da się przeciągnąć, strzałki narysować
 * ani niczego zaznaczyć — bieg nie jest miejscem, w którym zmienia się plan, a kontrolka bez
 * skutku nie wchodzi do repo (niezmiennik 16). Zmiana planu ma jedno miejsce i jest nim edytor.
 */
import type { Edge, Node, NodeProps } from '@xyflow/react';
import {
  Background,
  Handle,
  MarkerType,
  Position,
  ReactFlow,
  ReactFlowProvider,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
/* PO arkuszu biblioteki, nigdy przed — powód w całości stoi w nagłówku tamtego pliku.
 * Tłumaczenie zmiennych React Flow na tokeny tego repo jest JEDNO dla obu płócien: druga
 * kopia rozjechałaby kolor strzałki w edytorze z kolorem strzałki w biegu, a to jest ta sama
 * strzałka. */
import '../../workflows/canvas/react-flow-tokens.css';
import './graph.css';
import type { MouseEvent as ReactMouseEvent, ReactElement } from 'react';
import { Fragment, createContext, useContext, useEffect, useMemo, useRef } from 'react';
import { GRID } from '../../../state/workflows';
import { Asked } from '../feed/feed';
import type { GraphStep, Plan, TileData } from './model';
import { arrowsOf, hasLayout, tilesOf } from './model';
import { RunTile } from './tile';

type StepNode = Node<TileData>;

/** Plan dla kafelków.
 *
 * Kontekst, a nie kopia planu w `data` każdego kafelka: czwarta linia liczy się z WSZYSTKICH
 * strzałek i nazywa poprzednika po nazwie, więc kafelek potrzebuje całego planu. Piętnaście
 * kopii jednego planu rozjeżdża się dokładnie wtedy, gdy przyjdzie nowy stan kroku, a któraś
 * z nich jeszcze o tym nie wie. */
interface Showing {
  readonly plan: Plan;
  /**
   * Wejście w tego, kto ten krok robi — albo `null`, kiedy wołający nie ma dokąd wpuścić.
   *
   * Jedzie kontekstem razem z planem, a nie propsem kafelka: `nodeTypes` musi być STAŁĄ poza
   * komponentem (React Flow przemontowuje wszystkie kafelki przy każdej nowej referencji), więc
   * komponent kafelka nie ma jak dostać niczego od wołającego inaczej niż tędy.
   */
  readonly open: ((stepId: string) => void) | null;
  /** Odpowiedź człowieka na pytanie stojące przy kroku. Tą samą drogą, co z dołu strumienia. */
  readonly answer: ((questionId: number, option: string) => void) | null;
}

const Shown = createContext<Showing | null>(null);

/** Kafelek na płótnie: dwie kotwice i karta z `tile.tsx`.
 *
 * Kotwice są TUTAJ, a nie w `RunTile`: `<Handle>` czyta magazyn React Flow, więc kafelek
 * z kotwicą nie dałby się wyrenderować poza `<ReactFlow>` — czyli nie dałby się sprawdzić
 * w środowisku `node`, w którym biegną wszystkie kryteria tego repo. */
function CanvasTile({ data }: NodeProps<StepNode>): ReactElement | null {
  const showing = useContext(Shown);
  if (showing === null) return null;
  const open = showing.open;

  return (
    <>
      <Handle type="target" position={Position.Top} />
      {/* 246 px z makiety (`.node`). Szerokość narzuca płótno, a nie karta: płótno układa
          kafelki w kolumny, więc kafelek rosnący z treścią przesuwałby sąsiadów przy każdym
          nowym zdaniu agenta — czyli dwadzieścia razy w ciągu jednego kroku. */}
      <div className="w-61.5">
        <RunTile
          step={data.step}
          plan={showing.plan}
          {...(open === null || data.step.who === undefined
            ? {}
            : {
                onOpen: () => {
                  open(data.step.id);
                },
              })}
        />
        <Asking step={data.step} answer={showing.answer} />
      </div>
      <Handle type="source" position={Position.Bottom} />
    </>
  );
}

/**
 * Pytanie tego kroku, POD jego kafelkiem — albo nic.
 *
 * SIOSTRA KAFELKA, NIGDY JEGO DZIECKO, i to jest wymuszone, nie wybrane: kafelek z wejściem
 * w agenta JEST przyciskiem (`./tile.tsx`), a pole tekstowe i przyciski wyboru wewnątrz
 * przycisku są markupem, którego przeglądarka nie obsługuje. Ta sama funkcja stoi na obu
 * drogach rysowania — płótno i lista — bo to jest jedno miejsce, w którym mieszka odpowiedź
 * „gdzie stoi karta pytania" (niezmiennik 13).
 *
 * `nodrag`/`nopan` z biblioteki: bez nich pociągnięcie w polu odpowiedzi na płótnie przesuwa
 * WIDOK zamiast zaznaczać tekst. Klasy są nazwami zachowania React Flow, nie stylem, więc nie
 * są kopią żadnej decyzji tego repo.
 */
function Asking({
  step,
  answer,
}: {
  step: GraphStep;
  answer: ((questionId: number, option: string) => void) | null;
}): ReactElement | null {
  if (step.asked === undefined || answer === null) return null;
  return (
    <div className="nodrag nopan mt-2">
      <Asked question={step.asked} onAnswer={answer} />
    </div>
  );
}

/* JEDEN RODZAJ KAFELKA. Rodzaj kroku (agent, punkt kontrolny, sprawdzenie, „uruchom i zostaw")
 * jest faktem o PLANIE i mówi go edytor; bieg pokazuje pracę, a praca wygląda tak samo
 * niezależnie od tego, z którego kafelka wyszła.
 *
 * Poza komponentem, bo React Flow przy każdej nowej referencji `nodeTypes` przemontowuje
 * wszystkie kafelki — czyli przerysowuje całe płótno przy każdej linii z drutu. */
const NODE_TYPES = { step: CanvasTile };

/** Grot strzałki. Kolor bierze `--xy-edge-stroke`, więc grot nie rozjedzie się z linią. */
const ARROW = { type: MarkerType.ArrowClosed } as const;

/** SUFIT POWIĘKSZENIA 1. Domyślny `maxZoom` React Flow to 2, a `fitView` na planie o dwóch
 * krokach sięga po ten sufit — u właściciela cały edytor rysował się dwukrotnie za duży i to
 * była najbardziej rzucająca się w oczy rozbieżność z makietą. Powiększenie WYŻEJ niż 1:1 nie
 * pokazuje niczego więcej: kafelek nie ma drugiego poziomu szczegółu. */
const FIT = { maxZoom: 1 } as const;

/** Plakietka biblioteki jest linkiem NA ZEWNĄTRZ aplikacji desktopowej — jedyne wyjście z okna,
 * którego nikt nie zaprojektował, w rogu ekranu, na którym człowiek pilnuje swojej pracy. */
const PRO = { hideAttribution: true } as const;

export interface RunGraphProps {
  plan: Plan;
  /**
   * Co robi kliknięcie w kafelek — albo nic, kiedy wołający nie ma dokąd wpuścić.
   *
   * DRZWI SĄ DOKŁADNIE TAM, GDZIE WIEMY, KTO ZA NIMI STOI: kafelek dostaje przycisk wtedy
   * i tylko wtedy, gdy krok niesie `who`. Krok, o którym strumień jeszcze nic nie powiedział,
   * nie ma agenta do pokazania, a przycisk otwierający pusty ekran jest kontrolką bez skutku
   * z dodatkowym krokiem (niezmiennik 16).
   *
   * 2026-08-31: prawa kolumna ekranu pracy zniknęła, a była jedyną drogą do ekranu jednego
   * agenta. Bez tego propsa `openAgent`, `session/` i `rerun_step` zostają mechanizmem bez
   * ani jednego produkcyjnego wołającego (niezmiennik 16).
   */
  onOpen?: (stepId: string) => void;
  /**
   * Odpowiedź człowieka na pytanie stojące przy kroku — albo brak, i wtedy karty tu nie ma.
   *
   * TA SAMA DROGA, CO Z DOŁU STRUMIENIA: wołający podaje tę samą funkcję, którą podaje
   * komponentowi strumienia (`../index.tsx`, `answerQuestion`), więc odpowiedź jedzie jednym
   * torem niezależnie od tego, w którym z dwóch miejsc karta akurat stoi. Druga droga do
   * odblokowania biegu jest dokładnie tym, co rozjeżdża się po cichu (niezmiennik 13).
   *
   * Brak propsa znaczy „ten rysunek nie umie przyjąć odpowiedzi", a wtedy karty nie ma wcale:
   * karta z przyciskami, które nic nie robią, jest gorsza od jej braku (niezmiennik 16).
   */
  onAnswer?: (questionId: number, option: string) => void;
}

/** Kafelki dla React Flow. */
export function nodesFor(plan: Plan): StepNode[] {
  return tilesOf(plan).map((tile) => ({ ...tile }));
}

/**
 * Strzałki dla React Flow.
 *
 * FUNKCJA, A NIE WYRAŻENIE W ŚRODKU KOMPONENTU, i to jest jedyna droga, żeby udowodnić, że
 * odpowiedź modelu („tą strzałką idzie teraz praca") naprawdę DOCHODZI do rysującego. React
 * Flow pod `renderToStaticMarkup` oddaje pusty pojemnik na krawędzie, więc z markupu nie da się
 * tego przeczytać ani razu — a wyrażenie schowane w komponencie jest dokładnie tym, co przy
 * regresji zostaje zielone: model dalej wie, kto pracuje, i nikt tego nie rysuje (niezmiennik 29).
 */
export function edgesFor(plan: Plan): Edge[] {
  return arrowsOf(plan).map((arrow) => ({ ...arrow, markerEnd: ARROW }));
}

/** Lista kroków — odpowiedź na plan, który nie niesie układu.
 *
 * BEZ ANI JEDNEGO ZDANIA WYJAŚNIENIA. „This run has no shape to draw" byłoby zdaniem o czymś,
 * czego w danych nie ma: nie wiemy, czy plan układu nie ma, czy tylko nam go nie podano.
 * Milczenie o kształcie nie jest milczeniem o pracy — kroki stoją tu wszystkie, w kolejności
 * planu, tym samym kafelkiem, co na płótnie. */
function StepList({ plan, onOpen, onAnswer }: RunGraphProps): ReactElement {
  /* KROK, KTÓRY PRACUJE, MA BYĆ WIDOCZNY, a przy trzydziestu kilku krokach nie jest.
   *
   * Lista przewija się we własnym wycinku, więc krok, który właśnie idzie, potrafi stać poza nim
   * — a to jest dokładnie ta jedna rzecz, po którą człowiek na tę kolumnę patrzy. Zgłoszenie
   * właściciela 2026-08-23: „nie wiadomo które jak chodzą w sumie". Ta sama linia stała do
   * 2026-08-31 w pasku loadoutu i zeszła razem z jego torem bloków.
   *
   * BEZ RUCHU: `behavior` zostaje domyślne, czyli natychmiastowe. Sufit z ARCHITECTURE §7 to DWA
   * animujące się regiony na całą aplikację, a płynne przewijanie byłoby trzecim.
   *
   * `block: 'nearest'` pilnuje, żeby ruszyła TYLKO ta lista: bez tego przeglądarka ma prawo
   * poruszyć także stroną, a kolumna stoi u jej krawędzi i nie ma dokąd jechać.
   *
   * PIERWSZY pracujący, kiedy pracuje ich kilku. Bieg równoległy jest zwykłym biegiem
   * (niezmiennik 11), a wybieranie „ważniejszego" z trzech byłoby relacją, której w danych nie
   * ma (niezmiennik 17). */
  const list = useRef<HTMLDivElement>(null);
  /* KLUCZ, NIE POZYCJA W LIŚCIE, i to jest naprawa z 2026-08-31, nie kosmetyka: od dziś między
   * kafelkami stoi czasem karta pytania, więc n-te dziecko przestało być n-tym krokiem. Wersja
   * licząca po indeksie przewijała wtedy do SĄSIADA i nie było tego po czym poznać — kolumna
   * dalej się przewijała, tylko o jeden kafelek za daleko. */
  const working = plan.steps.find((step) => step.status === 'working')?.id ?? '';
  useEffect(() => {
    if (working === '') return;
    list.current
      ?.querySelector(`[data-step="${working}"]`)
      ?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [working]);

  return (
    <div ref={list} data-step-list className="grid content-start gap-2 overflow-auto p-2">
      {plan.steps.map((step) => (
        /* Fragment, a nie opakowanie: kafelki są BEZPOŚREDNIMI dziećmi listy i mają nimi zostać.
           Kryterium `e2e/tests/t161-long-workflow-stays-inside-run.spec.ts` liczy je jako
           `:scope > [data-step]`, a `<div>` na krok schowałby przed nim każdy z nich naraz. */
        <Fragment key={step.id}>
          <RunTile
            step={step}
            plan={plan}
            {...(onOpen === undefined || step.who === undefined
              ? {}
              : {
                  onOpen: () => {
                    onOpen(step.id);
                  },
                })}
          />
          <Asking step={step} answer={onAnswer ?? null} />
        </Fragment>
      ))}
    </div>
  );
}

export function RunGraph({ plan, onOpen, onAnswer }: RunGraphProps): ReactElement {
  const view = useMemo(
    () => ({ drawable: hasLayout(plan), tiles: nodesFor(plan), arrows: edgesFor(plan) }),
    [plan],
  );
  const showing = useMemo(
    () => ({ plan, open: onOpen ?? null, answer: onAnswer ?? null }),
    [plan, onOpen, onAnswer],
  );

  /**
   * Kliknięcie w kafelek NA PŁÓTNIE — i to jest naprawa, nie ozdoba obok przycisku w karcie.
   *
   * ZMIERZONE 2026-08-31 W PRAWDZIWYM CHROMIUM: kafelka na płótnie NIE DAŁO SIĘ KLIKNĄĆ ani
   * razu. React Flow stawia na opakowaniu kafelka `pointer-events: none`, kiedy nic w nim nie
   * jest ani wybieralne, ani przeciągalne, ani nie ma `onNodeClick`
   * (`@xyflow/react/dist/esm/index.js`, `hasPointerEvents`) — a płótno biegu jest tylko do
   * odczytu, więc wyłączone było wszystko naraz. Playwright melduje to jako
   * „<div class="react-flow__pane draggable"> intercepts pointer events". Martwy był przez to
   * CAŁY środek kafelka: wejście w krok i wszystkie kontrolki karty pytania (niezmiennik 16).
   * Lista kroków tej wady nie miała, bo nie przechodzi przez bibliotekę — czyli jedyna droga,
   * na której to widać, jest tą, którą rysuje się prawdziwy plan z pliku.
   *
   * TO NIE JEST CZWARTA DROGA GESTU DO PLANU: przeciąganie, łączenie i zaznaczanie zostają
   * wyłączone, więc płótno dalej niczego nie zmienia. Kliknięcie tylko OTWIERA to, co i tak
   * otwiera kafelek w liście.
   *
   * KARTA PYTANIA JEST WYJĘTA. Przyciski wyboru i pole odpowiedzi leżą w środku kafelka, więc
   * bez tego warunku naciśnięcie „Keep it" wysuwałoby przy okazji szufladę — czyli czynność
   * robiłaby dwie rzeczy, o jedną za dużo.
   */
  function pressed(event: ReactMouseEvent, node: StepNode): void {
    if (onOpen === undefined || node.data.step.who === undefined) return;
    if (event.target instanceof Element && event.target.closest('[data-asked]') !== null) return;
    onOpen(node.id);
  }

  if (!view.drawable)
    return (
      <StepList
        plan={plan}
        {...(onOpen === undefined ? {} : { onOpen })}
        {...(onAnswer === undefined ? {} : { onAnswer })}
      />
    );

  return (
    <Shown.Provider value={showing}>
      <div className="h-full min-h-0">
        <ReactFlowProvider>
          <ReactFlow
            className="loadout-run-canvas"
            nodes={view.tiles}
            edges={view.arrows}
            nodeTypes={NODE_TYPES}
            /* TYLKO DO ODCZYTU, i to są wszystkie trzy drogi, którymi płótno przyjmuje gest.
               Bieg nie jest miejscem, w którym zmienia się plan (niezmiennik 16). */
            nodesDraggable={false}
            nodesConnectable={false}
            elementsSelectable={false}
            /* Powód w całości stoi przy [`pressed`]: bez tego propsa środek kafelka nie
               przyjmuje ani jednego kliknięcia. */
            onNodeClick={pressed}
            fitView
            fitViewOptions={FIT}
            proOptions={PRO}
          >
            {/* Ta sama siatka, co w pliku i w edytorze: kafelek stoi tam, gdzie plik mówi,
                że stoi. */}
            <Background gap={GRID} />
          </ReactFlow>
        </ReactFlowProvider>
      </div>
    </Shown.Provider>
  );
}
