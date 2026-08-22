/* Szew ekranu agenta: skąd bierze się jego treść i kiedy ten ekran w ogóle istnieje.
 *
 * TU MIESZKA CAŁE „SKĄD", a `session.tsx` nie wie o magazynach nic. Podział jest ten sam, co
 * między `feed/model.ts` i `feed/feed.tsx`: sekcje liczy funkcja czysta, którą da się osądzić
 * bez okna (niezmiennik 15), a ten plik tylko podaje jej to, co okno naprawdę wie.
 *
 * DLACZEGO CZYTAMY MAGAZYNY SAMI, A NIE PROPSEM Z EKRANU PRACY. Ekran pracy (`../index.tsx`)
 * nie należy do tego zadania, a każdy nowy props na jego granicy jest zmianą w cudzym pliku.
 * Ważniejsze: „co jest w strumieniu" i „co jest w planie biegu" mają po jednym właścicielu
 * (`runFeed`, `useRun`), a komponent, który dostawałby je propsem od rodzica czytającego to samo,
 * byłby drugą drogą do tej samej odpowiedzi (niezmiennik 13). Jedyne, co przychodzi propsem, to
 * KAFELKI — bo ich policzenie należy do listy agentów i tam ma zostać.
 *
 * DLACZEGO `useSyncExternalStore` Z BIEŻĄCYM STANEM JAKO MIGAWKĄ SERWEROWĄ. `renderToStaticMarkup`
 * jest rendererem serwerowym, a zustand 5 podaje mu `getInitialState` — więc ekran czytany hakiem
 * pokazywałby stan Z CHWILI UTWORZENIA magazynu i nigdy tego, co do niego weszło. Ta aplikacja
 * nigdy nie hydratuje serwerowego HTML-a, więc powód, dla którego React chce tam stanu
 * początkowego, tutaj nie istnieje. Ten sam zapis stoi w `../index.tsx` i w `../start.tsx`.
 *
 * CZEGO OKNO NIE MA, więc czego ten ekran nie pokazuje (niezmiennik 17). Trzy z pięciu wierszy
 * bloku „co dostał" nie mają dziś ŻADNEGO nośnika po tej stronie granicy i dlatego jadą tu jako
 * puste listy, a nie jako zgadnięte wartości:
 *   brief kroku      `AgentStep.instructions` jest czytany z dysku i gubiony w `../choices.ts`
 *                    (`planOf` przepisuje `id`, `name`, `state` i nic więcej),
 *   przekazania      `list_handoffs` istnieje po stronie Rusta, ale sekcja Bieg nie ma do niej
 *                    krawędzi, a `Line::Handoff` NIE JEST produkowane przez strumień
 *                    (`engine/line.rs`, akapit o wariantach, których strumień nie produkuje),
 *   notatki w użyciu `list_notes` zna sekcja Pamięć; „która notatka poszła do promptu TEGO
 *                    kroku" nie jest polem, które ktokolwiek dziś wysyła.
 * Wszystkie trzy są zgłoszone. Wiersz zgadnięty wyglądałby dokładnie jak wiersz z danymi.
 */
import { useSyncExternalStore } from 'react';
import type { ReactElement } from 'react';

import { useRun } from '../../../state/run';
import { runStepAgain } from '../rail/again';
import type { Step } from '../../../state/run';
import { runFeed } from '../feed/live';
import type { FeedView } from '../feed/model';
import type { RailCard } from '../rail/card';
import { changesOf } from './changes';
import type { StepBrief } from './layout';
import { sessionSections } from './layout';
import { closeAgent, openedAgent, subscribeToOpenAgent } from './open';
import { Session } from './session';

export interface AgentScreenProps {
  /** Kafelki listy agentów — jedno źródło nazwy, roli, koloru i stanu (niezmiennik 13). */
  readonly cards: readonly RailCard[];
  /** Zdanie dla czlowieka po powtorzeniu kroku. Brak propsu = ten ekran nie umie go pokazac. */
  readonly onSaid?: (text: string) => void;
}

/* Ta sama migawka dla okna i dla renderu serwerowego. Model nie ma stanu „po stronie serwera":
 * `renderToStaticMarkup` widzi po prostu bieg, którego jeszcze nie ma. */
function currentView(): FeedView {
  return runFeed.view;
}

/**
 * Kroki, na których stoi TEN agent.
 *
 * PODPIS AGENTA W STRUMIENIU TO NAZWA KROKU i nie jest to domysł: pompa zdarzeń startuje jako
 * `forward(…, self.plan.steps[id].name.clone())` (`src-tauri/src/commands/run.rs`), więc pole
 * `agent` każdej linii niesie nazwę kroku. To samo dopasowanie robi lista agentów
 * (`../index.tsx`, `factsOf`) — jedna reguła, dwa miejsca odczytu, żadnej drugiej definicji.
 */
function stepsOf(steps: readonly Step[], agent: string): readonly StepBrief[] {
  return steps
    .filter((step) => step.name === agent)
    .map((step) => ({ agent: step.name, name: step.name, brief: '', files: [] }));
}

/**
 * Ekran otwartego agenta — albo `null`, kiedy żaden nie jest otwarty.
 *
 * `null` także wtedy, gdy otwarty podpis nie ma kafelka w tym zakresie: kafelek istnieje wtedy
 * i tylko wtedy, gdy agent pojawił się w strumieniu TEGO workspace'a, więc po przełączeniu
 * zakresu ekran cudzego agenta gaśnie sam. Identyfikator zostaje zapamiętany, żeby powrót do
 * tamtego folderu wrócił do tego samego agenta — sesji się nie traci.
 */
export function AgentScreen({ cards, onSaid }: AgentScreenProps): ReactElement | null {
  const opened = useSyncExternalStore(subscribeToOpenAgent, openedAgent, openedAgent);
  const view = useSyncExternalStore(runFeed.subscribe, currentView, currentView);
  const run = useSyncExternalStore(useRun.subscribe, useRun.getState, useRun.getState);

  const card = cards.find((one) => one.id === opened);
  if (card === undefined) return null;

  const sections = sessionSections(
    { id: card.id, name: card.name },
    {
      view,
      steps: stepsOf(run.steps, card.id),
      handoffs: [],
      changes: changesOf(run.lines, card.id),
      notes: [],
    },
  );

  /* Powtórzenie dostaje wyłącznie krok, który JEST w grafie: pod-agent rozpuszczony w trakcie
   * biegu nie ma czego powtórzyć, więc jego ekran nie dostaje przycisku. */
  const step = card.stepId;

  return (
    <Session
      card={card}
      sections={sections}
      onBack={closeAgent}
      onToggle={runFeed.toggle}
      {...(step === null || step === undefined || onSaid === undefined
        ? {}
        : {
            onRunAgain: () => {
              runStepAgain(step, onSaid ?? ((): void => undefined));
            },
          })}
    />
  );
}
