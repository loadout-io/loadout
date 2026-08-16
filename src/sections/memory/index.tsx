/* Ekran sekcji Memory: nagłówek i DWIE STREFY — to, co czeka na człowieka, i to, co wchodzi
 * do promptu.
 *
 * ROZDZIAŁ STREF JEST TU CAŁYM PRODUKTEM. Notatka zaproponowana przez agenta nie wchodzi do
 * promptu, dopóki człowiek jej nie promuje [T6 §5.1, T-17] — a ekran wyświetlający obie
 * w jednym worku kasuje jedyną widoczną różnicę między tym, co zaproponował agent, a tym, co
 * zatwierdził człowiek. Jedna płaska lista przechodzi „obie notatki są w dokumencie"
 * i unieważnia sekcję.
 *
 * CIENKI Z ZAŁOŻENIA. Wiersz notatki (`note-row.tsx`) i okno wymuszonego wyboru
 * (`forced-choice.tsx`) są wylądowane (T-17) i mają własne kryteria — drugiego wiersza ani
 * drugiego okna nie piszemy (niezmiennik 23). Między komponentem a sekcją brakowało nagłówka
 * i podziału na strefy, i tylko to jest w tym pliku.
 *
 * CZEGO TU ŚWIADOMIE NIE MA. Przełącznika zakresu („This project" / „Everywhere"
 * z `docs/mockup/index.html:745`): `MemoryState` nie ma czym go obsłużyć, a kontrolka bez
 * handlera nie wchodzi do repo (niezmiennik 16) — to jest dokładnie ta wada, którą T-26
 * cytuje jako powód swojego istnienia. Wraca w tym samym commicie, w którym pojawia się
 * filtrowanie po zakresie.
 *
 * ZGŁOSZENIE DLA CZŁOWIEKA (zmierzone 2026-08-16). `tasks/T-26.md` chce, żeby notatka
 * zaproponowana niosła „swoje DWIE akcje" (makieta: `Use it` i `Discard`,
 * `docs/mockup/index.html:757`). `NoteRow` renderuje dokładnie JEDNĄ — `Use this` przy
 * `suggested`, `Stop using` przy `in-use` — i tak zamraża to kryterium 6 z T-17. Drugiej nie
 * ma czym obsłużyć: `MemoryState` zna `use`, `stopUsing` i `cancel`, i ani jednego odrzucenia
 * kandydatki. Domknięcie wymaga `discard` w `src/state/memory.ts` i drugiego przycisku
 * w `note-row.tsx` — oba pliki są poza blokiem OWNS tego zadania (AGENTS.md §7).
 *
 * O migawce serwerowej zustanda i o tym, dlaczego magazyn czyta się tu przez
 * `useSyncExternalStore`, przeczytaj w `src/sections/workflows/index.tsx`.
 */
import type { ReactElement } from 'react';
import { useSyncExternalStore } from 'react';
import { useMemory } from '../../state/memory';
import { ForcedChoice } from './forced-choice';
import { NoteRow } from './note-row';

/** Magazyn notatek. Jest singletonem — `src/state/memory.ts` nie ma fabryki. */
export type MemoryStore = typeof useMemory;

export interface MemoryScreenProps {
  /** Bez propsu ekran bierze swój prawdziwy magazyn, z propsem ten z testu. */
  store?: MemoryStore;
}

const ZONE_TITLE = 'text-label text-muted';
const ZONE_LEAD = 'max-w-160 text-body text-muted';

/**
 * `1 note`, ale `4 notes`.
 *
 * Odmiana idzie za liczbą, a nie obok niej: `${n} notes` czyta się poprawnie przy czterech
 * i źle przy jednej, a napis wpisany na stałe czyta się poprawnie dokładnie na tym jednym
 * ekranie, na którym ktoś go sprawdzał.
 */
function counted(count: number, noun: string): string {
  return count === 1 ? '1 ' + noun : String(count) + ' ' + noun + 's';
}

export default function MemoryScreen({ store = useMemory }: MemoryScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);

  /* Podział liczony ze stanu przy każdym renderze, a nie trzymany w dwóch tablicach: dwie
   * listy w magazynie rozjeżdżają się przy pierwszej promocji, która trafi tylko do jednej
   * z nich, i widać to dopiero wtedy, gdy notatka jest w obu strefach naraz. */
  const waiting = state.notes.filter((note) => note.status === 'suggested');
  const inUse = state.notes.filter((note) => note.status === 'in-use');

  /* Obie akcje wołają magazyn, i tylko magazyn: to on rozmawia z dyskiem i to on decyduje,
   * czy notatka naprawdę zmieniła stan (`src/state/memory.ts` — komenda, odpowiedź, dopiero
   * potem stan). Wiersz ich nie zna, bo wiersz nie zna magazynu. */
  const use = (id: string): void => {
    void store.getState().use(id);
  };
  const stopUsing = (id: string): void => {
    void store.getState().stopUsing(id);
  };

  return (
    <section className="flex h-full flex-col">
      <header className="flex h-13 items-center gap-3 border-b border-line bg-panel px-4">
        <h1 className="text-title text-ink">Memory</h1>

        {/* Licznik żyje tylko wtedy, gdy jest co liczyć: przy zerze mówi to samo zdanie
            pustego ekranu (niezmiennik 13). */}
        {state.notes.length === 0 ? null : (
          <span className="font-mono text-mono text-muted">
            {counted(state.notes.length, 'note')}
          </span>
        )}
      </header>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {/* Zdanie od magazynu: odmowa promocji albo zapisu. Bez tego jedyną odpowiedzią na
            kliknięcie jest cisza, a człowiek klika drugi raz i zgłasza błąd. */}
        {state.message === null ? null : (
          <p className="mb-4 max-w-160 text-body text-attend">{state.message}</p>
        )}

        {state.notes.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3">
            <span className="flex size-8 items-center justify-center rounded-sq border border-dashed border-line-strong text-muted">
              ◇
            </span>
            {/* `data-empty` na elemencie z samym zdaniem — tak samo jak w `src/App.tsx`. */}
            <p data-empty className="text-ink">
              No notes yet.
            </p>
            <p className="text-muted">Agents leave what they learn here, for the next agent.</p>
          </div>
        ) : (
          <div className="flex max-w-160 flex-col gap-6">
            {/* Strefa, która czegoś od człowieka chce, stoi PIERWSZA. Pusta strefa nie jest
                rysowana: nagłówek nad niczym jest miejscem na ekranie oddanym za fakt, który
                już mówi jego brak. */}
            {waiting.length === 0 ? null : (
              <section data-zone="suggested" className="flex flex-col gap-2">
                <h2 className={ZONE_TITLE}>Waiting for you</h2>
                <p className={ZONE_LEAD}>
                  An agent suggested these. They stay out of every prompt until you say yes.
                </p>
                <ul className="flex flex-col">
                  {waiting.map((note) => (
                    <NoteRow key={note.id} note={note} onUse={use} onStopUse={stopUsing} />
                  ))}
                </ul>
              </section>
            )}

            {inUse.length === 0 ? null : (
              <section data-zone="in-use" className="flex flex-col gap-2">
                <h2 className={ZONE_TITLE}>In use</h2>
                <p className={ZONE_LEAD}>
                  These go into the prompt of every agent working on this project.
                </p>
                <ul className="flex flex-col">
                  {inUse.map((note) => (
                    <NoteRow key={note.id} note={note} onUse={use} onStopUse={stopUsing} />
                  ))}
                </ul>
              </section>
            )}
          </div>
        )}
      </div>

      {/* Wymuszony wybór: „zakres jest pełny" przyjeżdża z Rusta jako odmowa promocji
          [T6 §5.3] i magazyn stawia wtedy `choice`. Ekran, który tego okna nie montuje,
          zostawia człowieka z kliknięciem, po którym nie dzieje się nic. */}
      {state.choice === null ? null : (
        <ForcedChoice
          choice={state.choice}
          notes={state.notes}
          onStopUsing={stopUsing}
          onCancel={() => {
            store.getState().cancel();
          }}
        />
      )}
    </section>
  );
}
