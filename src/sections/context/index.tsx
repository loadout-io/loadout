/* Ekran sekcji Context: biblioteka nazwanych zestawów materiałów.
 *
 * `Knowledge` odpowiada na pytanie „co model WIE o mojej pracy" i wchodzi do promptu samo.
 * `Context` odpowiada na „który materiał daję mu do TEJ roboty" i jest wybierany za każdym razem.
 * To są dwa różne pytania, więc są to dwie szuflady — scalone byłyby jednym wyborem w odpowiedzi
 * na dwa pytania, czyli tą samą wadą, którą Knowledge samo naprawia w drugą stronę.
 *
 * LISTA NIE POKAZUJE ZAWARTOŚCI WSZYSTKICH ZESTAWÓW NARAZ (PLAN §12): kafelek niesie nazwę,
 * krótki opis i jeden stan, a materiał widać dopiero po wejściu do zestawu.
 *
 * ODCZYT MIESZKA TUTAJ, w efekcie po zamontowaniu — tak samo jak w Knowledge i Workflows. Pusta
 * biblioteka i biblioteka, której nikt nie czytał, wyglądają w danych identycznie, więc magazyn
 * ma trzy stany, a nie dwa: dopóki nie odpowiedział, ekran mówi, że CZYTA.
 *
 * O migawce serwerowej zustanda i o `useSyncExternalStore` przeczytaj
 * w `src/sections/workflows/index.tsx`.
 */
import type { ReactElement } from 'react';
import { useEffect, useState, useSyncExternalStore } from 'react';

import { createContextStore, matching } from '../../state/context';
import * as io from './io';
import ContextEditor from './editor';

/** Prawdziwy magazyn tej sekcji — jeden na okno, wstrzyknięty krawędzią z `./io.ts`. */
const useContext = createContextStore(io);

export type ContextStore = typeof useContext;

export interface ContextScreenProps {
  /** Bez propsu ekran bierze prawdziwy magazyn, z propsem ten z testu. */
  store?: ContextStore;
}

/** Zdanie, kiedy katalog jeszcze nie odpowiedział — trzecia odpowiedź, nie druga. */
const STILL_READING = 'Reading the context sets you have saved.';
const READING_THE_FOLDER = 'Loadout is looking through the sets you keep for your work.';

/** Przeczytaliśmy i naprawdę nic tam nie ma (DESIGN §6: pusty stan jest zaproszeniem). */
const NOTHING_YET = 'No context sets yet.';
const WHAT_A_SET_IS =
  'A set holds the material you want an agent to work from — notes, requirements, what matters.';

export default function ContextScreen({ store = useContext }: ContextScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  const [typed, setTyped] = useState('');

  /* Biblioteka leży pod `home`, nie w projekcie, więc ten odczyt nie zależy od otwartego
   * zakresu i biegnie RAZ na zamontowanie. `void`, bo odmowa jest obsłużona w magazynie
   * i ląduje w jego stanie jako zdanie dla człowieka. */
  useEffect(() => {
    void store.getState().load();
  }, [store]);

  const create = (): void => {
    if (typed.trim() === '') return;
    void store.getState().create(typed);
    setTyped('');
  };

  if (state.open !== null) {
    return (
      <Shell>
        {/* `key` na identyfikatorze: wejście do INNEGO zestawu ma dać świeże pola, a nie tekst
            poprzedniego z podmienioną nazwą. */}
        <ContextEditor
          key={state.open.set.id}
          open={state.open}
          refusal={state.refusal}
          onSave={store.getState().save}
          onClose={store.getState().close}
        />
      </Shell>
    );
  }

  if (state.library === 'unreadable') {
    return (
      <Shell>
        <div
          data-refusal
          role="alert"
          className="fade-in flex h-full flex-col items-center justify-center gap-3 px-4 text-center"
        >
          <span aria-hidden className="mark">
            ◇
          </span>
          <p className="text-fail">{state.refusal}</p>
          {/* Jedyna czynność, jaka ma tu sens: nic innego nie da się zrobić, dopóki katalog nie
              odpowie. Woła DOKŁADNIE ten sam odczyt, co wejście do sekcji (niezmiennik 16). */}
          <button
            data-retry
            type="button"
            className="btn-primary"
            onClick={() => {
              void store.getState().load();
            }}
          >
            Try again
          </button>
        </div>
      </Shell>
    );
  }

  if (state.sets.length === 0) {
    const answered = state.library === 'read';
    return (
      <Shell>
        <div className="mx-auto flex h-full max-w-240 flex-col items-center justify-center gap-4">
          <span aria-hidden className="mark">
            ◇
          </span>
          {/* `data-empty` siedzi na elemencie, który niesie SAMO zdanie — nie na opakowaniu
              z glifem, zaproszeniem i polem. Tak mówi o sobie `src/App.tsx` i tak robią pozostałe
              sekcje; znacznik na opakowaniu daje każdej wyroczni akapit zamiast zdania. */}
          <p data-empty className="text-ink">
            {answered ? NOTHING_YET : STILL_READING}
          </p>
          <p className="lead max-w-160 text-center">
            {answered ? WHAT_A_SET_IS : READING_THE_FOLDER}
          </p>
          {answered ? <NameAndCreate typed={typed} onType={setTyped} onCreate={create} /> : null}
        </div>
      </Shell>
    );
  }

  const shown = matching(state.sets, state.search);
  return (
    <Shell>
      <div className="flex flex-col gap-4">
        <NameAndCreate typed={typed} onType={setTyped} onCreate={create} />

        <div className="flex flex-col gap-1">
          <label className="label" htmlFor="context-search">
            Search
          </label>
          <input
            id="context-search"
            className="field"
            value={state.search}
            onChange={(event) => {
              store.getState().narrow(event.target.value);
            }}
          />
        </div>

        {state.refusal === null ? null : (
          <p data-refusal role="alert" className="text-fail">
            {state.refusal}
          </p>
        )}

        {shown.length === 0 ? (
          /* Wyszukiwanie, które nic nie znalazło, NIE jest pustą biblioteką: zaproszenie
             „utwórz pierwszy zestaw" nad pełną półką mówiłoby nieprawdę o katalogu, w którym
             leżą pliki. Dlatego to zdanie nie nosi `data-empty` (niezmiennik 13). */
          <p className="lead">Nothing here goes by that name.</p>
        ) : (
          <ul className="grid content-start gap-3 sm:grid-cols-2">
            {shown.map((set) => (
              <li key={set.id}>
                <button
                  data-context-set={set.id}
                  data-interactive
                  type="button"
                  className="card flex w-full flex-col gap-1 text-left"
                  onClick={() => {
                    void store.getState().openSet(set.id);
                  }}
                >
                  <span className="text-heading text-ink">{set.title}</span>
                  {set.description === '' ? null : <span className="lead">{set.description}</span>}
                  {/* JEDEN stan pozycji (PLAN §12). Dopóki nikt nie zbudował opracowania, każdy
                      zestaw stoi w tym samym miejscu drogi i mówi to jednym słowem. */}
                  <span className="value">
                    {set.latestReadyRevision === null ? 'Not prepared yet' : 'Ready'}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </Shell>
  );
}

/** Pasek nagłówka i przewijane ciało — jedno miejsce, w którym pada nazwa sekcji. */
function Shell({ children }: { readonly children: ReactElement }): ReactElement {
  return (
    <section data-context-screen className="flex h-full flex-col">
      <header className="screen-head glass">
        <h1 className="text-title text-ink">Context</h1>
      </header>
      <div className="screen-body">{children}</div>
    </section>
  );
}

/**
 * Jedyne wejście, którym powstaje zestaw — nazwa i przycisk obok niej.
 *
 * Nazwa jest podawana od razu, a nie nadawana zestawowi domyślnie i poprawiana potem: „Untitled"
 * na liście jest nazwą, której człowiek nie wybrał, a lista nazwana za człowieka przestaje być
 * jego biblioteką po trzecim wpisie.
 */
function NameAndCreate({
  typed,
  onType,
  onCreate,
}: {
  readonly typed: string;
  readonly onType: (typed: string) => void;
  readonly onCreate: () => void;
}): ReactElement {
  return (
    <div className="flex w-full items-end gap-2">
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <label className="label" htmlFor="context-new">
          Name a new set
        </label>
        <input
          id="context-new"
          className="field"
          value={typed}
          onChange={(event) => {
            onType(event.target.value);
          }}
          onKeyDown={(event) => {
            /* Enter robi to samo, co przycisk obok — jedno wejście, dwa sposoby naciśnięcia.
               Pole, w którym Enter nic nie robi, każe sięgnąć po mysz w połowie pisania. */
            if (event.key === 'Enter') onCreate();
          }}
        />
      </div>
      <button
        data-create
        type="button"
        className="btn-primary"
        disabled={typed.trim() === ''}
        onClick={onCreate}
      >
        ＋ Create
      </button>
    </div>
  );
}
