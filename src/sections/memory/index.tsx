/* Ekran sekcji Memory: nagłówek i TRZY STREFY — to, co czeka na człowieka, to, co wchodzi
 * do promptu, i to, co agenci przekazali sobie po drodze.
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
 * TRZECIA STREFA JEST NAGŁÓWNĄ OBIETNICĄ TEJ SEKCJI (dobudowana 2026-08-18). Zdanie pustego
 * ekranu, które rejestr trzyma dla Pamięci, brzmi „What agents leave for each other lands here"
 * (`src/ui/sections.tsx`) — a do tego dnia sekcja renderowała DWIE strefy z trzech i nie miała
 * ani jednej drogi, którą mogłaby zapytać o te pliki. Obietnica z rejestru nie miała pokrycia
 * na żadnym ekranie: pliki powstawały (`memory::handoff`), okno o nie nie pytało. Trzecia
 * strefa stoi teraz na `list_handoffs` (`src/sections/memory/io.ts`).
 *
 * DLACZEGO TRZECIA STREFA RYSUJE SIĘ TAKŻE PUSTA, A DWIE PIERWSZE NIE. „Waiting for you" i „In
 * use" to dwie połowy JEDNEJ listy — notatka jest dokładnie w jednej z nich, więc nagłówek nad
 * pustą połową jest miejscem oddanym za fakt, który już mówi jego brak. Przekazania to osobna
 * rzecz i osobne pliki: człowiek, który ich nie widzi, nie wie, czy ich nie ma, czy sekcja
 * o nich nie mówi. Pusta strefa z zaproszeniem odpowiada na to pytanie raz.
 *
 * CZEGO TU ŚWIADOMIE NIE MA. Przełącznika zakresu („This project" / „Everywhere"
 * z `docs/mockup/index.html:745`): `MemoryState` nie ma czym go obsłużyć, a kontrolka bez
 * handlera nie wchodzi do repo (niezmiennik 16) — to jest dokładnie ta wada, którą T-26
 * cytuje jako powód swojego istnienia. `list_handoffs` też nie przyjmuje w tej fali zakresu,
 * więc przełącznik nie miałby czego przestawić ani w trzeciej strefie. Wraca w tym samym
 * commicie, w którym pojawia się filtrowanie po zakresie.
 *
 * ZGŁOSZENIE Z 2026-08-16 ZAMKNIĘTE 2026-08-23 (T-92). `tasks/T-26.md` chciał, żeby notatka
 * zaproponowana niosła „swoje DWIE akcje" (makieta: `Use it` i `Discard`,
 * `docs/mockup/index.html:757`), a `NoteRow` renderował dokładnie JEDNĄ, bo drugiej nie było
 * czym obsłużyć: `MemoryState` znał `use`, `stopUsing` i `cancel`, i ani jednego odrzucenia
 * kandydatki. Oba brakujące pliki leżały wtedy poza blokiem OWNS tamtego zadania (AGENTS.md §7)
 * i leżą w bloku tego. `Discard` dostaje WYŁĄCZNIE strefa „Waiting for you": notatka, która już
 * jedzie do promptu, wychodzi z niego osobną decyzją, a dopiero potem można ją odrzucić.
 *
 * O migawce serwerowej zustanda i o tym, dlaczego magazyn czyta się tu przez
 * `useSyncExternalStore`, przeczytaj w `src/sections/workflows/index.tsx`.
 */
import type { ReactElement } from 'react';
import { useEffect, useSyncExternalStore } from 'react';
import type { NoteAddress } from '../../state/memory';
import { useMemory } from '../../state/memory';
import { activeWorkspace, useWorkspaces } from '../../state/workspaces';
import { ForcedChoice } from './forced-choice';
import { NoteRow } from './note-row';
import { PassedRow } from './passed-row';

/** Magazyn notatek. Jest singletonem — `src/state/memory.ts` nie ma fabryki. */
export type MemoryStore = typeof useMemory;

export interface MemoryScreenProps {
  /** Bez propsu ekran bierze swój prawdziwy magazyn, z propsem ten z testu. */
  store?: MemoryStore;
}

/* Nadoczko strefy, wiec `text-eyebrow` — stopien, ktory nosi wersaliki (DESIGN §4). Do
 * 2026-08-19 bylo tu `text-label`; po rozszczepieniu stopnia trzy naglowki stref przestaly
 * krzyczec i nic tego nie zglaszalo, bo klasa trzymana w stalej jest niewidoczna dla skanera,
 * ktory czyta wylacznie literaly `className="..."`. AC-6 rozwija teraz stale. */
const ZONE_TITLE = 'text-eyebrow text-muted';
const ZONE_LEAD = 'max-w-160 text-body text-muted';
/* `.ctx` z makiety: obrys `--line`, tło `--panel`, pozycje w środku z odstępem. */
const PASSED_BOX = 'flex flex-col gap-2 rounded-md border border-line bg-panel p-3';

/**
 * Zdanie trzeciej strefy, kiedy nie ma w niej ani jednego pliku.
 *
 * Mówi PRAWDĘ o tym, dlaczego jest pusta, i zaprasza (DESIGN §6: pusty stan jest zaproszeniem,
 * nie komunikatem o braku danych). Powód jest konkretny i sprawdzalny: te pliki powstają
 * dopiero wtedy, gdy krok biegu skończy pracę i odda wynik następnemu
 * (`memory::handoff::write_handoff`), a na tej maszynie nie skończył jeszcze żaden.
 */
const NOTHING_PASSED_YET =
  'Nothing yet. Agents leave these for each other as they finish steps, so the first workflow ' +
  'you run will fill this in.';

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

function activeCatalogFolder(): string | null {
  return activeWorkspace()?.folder ?? null;
}

export default function MemoryScreen({ store = useMemory }: MemoryScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  const catalogFolder = useSyncExternalStore(
    useWorkspaces.subscribe,
    activeCatalogFolder,
    activeCatalogFolder,
  );

  /* ODCZYT PRZY WEJŚCIU W SEKCJĘ — bez tego cała ścieżka odczytu jest martwa.
   *
   * Magazyn dostał `load()` w T-38 AC-6 i do 2026-08-18 NIE MIAŁ ANI JEDNEGO WOŁAJĄCEGO:
   * komenda po stronie Rusta istniała, krawędź `io.ts` istniała, magazyn umiał się wypełnić —
   * i ekran nigdy o nic nie pytał. To jest ta sama rodzina, co płótno przed T-26 i `wireChannel`
   * przed T-38: mechanizm wylądował, ma testy, nikt go nie zawołał. Objaw dla człowieka jest
   * dokładnie taki, jak przy braku funkcji: otwierasz sekcję i nie ma w niej tego, co leży
   * na dysku (niezmiennik 4 — pliki są prawdą).
   *
   * `void`, bo odmowa jest już obsłużona w magazynie i ląduje w jego stanie jako zdanie dla
   * człowieka; drugie `catch` tutaj byłoby drugim miejscem, w którym mieszka ta sama decyzja.
   * Pusta tablica zależności: sekcja pyta RAZ na zamontowanie, a nie na każdy render. */
  useEffect(() => {
    void store.getState().load(catalogFolder);
  }, [catalogFolder, store]);

  /* Podział liczony ze stanu przy każdym renderze, a nie trzymany w dwóch tablicach: dwie
   * listy w magazynie rozjeżdżają się przy pierwszej promocji, która trafi tylko do jednej
   * z nich, i widać to dopiero wtedy, gdy notatka jest w obu strefach naraz. */
  const legacy = state.notes.filter(
    (note) => note.place === 'library' && note.scope === 'this-project',
  );
  const current = state.notes.filter(
    (note) => !(note.place === 'library' && note.scope === 'this-project'),
  );
  const waiting = current.filter((note) => note.status === 'suggested');
  const inUse = current.filter((note) => note.status === 'in-use');

  /* Obie akcje wołają magazyn, i tylko magazyn: to on rozmawia z dyskiem i to on decyduje,
   * czy notatka naprawdę zmieniła stan (`src/state/memory.ts` — komenda, odpowiedź, dopiero
   * potem stan). Wiersz ich nie zna, bo wiersz nie zna magazynu. */
  const use = (address: NoteAddress): void => {
    void store.getState().use(address);
  };
  const stopUsing = (address: NoteAddress): void => {
    void store.getState().stopUsing(address);
  };
  const discard = (address: NoteAddress): void => {
    void store.getState().discard(address);
  };
  const moveToProject = (address: NoteAddress): void => {
    void store.getState().moveToProject(address);
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

        {/* Zaproszenie zamiast stref TYLKO wtedy, gdy naprawdę nie ma nic i nic nie odmówiło.
            Odmowa odczytu przekazań mieszka w swojej strefie, więc ekran, który przy zerze
            pokazuje samo zaproszenie, zjadłby ją w całości — a „nic tu jeszcze nie ma"
            i „nie umiem tego przeczytać" to dwie różne rzeczy do zrobienia. */}
        {state.notes.length === 0 && state.passed.length === 0 && state.passedProblem === null ? (
          <div className="flex h-full flex-col items-center justify-center gap-3">
            <span className="flex size-8 items-center justify-center rounded-md border border-dashed border-line-strong text-muted">
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
            {legacy.length === 0 ? null : (
              <section data-zone="earlier-project" className="flex flex-col gap-2">
                <h2 className={ZONE_TITLE}>Earlier project notes</h2>
                <p className={ZONE_LEAD}>
                  Move these earlier notes into this project before putting them to use.
                </p>
                <ul className="flex flex-col">
                  {legacy.map((note) => (
                    <NoteRow
                      key={`${note.place}:${note.id}`}
                      note={note}
                      onUse={use}
                      onStopUse={stopUsing}
                      onMove={moveToProject}
                    />
                  ))}
                </ul>
              </section>
            )}

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
                  {/* Handler „Discard" dostaje WYŁĄCZNIE ta strefa. Wiersz sam też pyta o stan
                      notatki, i to nie jest podwójna robota: wiersz broni się przed każdym
                      wołającym, a ekran mówi, w którym miejscu ta decyzja w ogóle istnieje. */}
                  {waiting.map((note) => (
                    <NoteRow
                      key={`${note.place}:${note.id}`}
                      note={note}
                      onUse={use}
                      onStopUse={stopUsing}
                      onDiscard={discard}
                    />
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
                    <NoteRow
                      key={`${note.place}:${note.id}`}
                      note={note}
                      onUse={use}
                      onStopUse={stopUsing}
                    />
                  ))}
                </ul>
              </section>
            )}

            {/* Trzecia strefa. Rysuje się też pusta — powód stoi w nagłówku pliku. */}
            <section data-zone="passed" className="flex flex-col gap-2">
              <h2 className={ZONE_TITLE}>What agents passed to each other</h2>
              <p className={ZONE_LEAD}>These are plain files on disk — open them anywhere.</p>

              {/* Odmowa odczytu TEJ strefy stoi w TEJ strefie. Wyżej, obok zdania o notatkach,
                  człowiek nie miałby jak zgadnąć, o który z dwóch katalogów chodzi. */}
              {state.passedProblem === null ? null : (
                <p className="max-w-160 text-body text-attend">{state.passedProblem}</p>
              )}

              {state.passed.length === 0 ? (
                <p className={ZONE_LEAD}>{NOTHING_PASSED_YET}</p>
              ) : (
                <ul className={PASSED_BOX}>
                  {state.passed.map((one) => (
                    <PassedRow key={one.id} passed={one} />
                  ))}
                </ul>
              )}
            </section>
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
