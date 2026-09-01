/* Ekran sekcji Workflows: lista tego, co leży w katalogu workflow.
 *
 * CIENKI Z ZAŁOŻENIA I TO JEST CAŁA TREŚĆ TEGO PLIKU. Nagłówek z licznikiem, przycisk
 * tworzenia, zaproszenie przy zerze i pytanie przed usunięciem stoją już w `WorkflowList`
 * (T-14) i mają tam własne kryteria. Drugi nagłówek albo drugie zaproszenie tutaj byłoby
 * drugim miejscem prawdy (niezmiennik 23), a w markupie DRUGIM przyciskiem tworzenia, czyli
 * drugą ścieżką, którą powstaje plik (niezmiennik 16). Między komponentem a sekcją brakowało
 * dokładnie dwóch rzeczy: magazynu i tego pliku.
 *
 * 2026-08-17 — PŁÓTNO JEST JUŻ TUTAJ. Do tego dnia ten akapit mówił „płótno jest świadomie
 * poza zakresem", a `canvas.tsx` i `step-panel/` miały testy i ani jednego miejsca montowania:
 * do edytora nie prowadziło żadne kliknięcie. Ten plik trzyma teraz jeden fakt — który plik
 * jest otwarty — i przełącza między listą a `editor.tsx`.
 *
 * DLACZEGO MAGAZYN NIE JEST CZYTANY HAKIEM ZUSTANDA — zmierzone 2026-08-16.
 * `renderToStaticMarkup` jest rendererem serwerowym, a zustand 5 podaje mu `getInitialState`
 * jako migawkę serwerową (`node_modules/zustand/esm/react.mjs`). Ekran czytający magazyn
 * hakiem `useStore` pokazywałby więc stan Z CHWILI UTWORZENIA magazynu i nigdy tego, co
 * wczytał `load()`: sonda mówiła dwie pozycje z `getState()` i zero w markupie. Trzecim
 * argumentem `useSyncExternalStore` jest tu dlatego STAN BIEŻĄCY — ta aplikacja nigdy nie
 * hydratuje serwerowego HTML-a, więc powód, dla którego React chce tam stanu początkowego
 * (zgodność hydratacji), tutaj nie istnieje.
 *
 * ADAPTER DYSKU MIESZKA W `./io.ts` i ten plik tylko go podstawia. Poprzednia wersja tego
 * akapitu tłumaczyła, dlaczego adapter stoi w środku — powodem był brak warstwy IPC, którą
 * dowiozło T-27. Powód wygasł, więc zaślepka też.
 */
import type { ReactElement } from 'react';
import { useEffect, useState, useSyncExternalStore } from 'react';

import { useWorkspaces } from '../../state/workspaces';
import type { WorkflowListIo } from './list/store';
import { createWorkflowListStore } from './list/store';
import { WorkflowList } from './list/workflow-list';
import * as Disk from './io';
import * as agentsIo from '../agents/io';
import { listSkills } from '../skills/io';
import { requestRun } from '../run/requested';
import type { Agent } from '../../state/agents';
import type { WorkflowFile } from '../../state/workflows';
import { useSectionStore } from '../../ui/shell/section-store';
import { why } from '../../ipc/why';
import { healed } from './canvas/map';
import { WorkflowEditor } from './editor';

/** Magazyn listy workflow — dokładnie ten, który oddaje `createWorkflowListStore`. */
export type WorkflowListStore = ReturnType<typeof createWorkflowListStore>;

export interface WorkflowsScreenProps {
  /**
   * Magazyn ekranu. Bez propsu ekran bierze swój prawdziwy, z propsem ten z testu —
   * dokładnie tak, jak powłoka przyjmuje opcjonalne `screens` (`src/App.tsx`).
   */
  store?: WorkflowListStore;
}

/* Zdanie odmowy jedzie do tego, kto wołał, jako `Error`. Sekcja nie ma go dziś gdzie pokazać
 * — obsługa błędów plików należy do T-12 — ale zapis, który po cichu KOŃCZY SIĘ SUKCESEM,
 * byłby kłamstwem o tym, co leży na dysku (niezmiennik 4), a to jest gorsze niż cisza. */
/* Adapter dysku sekcji — PRAWDZIWY, od 2026-08-17.
 *
 * Do tego dnia stała tu zaślepka z komentarzem „katalog workflow zakłada strona Rusta, której
 * jeszcze nie ma". Ta strona powstała w T-27, a `src/sections/workflows/io.ts` eksportował
 * komplet od kilkunastu godzin i nie miał ani jednego produkcyjnego wołającego.
 *
 * To jest wada, która nie krzyczy: `list` oddający pustą tablicę czyta się identycznie jak
 * pusty katalog, więc ekran wygląda na poprawny, a `write` odmawia dopiero pod palcem.
 *
 * Adnotacja typu zostaje z rozmysłem: moduł eksportuje więcej niż `WorkflowListIo` (`load`
 * i `check` należą do płótna), a to podstawienie ma sprawdzać, że NADAL niesie te cztery
 * funkcje, których chce magazyn listy. */
const DISK: WorkflowListIo = { ...Disk, list: Disk.listDefinitions };

/* Prawdziwy magazyn sekcji powstaje RAZ, przy wczytaniu modułu, a nie przy renderze: magazyn
 * budowany w ciele komponentu gubiłby całą zawartość ekranu przy każdym przemontowaniu. */
const OWN_STORE = createWorkflowListStore(DISK);

export default function WorkflowsScreen({ store = OWN_STORE }: WorkflowsScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);

  /* KTÓRY FOLDER CZYTAMY (2026-08-29, T-164). Katalog workflow należy od tego dnia do
   * workspace'u, więc przełączenie karty zmienia ZBIÓR PLIKÓW, a nie tylko nagłówek. Bez tego
   * pola w zależnościach efekt niżej biegłby raz na wejście do sekcji i pokazywałby katalog
   * projektu, z którego się wyszło — czyli dokładnie tę wadę, którą to zadanie zamyka. */
  const readingFolderOf = useWorkspaces((state) => state.activeId);

  /* Katalog czytamy przy wejściu na sekcję. Pliki są prawdą, a ekran jest ich widokiem
   * (niezmiennik 4) — lista, która nigdy nie pyta dysku, pokazuje to, co pamięta z ostatniego
   * zapisu tego okna. `store` w zależnościach, bo z propsem może przyjechać inny magazyn. */
  useEffect(() => {
    void store.getState().load();
  }, [store, readingFolderOf]);

  /* Który plik jest otwarty w edytorze. `null` znaczy „lista" — jeden fakt, jedno miejsce,
   * bez drugiego boolean-a „czy edytujemy" (niezmiennik 13). */
  /* Rewizja jedzie w tej samej parze co dokument, bo opisuje DOKŁADNIE te bajty, które
   * przyjechały: pobrana osobnym wywołaniem opisywałaby inną chwilę niż to, co widać na
   * płótnie — a wtedy zapis odmawiałby albo nadpisywał, w zależności od pogody (2026-08-28). */
  const [open, setOpen] = useState<{
    path: string;
    document: WorkflowFile;
    revision: string;
  } | null>(null);
  const [agents, setAgents] = useState<readonly Agent[]>([]);
  const [skills, setSkills] = useState<readonly string[]>([]);
  /* Jedno miejsce na to, czego nie udało się otworzyć. Do 2026-08-18 nie było go wcale:
   * `Disk.load(path).then(setOpen)` stało bez `catch`, więc odmowa Rusta ginęła w cichej
   * odrzuconej obietnicy, a plik, którego NIE dało się przeczytać, wjeżdżał do edytora jako
   * `document` i zabijał sekcję na `state.document.steps` (zmierzone w przeglądarce). */
  const [said, setSaid] = useState<string | null>(null);

  /* Biblioteka agentów jedzie do panelu kroku: panel pokazuje wartości EFEKTYWNE, więc bez
   * agenta nie umie odróżnić nadpisania od dziedziczenia. Czytamy ją raz, przy wejściu na
   * sekcję — a nie przy otwarciu każdego workflow, bo to ta sama lista. */
  useEffect(() => {
    let alive = true;
    agentsIo
      .list()
      .then((found) => {
        if (alive) setAgents(found);
      })
      .catch(() => {
        /* Brak biblioteki nie ma prawa zamknąć edytora: panel kroku bez agenta pokazuje
         * zaproszenie zamiast wartości, a płótno działa dalej. */
        if (alive) setAgents([]);
      });
    return () => {
      alive = false;
    };
  }, []);

  /* Umiejętności, które NAPRAWDĘ leżą w katalogach agentów. Wiersz Skills w panelu kroku ma
   * z czego wybierać albo nie ma go wcale — lista wpisana z palca byłaby polem, które zapisuje
   * do pliku nazwę umiejętności, jakiej na dysku nie ma (niezmiennik 17).
   *
   * Adapter jest CUDZY (`../skills/io`) i to jest świadome: nazwa komendy `list_skills` ma jedno
   * miejsce w repo (niezmiennik 23), a dopisanie jej drugi raz w `./io.ts` dałoby dwie krawędzie
   * do tej samej odpowiedzi. Ten sam kierunek importu ma już `src/state/skills.ts`. */
  useEffect(() => {
    let alive = true;
    listSkills()
      .then((found) => {
        if (alive) setSkills(found.map((one) => one.name));
      })
      .catch(() => {
        /* Brak umiejętności na dysku i nieudany odczyt dają ten sam skutek dla panelu: wiersza
         * Skills nie ma. To nie jest połknięcie błędu, bo nie ma tu żadnej obietnicy do złamania
         * — edytor workflow nie jest miejscem, w którym naprawia się bibliotekę umiejętności. */
        if (alive) setSkills([]);
      });
    return () => {
      alive = false;
    };
  }, []);

  if (open !== null) {
    return (
      /* `key={path}` jest MECHANIZMEM, nie ozdobą: magazyn dokumentu powstaje raz na
       * zamontowanie edytora, więc wymiana otwartego pliku musi przemontować ekran.
       * Bez tego drugie „Open" pokazałoby nowy dokument w magazynie starego. */
      <WorkflowEditor
        key={open.path}
        path={open.path}
        document={open.document}
        revision={open.revision}
        agents={agents}
        skills={skills}
        onClose={() => {
          setOpen(null);
          /* Katalog czytamy PONOWNIE po zamknięciu: autosave mógł zmienić nazwę workflow,
           * a lista pokazująca starą nazwę jest widokiem, który rozjechał się z dyskiem. */
          void store.getState().load();
        }}
        /* 2026-08-18 — TO JEST NAPRAWA KŁAMIĄCEGO PRZYCISKU RUN. Do tego dnia stało tu
         * `() => { useSectionStore.getState().go('run'); }`: `editor.tsx` podawał `path`,
         * a ta linia go WYRZUCAŁA, więc w całym łańcuchu nie było ani jednego `invoke`.
         * Klikasz Run, ekran przeskakuje, nic nie startuje i nic tego nie mówi.
         *
         * `requestRun` zapisuje intencję („uruchom TEN plik") i sam przechodzi na ekran pracy.
         * `start()` NIE jest tu wołane z rozmysłem: polityka startu — zapadka na drugie
         * kliknięcie, limit „ile naraz", folder z aktywnej karty — mieszka w sekcji Run
         * (niezmiennik 23), a druga jej kopia tutaj rozjechałaby się przy pierwszej zmianie. */
        onRun={requestRun}
        onCreateAgent={() => {
          useSectionStore.getState().go('agents');
        }}
      />
    );
  }

  return (
    <>
      {/* Zdanie o pliku, którego nie dało się otworzyć. Cicha porażka wygląda dokładnie jak
          martwy kafelek: człowiek klika i nie dowiaduje się, że plik jest zepsuty. */}
      {said === null ? null : (
        <p data-said className="px-4 pt-3 text-body text-fail">
          {said}
        </p>
      )}
      {/* Cały stan idzie jako `actions`: `WorkflowListState` rozszerza `WorkflowListActions`,
          więc to jest TEN SAM obiekt, który magazyn wystawia — a nie jego przepisana kopia.

          KLAMRY SĄ TU KONIECZNE, NIE OZDOBNE, i to jest zapisany incydent z 2026-08-18. Po
          owinięciu sekcji we fragment ten komentarz znalazł się między znacznikami BEZ klamer,
          a taki zapis jest w JSX zwykłym TEKSTEM: cała polska proza renderowała się jako widoczny
          wiersz nad nagłówkiem „Workflows". Widać to było na zrzucie z przeglądarki i na tym, że
          prettier zaczął ją zawijać jak zdanie. Ani jedno z 571 kryteriów tego nie złapało —
          `renderToStaticMarkup` pyta, czy w markupie coś jest, a nie co tam stoi. */}
      <WorkflowList
        workflows={state.workflows}
        problems={state.problems}
        pendingDeleteId={state.pendingDeleteId}
        actions={state}
        onOpen={(path) => {
          /* Dokument bierzemy z DYSKU, a nie z pozycji listy: lista trzyma migawkę z chwili
           * odczytu katalogu, a edytor ma otwierać to, co naprawdę tam leży (niezmiennik 4). */
          setSaid(null);
          void Disk.load(path)
            .then((opened) => {
              /* STRAŻ KSZTAŁTU, nie zaufanie do typu. Sygnatura mówi `Promise<OpenWorkflow>`,
               * ale po drugiej stronie granicy nie ma żadnych typów — jest JSON. Plik poprawiony
               * ręcznie, zmergowany gitem albo oddany przez starszy build potrafi przyjechać bez
               * `steps`, a edytor czyta `document.steps.find(...)` w pierwszym renderze i wtedy
               * ginie CAŁA sekcja. Zmierzone w przeglądarce 2026-08-18:
               * „TypeError: Cannot read properties of null (reading 'steps')". */
              if (!Array.isArray(opened.workflow?.steps)) {
                setSaid(
                  'Loadout could not read ' + path + '. Open the file and check it, or delete it.',
                );
                return;
              }
              /* LECZENIE PRZY OTWARCIU, tą samą drogą i z tego samego powodu, co straż wyżej:
               * plik na dysku bywa poprawiony ręcznie, zmergowany gitem albo zapisany przez
               * build sprzed naprawy z 2026-08-22 — a wtedy niesie strzałkę w krok, którego
               * w nim nie ma. Bez tej linii uwaga o niej wisi na ekranie, dopóki człowiek
               * czegoś nie skasuje, choć to nie on ją narysował.
               *
               * Rewizja opisuje bajty SPRZED leczenia i tak ma zostać: to jest odpowiedź na
               * pytanie „co leżało na dysku", a nie „co pokazałem". Zapis wyleczonego pliku
               * ma nadpisać dokładnie to, co przeczytał. */
              setOpen({ path, document: healed(opened.workflow), revision: opened.revision });
            })
            .catch((error: unknown) => {
              setSaid(why(error, 'Loadout could not open ' + path + '.'));
            });
        }}
      />
    </>
  );
}
