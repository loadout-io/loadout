/* Ekran sekcji `run`. Powłoka znajduje go po ŚCIEŻCE — `src/sections/<id>/index.tsx` — więc
 * ten plik jest całym wpisem do rejestru i nie ma żadnego drugiego miejsca, w którym trzeba by
 * go zadeklarować (T-25, HARNESS-QUEUE.md Q-5).
 *
 * Cienki z premedytacją: składa pasek loadoutu i widok pracy, i nic poza tym. Druga
 * implementacja czegokolwiek z `feed/` albo `strip/` tutaj byłaby drugim miejscem prawdy
 * o tej samej rzeczy (niezmiennik 23).
 *
 * Trzy oznaczone regiony i ani jednego więcej: `data-strip`, `data-feed`, `data-now`. Sufit
 * gęstości mówi 8 na ekran [ARCHITECTURE §7], a powłoka wydała już swoje na chrome — ten ekran
 * bierze trzy.
 *
 * SKĄD BIERZE SIĘ TREŚĆ. Z dwóch źródeł i każde odpowiada na inne pytanie. Model widoku
 * (`feed/live.ts`) trzyma wiersze historii, strefę TERAZ i przypięte pytanie; magazyn
 * (`state/run.ts`) trzyma okno linii i plan biegu, z którego rysuje się pasek. Oba są na
 * poziomie modułu, bo bieg trwa dłużej niż ten ekran: wyjście do Agentów odmontowuje komponent
 * i nie ma prawa skasować biegu.
 *
 * Zdarzeń z Rusta ten plik nie subskrybuje — kanał dowozi T-07, a stemplowanie wiersza z drutu
 * (`id`, `at`) jest decyzją tamtej granicy. Kiedy się domknie, paczka wchodzi dwoma wywołaniami
 * opisanymi w `feed/live.ts` i ten plik nie zmienia się ani o linię.
 */
import { useMemo, useSyncExternalStore } from 'react';
import type { ReactElement } from 'react';
import { useRun } from '../../state/run';
import { Feed } from './feed/feed';
import { attachPort, runFeed } from './feed/live';
import type { FeedView } from './feed/model';
import { Now } from './feed/now';
import { stripFor } from './strip/model';
import { Strip } from './strip/strip';

/* Ta sama migawka dla okna i dla renderu serwerowego. Model nie ma stanu „po stronie serwera":
 * `renderToStaticMarkup` widzi po prostu bieg, którego jeszcze nie ma. */
function currentView(): FeedView {
  return runFeed.view;
}

export default function Run(): ReactElement {
  const view = useSyncExternalStore(runFeed.subscribe, currentView, currentView);
  const workflow = useRun((state) => state.workflow);
  const steps = useRun((state) => state.steps);
  const strip = useMemo(() => stripFor(workflow, steps), [workflow, steps]);

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <Strip strip={strip} />
      <Feed
        view={view}
        portRef={attachPort}
        onToggle={runFeed.toggle}
        onAnswer={runFeed.answer}
        onJumpToNewest={runFeed.jumpToNewest}
      />
      <Now now={view.now} />
    </div>
  );
}
