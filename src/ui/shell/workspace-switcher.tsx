/* Przełącznik zakresu na górze bocznego menu — „gdzie pracują moi agenci", powiedziane RAZ.
 *
 * DECYZJA WŁAŚCICIELA, 2026-08-18. Do tego dnia folder pracy wybierało się systemowym okienkiem
 * przy KAŻDYM uruchomieniu workflow. Zdanie właściciela brzmiało „ten wybór jak odpalam workflow
 * to mega chujnia, my powinniśmy wybierać projekt z poziomu UI sidebar" i było trafne: wybór
 * folderu jest decyzją o projekcie, podejmowaną raz, a nie czynnością powtarzaną przed każdą
 * pracą. To NADPISUJE makietę w tym jednym miejscu — makieta ma pasek kart folderów i regułę
 * „jeden folder = jedna karta" (`AGENTS.md` §6a), a słowo właściciela stoi nad makietą.
 *
 * „UI JAK W CLICKUP" ZNACZY TU DOKŁADNIE TYLE: nazwa zakresu na górze bocznego menu, klikalna,
 * rozwija listę zakresów plus pozycję dodania. NIE znaczy: kolorowych ikonek, avatarów ani
 * drugiego poziomu nawigacji — 196 px szerokości nie ma na to miejsca, a drugi poziom nawigacji
 * jest dokładnie tym, czego `docs/ARCHITECTURE.md` §7 zabrania („boczne menu odpowiada na »co
 * robię«, karty na »w którym folderze«").
 *
 * PRZEŁĄCZENIE NIE GUBI SESJI, i to jest wymóg twardy. Ten plik spełnia go przez to, czego NIE
 * robi: `activate` jest wyłącznie zmianą widoku, nie zatrzymuje biegu i nie kasuje kart. Pompa
 * linii należy do KARTY po stronie Rusta (`src-tauri/src/workspace.rs`), nie do widoku — wersja,
 * w której wisi na tym, co akurat widać, przechodzi każdy test pisany na zakresie aktywnym
 * i gubi linie dokładnie wtedy, kiedy człowiek zajrzy gdzie indziej.
 *
 * DLACZEGO STAN OKIENKA MIESZKA W MAGAZYNIE, A NIE W `useState`. To repo nie ma jsdom, więc
 * kliknięcia nie da się odpalić w teście, a `renderToStaticMarkup` nigdy nie woła `onClick`.
 * Handler zamknięty w komponencie byłby więc kodem, którego żadne kryterium nie umie dotknąć —
 * a to jest ta sama rodzina, z której wzięło się 17 kłamiących kontrolek. Tutaj test woła
 * dokładnie to, co woła przycisk, i pyta magazyn, co się zmieniło.
 *
 * DWÓCH PRODUCENTÓW ODMOWY, JEDNO MIEJSCE WYŚWIETLENIA. `useWorkspaces.said` mówi, czego
 * odmówił DYSK; `useSwitcher.troubled` mówi, czego nie dało się zrobić w samym oknie (okienko
 * wyboru folderu jest wtyczką systemu, nie komendą Loadouta, i odmawia własną drogą). To dwa
 * różne podmioty, a nie dwie kopie tego samego faktu — natomiast zdanie na ekranie jest JEDNO
 * i składa je jedna funkcja [`refusal`], a `Dismiss` czyści oba. Kontrakt magazynu jest
 * zamrożony i nie ma w nim pola na odmowę okienka systemowego; gdyby kiedyś było, ta funkcja
 * i to pole znikają razem.
 */
import { useEffect, useSyncExternalStore } from 'react';
import type { ChangeEvent, ReactElement } from 'react';
import { create } from 'zustand';

import { why } from '../../ipc/why';
/* OKNO WYBORU FOLDERU JEST IMPORTOWANE, NIE SKOPIOWANE. `chooseWorkingFolder` jest dziś jedyną
 * drogą do wtyczki `dialog:allow-open` (`src/sections/run/folders.ts`) i druga kopia tego
 * wywołania byłaby drugim miejscem, w którym mieszka odpowiedź na pytanie „skąd bierze się
 * folder" (niezmiennik 23).
 *
 * SKUTEK UBOCZNY ZDJĘTY W TEJ SAMEJ FALI, 2026-08-18. Do dziś tamta funkcja przy okazji OTWIERAŁA
 * KARTĘ na wybranym folderze, bo karta ZNACZYŁA folder. Po rozstrzygnięciu właściciela znaczy coś
 * innego (karta = bieg wewnątrz zakresu), więc `chooseWorkingFolder` jest dziś czystym wyborem
 * ścieżki i nie zapisuje niczego. Kto z tą ścieżką co zrobi, decyduje wołający — czyli `save`
 * niżej, i tylko po potwierdzeniu z dysku. */
import { chooseWorkingFolder, folderName } from '../../sections/run/folders';
import type { Workspace } from '../../state/workspaces';
import { useWorkspaces } from '../../state/workspaces';

/** Co powiedzieć, kiedy człowiek nacisnął Save, a folderu jeszcze nie wskazał. */
export const NO_FOLDER_YET =
  'Choose the folder first — a workspace is a folder your agents work in.';

/** Napis na kontrolce, która rozwija listę, kiedy żaden zakres nie jest wybrany. */
export const NOTHING_PICKED = 'Choose a workspace';

/** Zaproszenie z pustego stanu. Jedno zdanie, tryb rozkazujący (DESIGN §6). */
export const FIRST_INVITE = 'Add a workspace';

/**
 * Stan samego okienka: rozwinięte, dodawane, i co jest wpisane w formularzu.
 *
 * Świadomie POZA `useWorkspaces`: nic z tego nie jest stanem trwałym i nic z tego nie ma prawa
 * dotknąć dysku. Kontrakt tamtego magazynu jest zamrożony i wpychanie tam pól okienka
 * zamieniłoby „gdzie pracujemy" w „co jest teraz rozwinięte".
 */
export interface SwitcherState {
  /** Czy lista zakresów jest rozwinięta. */
  readonly open: boolean;
  /** Czy stoi formularz dodawania. */
  readonly adding: boolean;
  /** Nazwa wpisywana w formularzu. */
  readonly name: string;
  /** Folder wskazany w formularzu; `null`, dopóki człowiek go nie wskazał. */
  readonly folder: string | null;
  /** Czego nie dało się zrobić w oknie (nie na dysku); `null`, kiedy nic. */
  readonly troubled: string | null;

  /** Rozwija i zwija listę. */
  toggle: () => void;
  /** Wybiera zakres i zwija listę. Dysku nie dotyka. */
  choose: (id: string) => void;
  /** Otwiera formularz dodawania. */
  startAdd: () => void;
  /** Zamyka formularz i porzuca wpisane wartości. */
  cancelAdd: () => void;
  /** Zapisuje wpisywaną nazwę. */
  typeName: (name: string) => void;
  /** Pyta system o folder. Anulowanie jest wartością, nie błędem (niezmiennik 7). */
  pickFolder: () => Promise<void>;
  /** Oddaje formularz dyskowi. Formularz zamyka się WYŁĄCZNIE na potwierdzenie z dysku. */
  save: () => Promise<void>;
  /** Człowiek przeczytał odmowę — obie, bo na ekranie stoi jedna. */
  dismiss: () => void;
}

export const useSwitcher = create<SwitcherState>()((set, get) => ({
  open: false,
  adding: false,
  name: '',
  folder: null,
  troubled: null,

  toggle: () => {
    set({ open: !get().open });
  },

  choose: (id) => {
    useWorkspaces.getState().activate(id);
    set({ open: false });
  },

  startAdd: () => {
    set({ adding: true, open: true, name: '', folder: null, troubled: null });
  },

  cancelAdd: () => {
    set({ adding: false, name: '', folder: null, troubled: null });
  },

  typeName: (name) => {
    set({ name });
  },

  pickFolder: async () => {
    try {
      const picked = await chooseWorkingFolder();
      /* `null` znaczy „człowiek się rozmyślił" i nie jest błędem — nie mówimy o tym niczego,
       * bo cisza po anulowaniu WŁASNEGO okienka jest tym, czego człowiek się spodziewa. */
      if (picked === null) return;
      /* Nazwa podpowiedziana z folderu: dodanie zakresu to wtedy jedno kliknięcie i Save.
       * Wpisanej ręcznie nie nadpisujemy — to byłoby odebranie człowiekowi jego własnego słowa. */
      set({
        folder: picked,
        name: get().name.trim() === '' ? folderName(picked) : get().name,
        troubled: null,
      });
    } catch (error) {
      set({ troubled: why(error, 'Loadout could not open the folder chooser.') });
    }
  },

  save: async () => {
    const { name, folder } = get();
    if (folder === null) {
      set({ troubled: NO_FOLDER_YET });
      return;
    }
    /* Pustej nazwy NIE odsiewamy tutaj: odmawia jej Rust, zdaniem lepszym niż nasze („Give this
     * workspace a name first — that name is how you will pick it later"), i to jest to samo
     * zdanie, które człowiek zobaczy przy każdej innej przyczynie. Dwa miejsca decydujące, czy
     * nazwa jest dobra, rozjadą się przy pierwszej zmianie po stronie dysku. */
    const done = await useWorkspaces.getState().add(name, folder);
    /* Formularz zostaje otwarty, kiedy dysk odmówił: wpisane wartości są tym, co człowiek chce
     * poprawić, a zdanie o odmowie stoi nad nim. */
    if (!done) return;
    set({ adding: false, open: false, name: '', folder: null, troubled: null });
  },

  dismiss: () => {
    useWorkspaces.getState().dismiss();
    set({ troubled: null });
  },
}));

/**
 * Jedno zdanie odmowy na ekran, albo `null`.
 *
 * Dysk pierwszy: kiedy odmówił i dysk, i okienko, człowiek ma przeczytać to, co powiedział plik.
 * Odmowa okienka jest wtedy nieaktualna — próba, której dotyczyła, już się skończyła.
 */
export function refusal(disk: string | null, inWindow: string | null): string | null {
  return disk ?? inWindow;
}

/* Klasy w jednym miejscu, bo trzy kontrolki mają być tą samą kontrolką. Wysokość 32 px
 * (`h-control`), promień kontrolki (`rounded-sm`), obrys `--line-strong` i tło `--raised`
 * i §6. `--accent` wyłącznie na kontrolce, która coś ZAPISUJE (reguła jednego akcentu). */
const TRIGGER =
  'flex h-control w-full items-center justify-between gap-2 rounded-sm border border-line-strong bg-raised px-[10px] text-ui text-ink';
const ITEM =
  'w-full truncate rounded-sm border border-transparent px-[10px] py-[7px] text-left text-ui text-body aria-[checked=true]:border-line aria-[checked=true]:bg-raised aria-[checked=true]:text-ink';
const QUIET =
  'w-full truncate rounded-sm border border-transparent px-[10px] py-[7px] text-left text-ui text-accent';
const SECONDARY = 'h-control rounded-sm border border-line-strong bg-raised px-3 text-ui text-ink';
const PRIMARY = 'h-control rounded-sm bg-accent px-3 text-ui text-bg';

export interface WorkspaceSwitcherProps {
  /** Zakresy zapisane na dysku. */
  readonly all: readonly Workspace[];
  /** Który jest aktywny. */
  readonly activeId: string | null;
  /** Czego odmówił dysk. */
  readonly said: string | null;
  /** Stan okienka. */
  readonly ui: Pick<SwitcherState, 'open' | 'adding' | 'name' | 'folder' | 'troubled'>;
}

/**
 * Przełącznik — komponent STEROWANY, żeby dał się wyrenderować na dowolnym stanie.
 *
 * Wszystko wchodzi propsami, a handlery wołają magazyny modułowe wprost. Dzięki temu render
 * i zachowanie sądzi się osobno: render na wstawionych danych, zachowanie przez wołanie tych
 * samych funkcji, które woła przycisk.
 */
export function WorkspaceSwitcher({
  all,
  activeId,
  said,
  ui,
}: WorkspaceSwitcherProps): ReactElement {
  const act = useSwitcher.getState();
  const active = all.find((one) => one.id === activeId) ?? null;
  const sentence = refusal(said, ui.troubled);

  return (
    <div data-workspace-switcher className="flex flex-col gap-1 pb-4">
      {sentence === null ? null : (
        <div className="flex flex-col gap-2 rounded-md border border-fail-edge bg-fail-soft p-2">
          <p data-workspace-said className="text-note text-fail">
            {sentence}
          </p>
          <button
            type="button"
            data-workspace-dismiss
            onClick={() => {
              act.dismiss();
            }}
            className={SECONDARY}
          >
            Dismiss
          </button>
        </div>
      )}

      {ui.adding ? (
        <div className="flex flex-col gap-2">
          {/* Prawdziwa etykieta z `htmlFor`, nie `<span>` obok pola: bez powiązania czytnik
              ekranu czyta pole bez nazwy, a kliknięcie w napis nie ustawia w nim kursora. */}
          <label className="text-label text-muted" htmlFor="workspace-name">
            Name
          </label>
          <input
            id="workspace-name"
            data-workspace-name
            className="field"
            value={ui.name}
            placeholder="What you call this project"
            onChange={(event: ChangeEvent<HTMLInputElement>) => {
              act.typeName(event.target.value);
            }}
          />
          <button
            type="button"
            data-workspace-folder
            onClick={() => {
              void act.pickFolder();
            }}
            className={TRIGGER}
          >
            <span className="truncate">
              {ui.folder === null ? 'Choose folder' : folderName(ui.folder)}
            </span>
          </button>
          <div className="flex gap-2">
            <button
              type="button"
              data-workspace-save
              onClick={() => {
                void act.save();
              }}
              className={PRIMARY}
            >
              Save
            </button>
            <button
              type="button"
              data-workspace-cancel
              onClick={() => {
                act.cancelAdd();
              }}
              className={SECONDARY}
            >
              Cancel
            </button>
          </div>
        </div>
      ) : all.length === 0 ? (
        /* PUSTY STAN JEST ZAPROSZENIEM, nie pustą listą wyboru (DESIGN §6: „pusty ekran to
         * zaproszenie do działania, nie komunikat o braku danych"). Rozwijana lista z zerem
         * pozycji jest gorsza niż nic: mówi „tu nic nie ma" i nie mówi, co z tym zrobić. */
        <button
          type="button"
          data-workspace-new
          data-workspace-invite
          onClick={() => {
            act.startAdd();
          }}
          className={PRIMARY}
        >
          {FIRST_INVITE}
        </button>
      ) : (
        <>
          <button
            type="button"
            data-workspace-open
            aria-expanded={ui.open ? 'true' : 'false'}
            onClick={() => {
              act.toggle();
            }}
            className={TRIGGER}
          >
            <span className="truncate">{active === null ? NOTHING_PICKED : active.name}</span>
            <span aria-hidden className="font-mono text-meta text-muted">
              {ui.open ? '▴' : '▾'}
            </span>
          </button>
          {ui.open ? (
            <div role="menu" className="flex flex-col">
              {all.map((one) => (
                <button
                  key={one.id}
                  type="button"
                  role="menuitemradio"
                  aria-checked={one.id === activeId ? 'true' : 'false'}
                  data-workspace-pick={one.id}
                  title={one.folder}
                  onClick={() => {
                    act.choose(one.id);
                  }}
                  className={ITEM}
                >
                  {one.name}
                </button>
              ))}
              <button
                type="button"
                data-workspace-new
                onClick={() => {
                  act.startAdd();
                }}
                className={QUIET}
              >
                {FIRST_INVITE}
              </button>
            </div>
          ) : null}
        </>
      )}
    </div>
  );
}

/**
 * Ten sam przełącznik, wpięty w oba magazyny — to jego montuje boczne menu.
 *
 * MAGAZYNY CZYTAMY PRZEZ `useSyncExternalStore` Z BIEŻĄCYM STANEM JAKO MIGAWKĄ SERWEROWĄ, a nie
 * hakiem `useWorkspaces(selector)`. `renderToStaticMarkup` jest rendererem serwerowym, a zustand 5
 * podaje mu `getInitialState`, więc menu czytane hakiem pokazywałoby stan Z CHWILI UTWORZENIA
 * magazynu i nigdy tego, co weszło z dysku. Ta aplikacja nigdy nie hydratuje serwerowego HTML-a,
 * więc powód, dla którego React chce tam stanu początkowego, tutaj nie istnieje. Ten sam zapis
 * stoi w `src/sections/run/index.tsx` i w `src/sections/workflows/index.tsx`.
 *
 * `load()` JEST TUTAJ, a nie w `src/main.tsx`: tamten plik nie należy do żadnego zadania tej
 * fali, a lista zakresów musi wejść z dysku dokładnie raz, przy starcie okna. Pusta tablica
 * zależności znaczy „raz na życie okna" — boczne menu nigdy się nie odmontowuje.
 */
export function NavWorkspaces(): ReactElement {
  const disk = useSyncExternalStore(
    useWorkspaces.subscribe,
    useWorkspaces.getState,
    useWorkspaces.getState,
  );
  const ui = useSyncExternalStore(
    useSwitcher.subscribe,
    useSwitcher.getState,
    useSwitcher.getState,
  );

  useEffect(() => {
    void useWorkspaces.getState().load();
  }, []);

  return <WorkspaceSwitcher all={disk.all} activeId={disk.activeId} said={disk.said} ui={ui} />;
}
