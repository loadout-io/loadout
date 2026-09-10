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
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';

import type { PageMaker } from '../../state/context';
import { useAgentApps } from '../../state/agent-apps';
import { createContextStore, matching } from '../../state/context';
import * as io from './io';
import { activeWorkspace } from '../../state/workspaces';
import ContextEditor from './editor';

/** Prawdziwy magazyn tej sekcji — jeden na okno, wstrzyknięty krawędzią z `./io.ts`. */
export type ContextStore = ReturnType<typeof createContextStore>;

// 2026-09-10: powrót z innej sekcji zachowuje otwarty zestaw i jego budowanie,
// ale inny projekt dostaje osobny magazyn oraz adapter z własnym katalogiem.
const projectStores = new Map<string | null, ContextStore>();
function contextForProject(folder: string | null): ContextStore {
  let store = projectStores.get(folder);
  if (store === undefined) {
    store = createContextStore(io.forProject(folder));
    projectStores.set(folder, store);
  }
  return store;
}

export interface ContextScreenProps {
  /** Bez propsu ekran bierze prawdziwy magazyn, z propsem ten z testu. */
  store?: ContextStore;
}

/** Zdanie, kiedy katalog jeszcze nie odpowiedział — trzecia odpowiedź, nie druga. */
const STILL_READING = 'Reading the context sets you have saved.';
const READING_THE_FOLDER = 'Loadout is looking through the sets you keep for your work.';

/**
 * Sterownik dokumentów, doczytywany DOPIERO przy pierwszym `Prepare`.
 *
 * Biblioteka rysująca strony waży więcej niż cała reszta tej sekcji razem, a większość wejść
 * do zestawu nie przygotowuje niczego. Doczytanie stoi TUTAJ, a nie w magazynie: magazyn nie
 * zna ani nazw komend, ani bibliotek, które rysują (niezmiennik 23). Nieudane doczytanie leci
 * wyjątkiem prosto w `prepareSource`, więc kończy się zdaniem na ekranie, nie ciszą.
 */
const readTheDocument: PageMaker = async (file) => {
  const { openTheDocument } = await import('./pdf-preparation');
  return openTheDocument(file);
};

/** Przeczytaliśmy i naprawdę nic tam nie ma (DESIGN §6: pusty stan jest zaproszeniem). */
const NOTHING_YET = 'No context sets yet.';
const WHAT_A_SET_IS =
  'A set holds the material you want an agent to work from — notes, requirements, what matters.';

export default function ContextScreen({ store: supplied }: ContextScreenProps): ReactElement {
  const [ownStore] = useState(() => contextForProject(activeWorkspace()?.folder ?? null));
  const store = supplied ?? ownStore;
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  const apps = useSyncExternalStore(
    useAgentApps.subscribe,
    useAgentApps.getState,
    useAgentApps.getState,
  );
  const [typed, setTyped] = useState('');
  const preparation = useRef<AbortController | null>(null);
  useEffect(() => () => preparation.current?.abort(), []);

  const prepare = async (sourceId: string): Promise<void> => {
    if (preparation.current !== null) return;
    const operation = new AbortController();
    preparation.current = operation;
    try {
      await store.getState().prepareSource(sourceId, readTheDocument, operation.signal);
    } finally {
      if (preparation.current === operation) preparation.current = null;
    }
  };

  const buildSavedMaterial = async (): Promise<void> => {
    if (preparation.current !== null) return;
    const opened = store.getState().open;
    if (opened === null) return;
    const operation = new AbortController();
    preparation.current = operation;
    try {
      // 2026-09-09: przygotowanie PDF należy do tego samego kliknięcia co build.
      // Kolejny dokument startuje dopiero po zapisaniu stron poprzedniego.
      for (const source of opened.draft.sources) {
        if (
          opened.draft.excluded.includes(source.id) ||
          source.kind !== 'pdf' ||
          source.preparation?.state !== 'needs'
        )
          continue;
        await store.getState().prepareSource(source.id, readTheDocument, operation.signal);
        if (
          operation.signal.aborted ||
          store.getState().refusal !== null ||
          store.getState().open?.set.id !== opened.set.id
        )
          return;
        const prepared = store
          .getState()
          .open?.draft.sources.find((one) => one.id === source.id)?.preparation;
        if (prepared?.state !== 'ready') {
          store.setState({
            refusal:
              prepared?.state === 'failed'
                ? prepared.said
                : 'This document needs preparing before the context can be built.',
          });
          return;
        }
      }
      if (!operation.signal.aborted && store.getState().open?.set.id === opened.set.id) {
        await store.getState().startBuild();
      }
    } finally {
      if (preparation.current === operation) preparation.current = null;
    }
  };

  /* Ponowne wejście odświeża pliki tylko tego projektu. Odmowa zostaje w jego magazynie
   * jako zdanie dla człowieka, a stan budowania wraca z dysku. */
  useEffect(() => {
    void store.getState().load();
    void useAgentApps.getState().check();
    const open = store.getState().open;
    if (open !== null) void store.getState().openSet(open.set.id);
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
          imported={state.imported}
          preview={state.preview}
          preparing={state.preparing}
          build={state.build}
          version={state.version}
          buildWith={state.buildWith}
          buildModel={state.buildModel}
          claudeCode={apps.claudeCode}
          codex={apps.codex}
          onSave={store.getState().save}
          onAdd={store.getState().addSources}
          onPrepare={prepare}
          onPreview={(sourceId, page) => {
            void store.getState().showSource(sourceId, page);
          }}
          onHidePreview={store.getState().hidePreview}
          onRemove={store.getState().dropSource}
          onChooseBuildWith={store.getState().chooseBuildWith}
          onBuildModel={store.getState().chooseBuildModel}
          onBuild={buildSavedMaterial}
          onStopBuild={() => {
            preparation.current?.abort();
            void store.getState().stopBuild();
          }}
          onSaveRevision={(edit) => {
            void store.getState().saveRevision(edit);
          }}
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
          {answered ? (
            <>
              <ShelfFilter archived={state.archived} store={store} />
              {state.archived ? null : (
                <NameAndCreate typed={typed} onType={setTyped} onCreate={create} />
              )}
            </>
          ) : null}
        </div>
      </Shell>
    );
  }

  const shown = matching(state.sets, state.search);
  return (
    <Shell>
      <div className="flex flex-col gap-4">
        <NameAndCreate typed={typed} onType={setTyped} onCreate={create} />

        <ShelfFilter archived={state.archived} store={store} />

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
                <article className="card flex w-full flex-col gap-2">
                  <button
                    data-context-set={set.id}
                    data-interactive
                    type="button"
                    className="flex w-full flex-col gap-1 text-left"
                    onClick={() => {
                      void store.getState().openSet(set.id);
                    }}
                  >
                    <span className="text-heading text-ink">{set.title}</span>
                    {set.description === '' ? null : (
                      <span className="lead">{set.description}</span>
                    )}
                    {/* JEDEN stan pozycji (PLAN §12). Dopóki nikt nie zbudował opracowania,
                        każdy zestaw stoi w tym samym miejscu drogi i mówi to jednym słowem. */}
                    <span className="value">
                      {set.latestReadyRevision === null ? 'Not prepared yet' : 'Ready'}
                    </span>
                  </button>
                  <div className="flex gap-2">
                    <button
                      data-archive-context={set.id}
                      type="button"
                      className="btn"
                      onClick={() => {
                        void store.getState().archive(set.id, !state.archived);
                      }}
                    >
                      {state.archived ? 'Restore' : 'Archive'}
                    </button>
                    <button
                      data-delete-context={set.id}
                      type="button"
                      className="btn-danger"
                      onClick={() => {
                        void store.getState().previewDelete(set.id);
                      }}
                    >
                      Delete
                    </button>
                  </div>
                </article>
              </li>
            ))}
          </ul>
        )}

        {state.deleting === null ? null : (
          <div data-delete-context-confirmation role="alertdialog" className="card stack">
            <p className="text-ink">{state.deleting.said}</p>
            <div className="flex gap-2">
              <button type="button" className="btn" onClick={store.getState().cancelDelete}>
                Keep set
              </button>
              <button
                data-confirm-delete-context
                type="button"
                className="btn-danger"
                onClick={() => {
                  void store.getState().confirmDelete();
                }}
              >
                Delete set
              </button>
            </div>
          </div>
        )}
      </div>
    </Shell>
  );
}

function ShelfFilter({
  archived,
  store,
}: {
  readonly archived: boolean;
  readonly store: ContextStore;
}): ReactElement {
  return (
    <div className="flex gap-2" aria-label="Context set shelf">
      <button
        data-context-filter="active"
        type="button"
        className={archived ? 'btn' : 'btn-primary'}
        onClick={() => {
          void store.getState().showArchived(false);
        }}
      >
        Active
      </button>
      <button
        data-context-filter="archived"
        type="button"
        className={archived ? 'btn-primary' : 'btn'}
        onClick={() => {
          void store.getState().showArchived(true);
        }}
      >
        Archived
      </button>
    </div>
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
