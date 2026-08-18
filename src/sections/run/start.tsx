/* Kontrolka „co uruchomić i ile naraz" — jedyne miejsce w oknie, przez które da się ZACZĄĆ bieg.
 *
 * DLACZEGO TEN PLIK W OGÓLE POWSTAŁ. `src/sections/run/io.ts` eksportuje `start` i `stop` od
 * T-27 i do 2026-08-17 nie miał ani jednego produkcyjnego wołającego: jedynym miejscem w repo,
 * które go importowało, był jego własny test. Silnik był gotowy, komendy zarejestrowane,
 * a okno nie miało czym ich zawołać — czyli aplikacji nie dało się uruchomić z aplikacji.
 * To ta sama rodzina, co zaślepki w sekcjach: mechanizm wylądował, nikt go nie podłączył.
 *
 * DLACZEGO WYBÓR Z LISTY, A NIE WIERSZ POLECEŃ. Makieta (`docs/mockup/index.html`) stawia tu
 * wiersz wejścia z `/plan · /run`, czyli parser komend — a parser, który rozumie jedno słowo,
 * jest gorszy niż lista, bo obiecuje więcej, niż umie. Lista czyta katalog przez ten sam
 * adapter, którego używa sekcja Workflow, więc nie powstaje druga odpowiedź na pytanie
 * „jakie workflow istnieją" (niezmiennik 13).
 *
 * Nazwa pliku jedzie do Rusta, nie cały workflow: to plik na dysku jest prawdą (niezmiennik 4),
 * a kopia treści wysłana z okna byłaby drugim opisem tego samego pliku — i tym, który się
 * rozjedzie, gdy ktoś zapisze workflow między wyborem a kliknięciem.
 *
 * 2026-08-18 — STAN BIEGU CZYTAMY SAMI, I TO JEST WYMÓG, NIE GUST (T-38 AC-3, AC-4).
 * `src/sections/run/index.tsx` należy do T-29 i tego zadania nie da się zrobić przez zmianę
 * jego propsów. Ale powód jest głębszy niż podział plików: Stop i „Continue" pytają o dwa
 * różne fakty, a każdy z nich ma dokładnie jednego właściciela (niezmiennik 13) — czy coś
 * biegnie wie magazyn biegu (`RunState.workflow`), a czy bieg stoi na pytaniu do człowieka
 * wie model widoku pracy (`feed/model.ts`, pole `pinned`). Trzeci opis któregokolwiek z nich,
 * przepisany do propsa albo do własnego `useState`, rozjedzie się pierwszego dnia.
 */
import type { ReactElement } from 'react';
import { useEffect, useState, useSyncExternalStore } from 'react';

import { AtOnce, DEFAULT_AT_ONCE } from './limits/at-once';
import { list } from '../workflows/io';
import { runFeed } from './feed/live';
import type { FeedView } from './feed/model';
import { continueRun, start, stop } from './io';
import { useRun } from '../../state/run';
import type { Step as RunStep } from '../../state/run';
import type { Step as FileStep } from '../../state/workflows';

/** Pozycja listy: nazwa pliku, to, jak workflow nazywa sam siebie, i jego plan kroków. */
interface Choice {
  path: string;
  name: string;
  steps: readonly RunStep[];
}

const PRIMARY = 'h-9 rounded-sq bg-accent px-4 text-ui text-bg disabled:opacity-40';
const DANGER = 'h-7 rounded-sq border border-fail-edge px-3 text-ui text-fail';
/** Kolor `--attend` odpowiada na jedno pytanie: co czeka na MOJĄ decyzję [DESIGN §3]. */
const ATTEND = 'h-7 rounded-sq border border-attend-edge px-3 text-ui text-attend';
const FIELD = 'h-8 rounded-sq border border-line-strong bg-well px-2 font-mono text-mono text-ink';

/**
 * Plan biegu z pliku workflow: kafelki grafu w kolejności wstawiania, wszystkie jeszcze czekają.
 *
 * `pending` dla każdego, bo w chwili kliknięcia Start żaden krok nie ruszył. Blok `todo` jest
 * obrysem, nie obietnicą — to blok wypełniony obiecuje, że krok się udał [DESIGN §2], więc plan
 * pokazany od pierwszej sekundy nie mówi nic nieprawdziwego o tym, co się już wydarzyło.
 */
function planOf(steps: readonly FileStep[]): readonly RunStep[] {
  return steps.map((step) => ({ id: step.id, name: step.name, state: 'pending' as const }));
}

/**
 * Migawka magazynu biegu — TA SAMA dla okna i dla renderu statycznego.
 *
 * DLACZEGO NIE `useRun((state) => state.workflow)`. Wiązanie zustanda podaje `useSyncExternal
 * Store` jako migawkę serwerową `getInitialState()`, czyli stan SPRZED pierwszego zapisu.
 * W tej aplikacji nie ma serwera — okno renderuje się przez `createRoot`, więc produkcja tej
 * ścieżki nie dotyka — ale `renderToStaticMarkup` (jedyny renderer, jaki to repo ma w testach,
 * bo nie ma jsdom) dostaje właśnie ją. Komponent czytający magazyn tamtą drogą jest wtedy
 * NIEMOŻLIWY do sprawdzenia: pokazuje „nic nie biegnie" niezależnie od tego, co w magazynie
 * naprawdę stoi. `index.tsx` czyta tak `runFeed` i pisze przy tym dokładnie to zdanie.
 */
function runningWorkflow(): string {
  return useRun.getState().workflow;
}

/** Ta sama migawka dla okna i dla renderu statycznego; model widoku nie ma stanu serwerowego. */
function currentView(): FeedView {
  return runFeed.view;
}

export interface StartProps {
  /**
   * Czy bieg trwa — POLE PRZYJMOWANE I ŚWIADOMIE NIECZYTANE, i to jest zapis decyzji, nie
   * przeoczenie.
   *
   * `src/sections/run/index.tsx` woła dziś `<Start running={run.workflow !== ''} />`, nie
   * należy do tego zadania i nie wolno go tknąć, więc usunięcie tego pola wywróciłoby
   * kompilację cudzego pliku. Czytanie go byłoby jednak gorsze: „czy coś biegnie" ma jednego
   * właściciela (`RunState.workflow`, niezmiennik 13), a komponent biorący tę odpowiedź
   * z propsa robi z siebie drugie miejsce, w którym ona mieszka — i wtedy test sprawdzający
   * ścieżkę magazynową sprawdza inną drogę niż ta, którą chodzi aplikacja. Zdanie z kontraktu
   * T-38 jest tu dosłowne: Stop **czyta stan sam, zamiast dostawać go propsem**.
   *
   * Wartość, którą tamten ekran podaje, jest dziś CO DO ZNAKU tą, którą ten komponent czyta
   * z magazynu (`useSyncExternalStore(useRun.subscribe, …)` po obu stronach), więc pominięcie
   * jej niczego nie zmienia w oknie. Pole znika w dniu, w którym `index.tsx` da się dotknąć.
   */
  running?: boolean;
}

export function Start(_props: StartProps): ReactElement {
  const [choices, setChoices] = useState<readonly Choice[]>([]);
  const [picked, setPicked] = useState('');
  const [atOnce, setAtOnce] = useState(DEFAULT_AT_ONCE);
  const [said, setSaid] = useState<string | null>(null);

  const workflow = useSyncExternalStore(useRun.subscribe, runningWorkflow, runningWorkflow);
  const view = useSyncExternalStore(runFeed.subscribe, currentView, currentView);

  /* Bieg trwa dokładnie wtedy, kiedy magazyn zna jego workflow — ta sama prawda, z której żyje
   * pasek loadoutu, i jedyna, jakiej ten komponent słucha (patrz `StartProps.running`). */
  const busy = workflow !== '';

  /* Bieg stoi na punkcie kontrolnym dokładnie wtedy, kiedy czeka nieodpowiedziane pytanie.
   * `Line::Asked` powstaje po stronie Rusta w JEDNYM miejscu — `commands::run::ask`, wołanym
   * z `wait_for_a_person` — więc przypięte pytanie i zaparkowany bieg to ten sam fakt. */
  const atCheckpoint = view.pinned !== null;

  /* Katalog czytamy przy wejściu na sekcję. Pliki są prawdą, a ekran jest ich widokiem —
   * lista trzymana w pamięci między wejściami pokazywałaby workflow skasowany obok. */
  useEffect(() => {
    let alive = true;
    list()
      .then((entries) => {
        if (!alive) return;
        setChoices(
          entries.map((e) => ({
            path: e.path,
            name: e.workflow.name,
            steps: planOf(e.workflow.steps),
          })),
        );
      })
      .catch((error: unknown) => {
        if (!alive) return;
        /* Odmowa Rusta jest już napisana po ludzku; własne zdanie dokładamy tylko wtedy,
         * gdy jego nie ma — cicha porażka czyta się jak pusty katalog. */
        const why = error instanceof Error ? error.message.trim() : '';
        setSaid(why.length > 0 ? why : 'Loadout could not read the workflows folder.');
      });
    return () => {
      alive = false;
    };
  }, []);

  const chosen = picked === '' ? (choices[0]?.path ?? '') : picked;

  async function go(): Promise<void> {
    if (chosen === '') return;
    setSaid(null);
    /* Nazwa i plan jadą razem z kliknięciem, bo TU są znane: lista wyboru powstaje z tych
     * samych plików workflow. Rust ich nie odeśle — `run_workflow` oddaje `()` (zapisany dług
     * w `src-tauri/src/ipc.rs`) — a pasek loadoutu ma pokazać plan od pierwszej sekundy,
     * nie dorysowywać go z linii w trakcie biegu (niezmiennik 17). */
    const chosenOne = choices.find((choice) => choice.path === chosen);
    try {
      await start(chosen, atOnce, {
        /* Nazwa pliku tylko wtedy, gdy pozycji nie ma na liście — czyli gdy katalog zmienił
         * się między odczytem a kliknięciem. Prawdziwa, choć nie ta, którą workflow nadał
         * sobie sam; pusty podpis paska byłby gorszy. */
        name: chosenOne?.name ?? chosen,
        steps: chosenOne?.steps ?? [],
      });
    } catch (error: unknown) {
      const why = error instanceof Error ? error.message.trim() : '';
      setSaid(why.length > 0 ? why : 'Loadout could not start that run.');
    }
  }

  async function carryOn(): Promise<void> {
    setSaid(null);
    try {
      await continueRun();
    } catch (error: unknown) {
      const why = error instanceof Error ? error.message.trim() : '';
      setSaid(why.length > 0 ? why : 'Loadout could not let that run carry on.');
    }
  }

  async function halt(): Promise<void> {
    setSaid(null);
    try {
      await stop();
    } catch (error: unknown) {
      const why = error instanceof Error ? error.message.trim() : '';
      setSaid(why.length > 0 ? why : 'Loadout could not stop the run.');
    }
  }

  return (
    <div className="flex shrink-0 flex-col gap-2 rounded-sq border border-line bg-panel p-3">
      <div className="flex items-center gap-3">
        <select
          aria-label="Workflow to run"
          className={FIELD}
          value={chosen}
          disabled={busy || choices.length === 0}
          onChange={(event) => {
            setPicked(event.target.value);
          }}
        >
          {choices.length === 0 ? (
            <option value="">No workflows saved yet</option>
          ) : (
            choices.map((choice) => (
              <option key={choice.path} value={choice.path}>
                {choice.name}
              </option>
            ))
          )}
        </select>

        {/* Kontrolka „dalej" istnieje DOKŁADNIE wtedy, kiedy ma co puścić (niezmiennik 16).
            Wersja stale obecna i wyszarzona obiecuje sterowanie, którego nie ma, a wersja
            wyrenderowana bez zaparkowanego biegu woła `continue_run` w próżnię: Rust podbija
            wtedy licznik zgód i NASTĘPNY punkt kontrolny przelatuje bez pytania. */}
        {atCheckpoint ? (
          <button type="button" className={ATTEND} onClick={() => void carryOn()}>
            Continue
          </button>
        ) : null}

        {busy ? (
          <button type="button" className={DANGER} onClick={() => void halt()}>
            Stop
          </button>
        ) : (
          <button
            type="button"
            className={PRIMARY}
            disabled={chosen === ''}
            onClick={() => void go()}
          >
            Start
          </button>
        )}
      </div>

      {/* Limit siedzi obok Startu, a nie w ustawieniach: to decyzja podejmowana przy każdym
       * biegu, bo zależy od tego, co jeszcze chodzi na tej maszynie. */}
      <AtOnce value={atOnce} onChange={setAtOnce} />

      {said === null ? null : (
        <p data-said className="text-body text-fail">
          {said}
        </p>
      )}
    </div>
  );
}
