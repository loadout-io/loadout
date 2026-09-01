/* Ekran sekcji Knowledge: JEDNO miejsce na wszystko, co model wie o pracy tego człowieka.
 *
 * DECYZJA WŁAŚCICIELA, 2026-08-31. Do tego dnia to były dwie sekcje — Skills i Memory — czyli
 * dwie pozycje menu odpowiadające na jedno pytanie. Scalony jest EKRAN; magazyny zostają osobne
 * (`src/state/skills.ts`, `src/state/memory.ts`) i to jest rozstrzygnięcie, nie zaniechanie:
 * umiejętność bywa cudza i wykonywalna, więc idzie przez przegląd bezpieczeństwa z blokującymi
 * znaleziskami, a notatka jest własna, deklaratywna i konkuruje o twardy limit długości.
 * Karty przeglądu NIE WOLNO rozlać na notatki — skanowanie własnych zdań o własnym repo
 * zamienia przegląd w rytuał, a rytuał przeklikuje się bez czytania.
 *
 * DLACZEGO TO JEST ZYSK, a nie przestawianie mebli. Różnica między jedną a drugą rzeczą jest
 * najważniejszą rzeczą, jaką człowiek musi tu zrozumieć — notatka w użyciu wchodzi do KAŻDEGO
 * promptu, po umiejętność model sięga sam, kiedy pasuje — i przy dwóch osobnych sekcjach nie
 * była powiedziana nigdzie. Dwie półki jedna pod drugą mówią ją samym sąsiedztwem.
 *
 * KOLEJNOŚĆ JEST TREŚCIĄ. Kolejka decyzji (notatki, które agent zaproponował) stoi na górze,
 * bo tylko ona czegoś od człowieka chce. Potem dwie półki, które muszą się dotykać. Na końcu
 * pliki, które agenci przekazali sobie w biegach — to nie jest półka, tylko lista plików.
 * Układ mieszka w `memory/shelf.tsx`, który przyjmuje półkę umiejętności propsem `nextShelf`:
 * powód tej szczeliny stoi przy samym propsie.
 *
 * DWA ODCZYTY MIESZKAJĄ TUTAJ, NIE W PÓŁKACH, i to nie jest kwestia gustu. Przy pustym
 * wszystkim ten ekran pokazuje jedno zaproszenie ZAMIAST półek, więc półki nie są wtedy
 * zamontowane. Efekt odczytu wiszący w półce nie odpaliłby się ani razu i pustka byłaby
 * jednocześnie swoją własną przyczyną — katalog pełen umiejętności zostawałby niewidoczny do
 * końca życia okna (niezmiennik 4: pliki są prawdą).
 *
 * JEDNO ZAPROSZENIE, JEDEN ZNACZNIK. `data-empty` stoi na tym ekranie dokładnie raz i tylko
 * wtedy, gdy naprawdę nie ma nic i nic nie odmówiło. Dwa znaczniki — po jednym na półkę —
 * byłyby dwiema odpowiedziami na jedno pytanie (niezmiennik 13) i każdy czytelnik tego
 * znacznika dostałby dla tej sekcji co innego niż dla pozostałych
 * (`src/sections/empty-screen-invites.test.tsx`).
 *
 * O migawce serwerowej zustanda i o tym, dlaczego magazyny czyta się tu przez
 * `useSyncExternalStore`, przeczytaj w `src/sections/workflows/index.tsx`.
 */
import type { ReactElement } from 'react';
import { useEffect, useSyncExternalStore } from 'react';
import { useMemory } from '../../state/memory';
import { useSkills } from '../../state/skills';
import { activeWorkspace, useWorkspaces } from '../../state/workspaces';
import type { MemoryStore } from '../memory/shelf';
import NotesShelf from '../memory/shelf';
import type { SkillsStore } from '../skills/shelf';
import SkillsShelf from '../skills/shelf';

export interface KnowledgeScreenProps {
  /** Magazyn notatek. Bez propsu ekran bierze prawdziwy, z propsem ten z testu. */
  notes?: MemoryStore;
  /** Magazyn umiejętności. Osobny, bo polityki są osobne — patrz nagłówek pliku. */
  skills?: SkillsStore;
}

/**
 * Zdanie, kiedy nic jeszcze nie przeczytano.
 *
 * TRZECIA ODPOWIEDŹ, nie druga. Odczyt startuje w efekcie, czyli PO pierwszym malowaniu, więc
 * zaproszenie „nic tu nie ma" stałoby nad katalogami, których nikt jeszcze nie otworzył, i było
 * nieprawdziwe dokładnie tak długo, jak trwa czytanie dysku. Pusta lista i lista jeszcze
 * nieprzeczytana wyglądają w danych identycznie.
 */
const STILL_READING = 'Reading what your agents know.';

/** Co dokładnie się teraz dzieje — katalogi notatek, biegów i narzędzi agentowych. */
const READING_THE_FOLDERS =
  'Loadout is looking through your notes, your past runs and the folders your agent apps use.';

/** Przeczytaliśmy i naprawdę nic tam nie ma (DESIGN §6: pusty stan jest zaproszeniem). */
const NOTHING_YET = 'Nothing here yet.';

/**
 * Zdanie pod zaproszeniem — i jedyne miejsce, w którym różnica pada, zanim powstanie pierwsza
 * półka.
 *
 * Człowiek, który wchodzi tu pierwszy raz, nie zobaczy ani jednej półki, bo obie są puste —
 * a to jest dokładnie ta chwila, w której musi się dowiedzieć, czym te dwie rzeczy się różnią.
 */
const THE_DIFFERENCE =
  'Notes go into every prompt. Skills get picked up when they fit the work. Agents write both ' +
  'as they go, and you say which ones stay.';

function activeCatalogFolder(): string | null {
  return activeWorkspace()?.folder ?? null;
}

export default function KnowledgeScreen({
  notes = useMemory,
  skills = useSkills,
}: KnowledgeScreenProps): ReactElement {
  const noteState = useSyncExternalStore(notes.subscribe, notes.getState, notes.getState);
  const skillState = useSyncExternalStore(skills.subscribe, skills.getState, skills.getState);
  const catalogFolder = useSyncExternalStore(
    useWorkspaces.subscribe,
    activeCatalogFolder,
    activeCatalogFolder,
  );

  /* Notatki czyta się PER PROJEKT, więc przełączenie projektu w bocznym menu pyta jeszcze raz.
   * `void`, bo odmowa jest obsłużona w magazynie i ląduje w jego stanie jako zdanie dla
   * człowieka; drugie `catch` tutaj byłoby drugim miejscem, w którym mieszka ta sama decyzja. */
  useEffect(() => {
    void notes.getState().load(catalogFolder);
  }, [catalogFolder, notes]);

  /* Umiejętności leżą w katalogach narzędzi agentowych, a nie w projekcie, więc ten odczyt nie
   * zależy od otwartego projektu i biegnie RAZ na zamontowanie.
   *
   * DRUGI ODCZYT, DRUGIE PYTANIE. „Co leży w katalogach agentów" i „kogo mam zapisanych" to dwa
   * różne fakty i dwie różne komendy; bez tego wiersza wybór autora w panelu dodawania byłby
   * pusty na każdej maszynie, czyli kontrolką, której nie da się użyć (niezmiennik 16). */
  useEffect(() => {
    void skills.getState().load();
    void skills.getState().loadAgents();
  }, [skills]);

  /* CZY NA TYM EKRANIE JEST COKOLWIEK. Liczone z OBU magazynów, bo półka jest jedna z dwóch
   * i pusta strona nie jest pustym ekranem.
   *
   * OTWARTY PANEL DODAWANIA LICZY SIĘ JAKO „coś jest", i to nie jest szczegół. Zaproszenie
   * rysuje się ZAMIAST półek, a panel dodawania mieszka w półce umiejętności — bez tego wiersza
   * przycisk „Add a skill" w zaproszeniu otwierałby panel, którego nie ma na ekranie, czyli
   * byłby kontrolką bez widocznego skutku (niezmiennik 16). Złapane 2026-08-31 przez
   * `e2e/tests/no-dead-controls.spec.ts`, które klika każdy przycisk naprawdę. */
  const nothingOnScreen =
    noteState.notes.length === 0 &&
    noteState.passed.length === 0 &&
    skillState.installed.length === 0 &&
    skillState.pending === null &&
    skillState.adding === null;
  /* Odmowa którejkolwiek ze stron ZDEJMUJE zaproszenie. „Nic tu jeszcze nie ma" i „nie umiem
   * tego przeczytać" to dwie różne rzeczy do zrobienia, a zaproszenie postawione nad odmową
   * mówi człowiekowi, że nie ma nic do roboty — i to jest nieprawda o katalogu, który bywa
   * pełny. Zdania obu odmów mieszkają w swoich półkach, więc wtedy rysujemy półki. */
  const nothingWentWrong =
    noteState.message === null &&
    noteState.passedProblem === null &&
    skillState.message === null &&
    skillState.folders !== 'unreadable';
  /** Czy OBIE strony już odpowiedziały. Jedna nieprzeczytana wystarczy, żeby nie mówić „nic". */
  const bothAnswered = noteState.read && skillState.folders === 'read';

  return (
    <section className="flex h-full flex-col">
      {/* Pasek nagłówka jest chrome, więc materiał bierze z klasy materiału: `.screen-head`
          celowo nie ma tła, żeby `prefers-reduced-transparency` dotyczyło go tak samo jak
          reszty szkła (`src/styles/theme.css`). Nazwa sekcji pada TU i tylko tu — półki mają
          nagłówki stref, nie własne paski (niezmiennik 13). */}
      <header className="screen-head glass">
        <h1 className="text-title text-ink">Knowledge</h1>
      </header>

      <div className="screen-body">
        {nothingOnScreen && nothingWentWrong ? (
          <div className="flex h-full flex-col items-center justify-center gap-3">
            <span className="mark">◇</span>
            {/* `data-empty` na elemencie z samym zdaniem — tak samo jak w `src/App.tsx`.
                JEDEN znacznik na oba zdania, bo to jedno miejsce w dwóch chwilach: sekcja
                najpierw nie wie, potem wie i mówi, że nie ma nic. */}
            <p data-empty className="text-ink">
              {bothAnswered ? NOTHING_YET : STILL_READING}
            </p>
            <p className="lead max-w-160 text-center">
              {bothAnswered ? THE_DIFFERENCE : READING_THE_FOLDERS}
            </p>
            {/* Zaproszenie bez czynnej drogi dalej jest ślepym zaułkiem. Umiejętność człowiek
                dopisuje sam; notatki pisze AGENT w trakcie biegu, więc przycisku „dodaj
                notatkę" tu nie ma i mieć nie może — byłby kontrolką bez czynności
                (niezmiennik 16). */}
            <button
              data-create
              type="button"
              className="btn-primary"
              onClick={() => {
                skills.getState().openAdd();
              }}
            >
              ＋ Add a skill
            </button>
          </div>
        ) : (
          <NotesShelf store={notes} nextShelf={<SkillsShelf store={skills} />} />
        )}
      </div>
    </section>
  );
}
