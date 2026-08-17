/* Ekran sekcji `run`. Powłoka znajduje go po ŚCIEŻCE — `src/sections/<id>/index.tsx` — więc
 * ten plik jest całym wpisem do rejestru i nie ma żadnego drugiego miejsca, w którym trzeba by
 * go zadeklarować (T-25, HARNESS-QUEUE.md Q-5).
 *
 * Cienki z premedytacją: składa pasek loadoutu i widok pracy, i nic poza tym. Druga
 * implementacja czegokolwiek z `feed/` albo `strip/` tutaj byłaby drugim miejscem prawdy
 * o tej samej rzeczy (niezmiennik 23).
 *
 * Trzy oznaczone regiony treści i ani jednego więcej: `data-strip`, `data-feed`, `data-now`.
 * Sufit gęstości mówi 8 na ekran [ARCHITECTURE §7], a powłoka wydała już swoje na chrome —
 * ten ekran bierze trzy.
 *
 * NAGŁÓWEK SEKCJI, dopisany 2026-08-17 (T-29, kryterium 2) — i ZGŁOSZONY, nie przemilczany.
 * Cztery pozostałe sekcje nazywają się własnym `<h1>` w pasku `h-13` i to jest tutaj ta sama
 * konwencja, co u nich: ekran, który rysuje treść, nie mówiąc, na której sekcji stoisz,
 * przechodzi „coś się zamontowało" i nie odpowiada na nic. Kosztu tego paska nie ma jednak
 * kto zapłacić z budżetu chrome i to jest fakt do decyzji człowieka, nie do cichego wyboru
 * tego pliku: ARCHITECTURE §7 daje 96 px nad pierwszą treścią, karty biorą 34, a pasek
 * loadoutu — który mieszka WYŁĄCZNIE na tym ekranie — bierze 56. Zostawało sześć pikseli,
 * a `h-13` to 52. Egzekutora tej liczby dziś nie ma (`checks/_quick-density.sh` jest odstawiony
 * do czasu kolektora z T-27), więc bramka tego nie złapie — dlatego stoi to tu napisane.
 * Domknięcie należy do zadania, które posiada pasek loadoutu: albo nazwa sekcji wchodzi
 * W ten pasek, albo któryś z dwóch znika. Kryterium 2 pyta o nagłówek i dostaje nagłówek.
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
import { Start } from './start';
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
    <section className="flex h-full min-h-0 flex-col">
      {/* Ten sam pasek, co w Agents, Skills, Memory i Workflows — jedna konwencja na pięć
          sekcji, a nie pięć wariantów tej samej odpowiedzi na pytanie „gdzie jestem". */}
      <header className="flex h-13 items-center gap-3 border-b border-line bg-panel px-4">
        <h1 className="text-title text-ink">Run</h1>
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-3">
        <Strip strip={strip} />
        {/* Bieg trwa wtedy, gdy magazyn zna jego workflow — ta sama prawda, z której żyje
         * pasek loadoutu, więc nie powstaje drugi opis stanu biegu (niezmiennik 13).
         *
         * 2026-08-18, ROZWIĄZANIE KONFLIKTU: gałąź T-29 powstała PRZED tą kontrolką i nie
         * znała jej wcale, a `main` nie znał nagłówka sekcji. Wzięcie którejkolwiek strony
         * w całości kasowało cudzą pracę: bez nagłówka pada konwencja pięciu sekcji, bez
         * Startu nie da się uruchomić biegu. Stąd oba. */}
        <Start running={workflow !== ''} />
        <Feed
          view={view}
          portRef={attachPort}
          onToggle={runFeed.toggle}
          onAnswer={runFeed.answer}
          onJumpToNewest={runFeed.jumpToNewest}
        />
        <Now now={view.now} />
      </div>
    </section>
  );
}
