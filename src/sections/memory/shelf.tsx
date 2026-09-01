/* Półka notatek: to, co czeka na człowieka, to, co wchodzi do KAŻDEGO promptu, i to, co agenci
 * przekazali sobie po drodze.
 *
 * SKĄD SIĘ TU WZIĄŁ TEN PLIK. Do 2026-08-31 to był `src/sections/memory/index.tsx`, czyli cały
 * ekran sekcji Memory, z własnym nagłówkiem i własnym pustym zaproszeniem. Sekcja zniknęła —
 * scaliła się z Umiejętnościami w jedną sekcję Knowledge (decyzja właściciela, `src/ui/
 * sections.tsx`) — a to, co robił ekran, zostało półką: bez paska nagłówka i bez zdania
 * o pustce, bo jedno i drugie należy teraz do ekranu, który obie półki trzyma.
 *
 * ROZDZIAŁ STREF JEST TU CAŁYM PRODUKTEM. Notatka zaproponowana przez agenta nie wchodzi do
 * promptu, dopóki człowiek jej nie promuje [T6 §5.1, T-17] — a ekran wyświetlający obie
 * w jednym worku kasuje jedyną widoczną różnicę między tym, co zaproponował agent, a tym, co
 * zatwierdził człowiek. Jedna płaska lista przechodzi „obie notatki są w dokumencie"
 * i unieważnia sekcję.
 *
 * KOLEJNOŚĆ STREF ZMIENIŁA SIĘ RAZEM ZE SCALENIEM. Najpierw stoi wszystko, co czegoś od
 * człowieka chce — kandydatki i notatki, które trzeba przenieść — a dopiero potem to, co już
 * działa. Wcześniej strefa „Earlier project notes" stała nad kandydatkami i pytanie, na które
 * człowiek miał odpowiedzieć najpierw, było drugie.
 *
 * „ALWAYS ON", A NIE „IN USE". Ta strefa i półka umiejętności obok niej są dwiema połowami
 * jednej różnicy, a różnica jest najważniejszą rzeczą, jaką człowiek musi tu zrozumieć: notatka
 * w użyciu wchodzi do KAŻDEGO promptu, a po umiejętność model sięga sam, kiedy pasuje. „In use"
 * mówiło o notatce, że jest włączona, i ani słowa o tym, co to dla człowieka znaczy.
 *
 * CIENKI Z ZAŁOŻENIA. Wiersz notatki (`note-row.tsx`) i okno wymuszonego wyboru
 * (`forced-choice.tsx`) są wylądowane (T-17) i mają własne kryteria — drugiego wiersza ani
 * drugiego okna nie piszemy (niezmiennik 23).
 *
 * TRZECIA STREFA JEST NAGŁÓWNĄ OBIETNICĄ TEJ POŁOWY (dobudowana 2026-08-18). Do tego dnia
 * pliki przekazań powstawały (`memory::handoff`), a okno o nie nie pytało. Trzecia strefa stoi
 * na `list_handoffs` (`src/sections/memory/io.ts`) i rysuje się TAKŻE PUSTA: przekazania to
 * osobne pliki, więc człowiek, który ich nie widzi, nie wie, czy ich nie ma, czy półka o nich
 * nie mówi. „Waiting for you" i „Always on" to za to dwie połowy JEDNEJ listy — notatka jest
 * dokładnie w jednej z nich, więc nagłówek nad pustą połową jest miejscem oddanym za fakt,
 * który już mówi jego brak.
 *
 * ZGŁOSZENIE Z 2026-08-16 ZAMKNIĘTE 2026-08-23 (T-92). `Discard` dostaje WYŁĄCZNIE strefa
 * „Waiting for you": notatka, która już jedzie do promptu, wychodzi z niego osobną decyzją,
 * a dopiero potem można ją odrzucić.
 *
 * O migawce serwerowej zustanda i o tym, dlaczego magazyn czyta się tu przez
 * `useSyncExternalStore`, przeczytaj w `src/sections/workflows/index.tsx`.
 */
import type { ReactElement, ReactNode } from 'react';
import { useSyncExternalStore } from 'react';
import type { Note, NoteAddress } from '../../state/memory';
import { useMemory } from '../../state/memory';
import { ForcedChoice } from './forced-choice';
import { NoteRow } from './note-row';
import { PassedRow } from './passed-row';

/** Magazyn notatek. Jest singletonem — `src/state/memory.ts` nie ma fabryki. */
export type MemoryStore = typeof useMemory;

export interface NotesShelfProps {
  /** Bez propsu półka bierze swój prawdziwy magazyn, z propsem ten z testu. */
  store?: MemoryStore;
  /**
   * Półka, która stoi MIĘDZY notatkami w użyciu a plikami przekazań — czyli umiejętności.
   *
   * SZCZELINA, A NIE DWA KOMPONENTY, i powód jest jeden: „Always on" i „Used when it fits" mają
   * się dotykać. Różnica między nimi czyta się wyłącznie z sąsiedztwa — półka odsunięta o dwie
   * strefy jest znów osobną listą i nic na ekranie nie mówi, czym się od tamtej różni.
   * Przekazania są za to zwykłymi plikami na dysku, nie półką, i należą na sam dół.
   *
   * Bez tego propsu (tak montują tę półkę jej własne kryteria) nie zmienia się nic.
   */
  nextShelf?: ReactNode;
}

/* Nadoczko strefy, wiec `text-eyebrow` — stopien, ktory nosi wersaliki (DESIGN §4). Do
 * 2026-08-19 bylo tu `text-label`; po rozszczepieniu stopnia trzy naglowki stref przestaly
 * krzyczec i nic tego nie zglaszalo, bo klasa trzymana w stalej jest niewidoczna dla skanera,
 * ktory czyta wylacznie literaly `className="..."`. AC-6 rozwija teraz stale. */
const ZONE_TITLE = 'text-eyebrow text-muted';
/* 2026-08-31: rola „zdanie drugoplanowe" ma teraz nazwę (`.lead`), a stopień rozstrzygnięty
 * jeden raz w DESIGN §6 na `--t-note`. Zostaje tu wyłącznie miara wiersza — szerokość nie jest
 * rolą, tylko rozmieszczeniem, i prymityw jej nie wchłania. */
const ZONE_LEAD = 'lead max-w-160';
/* `.ctx` z makiety. Pojemnik treści = `.card` (obrys `--line`, tło `--panel`, promień `md`,
 * padding 12 px); zostaje sam klej układu, bo karta nie ustawia kierunku ani odstępu dzieci. */
const PASSED_BOX = 'card flex flex-col gap-2';

/**
 * Notatki, które CZEKAJĄ na decyzję człowieka — jedna definicja na całą sekcję.
 *
 * EKSPORTOWANA, bo od 2026-08-31 czyta ją także ekran (`knowledge/index.tsx`): to on rozstrzyga,
 * czy czynnością główną jest odpowiedzieć na kolejkę, czy dodać umiejętność. Policzona tam
 * drugi raz byłaby drugą odpowiedzią na jedno pytanie (niezmiennik 13) i pierwszym miejscem,
 * w którym ekran mógłby uznać kolejkę za pustą, kiedy półka rysuje ją pełną.
 *
 * Notatka biblioteczna o zakresie projektu jest tu WYŁĄCZONA celowo: ona czeka na przeniesienie,
 * a nie na „tak", i ma własną strefę z własnym czasownikiem.
 */
export function waitingFrom(notes: readonly Note[]): Note[] {
  return notes.filter(
    (note) =>
      note.status === 'suggested' && !(note.place === 'library' && note.scope === 'this-project'),
  );
}

/** Nagłówek półki notatek — to samo słowo czyta kryterium i człowiek. */
export const ALWAYS_ON = 'Always on';

/**
 * Zdanie, które robi z „Always on" połowę różnicy, a nie kolejnej listy.
 *
 * Mówi rzecz, której nie widać i której nie da się zgadnąć z samej listy: te zdania jadą do
 * modelu ZA KAŻDYM RAZEM, także wtedy, gdy nie mają nic wspólnego z tym, co człowiek właśnie
 * robi. To jest cała cena notatki i cały powód, dla którego jest ich twardy limit.
 */
const ALWAYS_ON_LEAD = 'These go into every prompt, every time. Their reach is shown by row.';

/** Kiedy półka stoi, ale nie ma w niej ani jednej notatki w użyciu. */
const NONE_IN_USE = 'Nothing is going into your prompts yet.';

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
 * Dwie półki obok siebie, przy jednej kresce — albo sama półka notatek, gdy drugiej nie ma.
 *
 * FUNKCJA, A NIE DRUGI KOMPONENT: to jest rozmieszczenie dwojga dzieci, nie rzecz z własnym
 * stanem ani z własnym kryterium. Bez drugiej półki siatka o dwóch kolumnach zostawiłaby
 * notatki w połowie szerokości i pustą kolumnę obok — czyli pojemnik, którego nic nie niesie,
 * a taki nie ma prawa zajmować miejsca (DESIGN §6). Tak montują tę półkę jej własne kryteria.
 *
 * Kreska przerzuca się z góry na bok razem z kolumnami: w obu układach półki się dotykają, bo
 * dotknięcie jest tu treścią, a nie ozdobą.
 */
function shelfPair(notes: ReactElement, skills: ReactNode): ReactElement {
  if (skills === undefined) return notes;
  return (
    <div data-shelves className="grid grid-cols-1 gap-6 md:grid-cols-2 md:gap-0">
      {notes}
      <div className="border-t border-line pt-6 md:border-t-0 md:border-l md:pt-0 md:pl-6">
        {skills}
      </div>
    </div>
  );
}

export default function NotesShelf({
  store = useMemory,
  nextShelf,
}: NotesShelfProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);

  /* Podział liczony ze stanu przy każdym renderze, a nie trzymany w dwóch tablicach: dwie
   * listy w magazynie rozjeżdżają się przy pierwszej promocji, która trafi tylko do jednej
   * z nich, i widać to dopiero wtedy, gdy notatka jest w obu strefach naraz. */
  const legacy = state.notes.filter(
    (note) => note.place === 'library' && note.scope === 'this-project',
  );
  const current = state.notes.filter(
    (note) => !(note.place === 'library' && note.scope === 'this-project'),
  );
  const waiting = waitingFrom(state.notes);
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
  /* Pierwsze kliknięcie w „Discard" PYTA, drugie wykonuje. Obie połowy wołają magazyn i tylko
   * magazyn: to on trzyma, o co pytamy, i pilnuje, żeby pytanie stało przy jednym wierszu
   * naraz (niezmiennik 13). */
  const askDiscard = (address: NoteAddress): void => {
    store.getState().askDiscard(address);
  };
  const discard = (address: NoteAddress): void => {
    void store.getState().discard(address);
  };
  const keepIt = (): void => {
    store.getState().keepIt();
  };
  const moveToProject = (address: NoteAddress): void => {
    void store.getState().moveToProject(address);
  };

  /** Czy pytanie o odrzucenie stoi przy TEJ notatce. Jedno pytanie, jedno miejsce. */
  const askingAbout = (note: Note): boolean =>
    state.pendingDiscard !== null &&
    state.pendingDiscard.place === note.place &&
    state.pendingDiscard.id === note.id;

  return (
    <div className="flex flex-col gap-6">
      {/* Zdanie od magazynu: odmowa promocji albo zapisu. Bez tego jedyną odpowiedzią na
          kliknięcie jest cisza, a człowiek klika drugi raz i zgłasza błąd. */}
      {/* WEJŚCIE, bo to zdanie PRZYCHODZI — jest jedyną odpowiedzią na kliknięcie, po którym
          nic innego na ekranie się nie rusza. Zdanie, które pojawia się skokiem tam, gdzie
          przed chwilą nic nie było, czyta się jak przeskok widoku (DESIGN §7). Magazyn stawia
          `message` i `choice` rozłącznie (`src/state/memory.ts`), więc jedno zdarzenie
          porusza tu dokładnie jednym regionem — sufit z ARCHITECTURE §7 wynosi dwa. */}
      {state.message === null ? null : (
        <p className="lead enter max-w-160" data-tone="attend">
          {state.message}
        </p>
      )}

      {/* Strefa, która czegoś od człowieka chce, stoi PIERWSZA. Pusta strefa nie jest
          rysowana: nagłówek nad niczym jest miejscem na ekranie oddanym za fakt, który
          już mówi jego brak. */}
      {waiting.length === 0 ? null : (
        /* BOHATER TEGO EKRANU (2026-08-31, fala kompozycji). Kolejka decyzji jest jedyną rzeczą
           w tej sekcji, która czegoś od człowieka CHCE — reszta jest stanem rzeczy. Do tego dnia
           wyglądała dokładnie tak samo jak dwie półki pod nią: to samo nadoczko, ten sam wiersz,
           ten sam cichy przycisk. Zmruż oczy nad zrzutem z 2026-08-31: największą rzeczą na
           ekranie była lista notatek już w użyciu, czyli to, co jest zrobione.

           `.pane`, bo to jedyna rzecz na tym ekranie, która PŁYWA (DESIGN §3): półki są treścią
           leżącą na tle, a to jest karta położona NA nich. `.enter` sprężyną, bo kolejka
           PRZYBYWA — pustej nie ma w dokumencie wcale.

           Nagłówek jest o dwa stopnie wyżej niż nadoczka półek (`text-heading` wobec
           `text-eyebrow`), bo trzy poziomy głośności zaczynają się od rozmiaru. */
        <section data-zone="suggested" data-gap="3" className="stack pane enter p-4">
          <div className="flex flex-wrap items-baseline gap-2">
            <h2 className="text-heading text-ink">Waiting for you</h2>
            {/* Licznik mówi, ILE decyzji zostało — czego lista nie mówi, dopóki jej nie
                przewiniesz. Ta sama forma, co licznik zapisanych umiejętności w półce obok. */}
            <span className="value">{`${String(waiting.length)} to answer`}</span>
          </div>
          <p className={ZONE_LEAD}>
            An agent suggested these. They stay out of every prompt until you say yes.
          </p>
          <ul className="flex flex-col">
            {/* Handler „Discard" dostaje WYŁĄCZNIE ta strefa. Wiersz sam też pyta o stan
                notatki, i to nie jest podwójna robota: wiersz broni się przed każdym
                wołającym, a ekran mówi, w którym miejscu ta decyzja w ogóle istnieje. */}
            {waiting.map((note, index) => (
              <NoteRow
                key={`${note.place}:${note.id}`}
                note={note}
                /* CZOŁO KOLEJKI — jedyny wiersz na ekranie z czynnością główną. Powód stoi
                   przy propsie `queue` w `note-row.tsx`. */
                queue={index === 0 ? 'head' : 'behind'}
                onUse={use}
                onStopUse={stopUsing}
                onDiscard={askDiscard}
                askingToDiscard={askingAbout(note)}
                onDiscardForGood={discard}
                onKeepIt={keepIt}
              />
            ))}
          </ul>
        </section>
      )}

      {legacy.length === 0 ? null : (
        <section data-zone="earlier-project" data-gap="2" className="stack">
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

      {/* PIERWSZA POŁOWA RÓŻNICY, i dlatego ta strefa rysuje się TAKŻE PUSTA — inaczej niż
          „Waiting for you" nad nią. Nagłówek i zdanie pod nim są tu jedyną rzeczą, która mówi
          człowiekowi, czym notatka różni się od umiejętności; strefa znikająca przy zerze
          zabiera to zdanie dokładnie wtedy, kiedy człowiek pierwszy raz tu wchodzi.

          OBIE POŁOWY STOJĄ OBOK SIEBIE, NIE JEDNA POD DRUGĄ (2026-08-31, fala kompozycji).
          Sąsiedztwo było i jest całym zyskiem scalenia — ale półka pod półką znaczy, że
          człowiek czyta jedną, przewija, i dopiero potem widzi drugą, a różnicy nie da się
          przeczytać z rzeczy, których nie widać naraz. Kolumny leżą przy jednej kresce, więc
          półki DOTYKAJĄ SIĘ dosłownie. Przy okazji znika wada zmierzona na zrzucie z tego dnia:
          treść siedziała w kolumnie 640 px, a prawa połowa okna była czarna na całą wysokość.

          KOLEJNOŚĆ W DOKUMENCIE ZOSTAJE. Siatka nie przestawia dzieci, więc „Always on" wciąż
          poprzedza „Used when it fits" — a to jest zamrożone kryterium
          (`knowledge/one-section-two-shelves.test.tsx`). */}
      {shelfPair(
        <section data-zone="in-use" data-gap="2" className="stack md:pr-6">
          <h2 className={ZONE_TITLE}>{ALWAYS_ON}</h2>
          <p className={ZONE_LEAD}>{ALWAYS_ON_LEAD}</p>
          {inUse.length === 0 ? (
            <p className={ZONE_LEAD}>{NONE_IN_USE}</p>
          ) : (
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
          )}
        </section>,
        nextShelf,
      )}

      {/* Przekazania. Rysują się też puste — powód stoi w nagłówku pliku. */}
      <section data-zone="passed" data-gap="2" className="stack">
        <h2 className={ZONE_TITLE}>What agents passed to each other</h2>
        <p className={ZONE_LEAD}>These are plain files on disk — open them anywhere.</p>

        {/* Odmowa odczytu TEJ strefy stoi w TEJ strefie. Wyżej, obok zdania o notatkach,
            człowiek nie miałby jak zgadnąć, o który z dwóch katalogów chodzi. */}
        {state.passedProblem === null ? null : (
          <p className="lead max-w-160" data-tone="attend">
            {state.passedProblem}
          </p>
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

      {/* Wymuszony wybór: „zakres jest pełny" przyjeżdża z Rusta jako odmowa promocji
          [T6 §5.3] i magazyn stawia wtedy `choice`. Półka, która tego okna nie montuje,
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
    </div>
  );
}
