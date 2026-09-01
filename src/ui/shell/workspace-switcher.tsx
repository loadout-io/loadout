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

/* PIĘĆ STAŁYCH-LIST-KLAS ZOSTAŁO JEDNĄ, 2026-08-31. Cztery z nich były dosłownym opisem
 * kontrolki, którą warstwa prymitywów nazywa jednym słowem: `SECONDARY` to `.btn`, `PRIMARY`
 * to `.btn-primary`, `ITEM` i `QUIET` to `.row`. Opis przepisany w komponencie jest kopią
 * decyzji — a decyzji o geometrii przycisku było w tym repo pięć zapisów na trzy realne pary
 * pikseli (DESIGN §6). Zostaje jedna nazwa, bo niesie coś PONAD prymityw: pełną szerokość
 * i rozsunięcie nazwy od strzałki. `--accent` dalej wyłącznie na kontrolce, która ZAPISUJE. */
const TRIGGER = 'btn w-full justify-between';

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
      {/* ODMOWA PRZYCHODZI SPRĘŻYNĄ, 2026-08-31 (DESIGN §7). Ten blok POJAWIA SIĘ nad tym, co
          już stoi na ekranie — dysk albo okienko właśnie odmówiły — a element wskakujący
          skokiem nad czytaną treść czyta się jak przeskok widoku, nie jak odpowiedź na
          kliknięcie. JEDEN region: formularz pod spodem zostaje w drzewie i nie animuje się
          drugi raz, więc sufit dwóch regionów z ARCHITECTURE §7 nie jest przekroczony.
          `.card` niesie rolę pojemnika i ton awarii; `bg-fail-soft` zostaje, bo to jest to,
          co prymityw ma PONAD sobą — wypełnienie, którego karta neutralna nie ma. */}
      {sentence === null ? null : (
        <div className="enter card stack bg-fail-soft" data-tone="fail" data-gap="2">
          <p data-workspace-said className="lead" data-tone="fail">
            {sentence}
          </p>
          <button
            type="button"
            data-workspace-dismiss
            onClick={() => {
              act.dismiss();
            }}
            className="btn"
          >
            Dismiss
          </button>
        </div>
      )}

      {ui.adding ? (
        /* FORMULARZ WCHODZI SPRĘŻYNĄ (DESIGN §7): pojawia się nad listą, której miejsce zajął.
           `.stack` z odstępem 8 px — odstęp MIĘDZY wierszami formularza, nie wewnątrz pary
           etykieta+pole; baza prymitywu (4 px) jest tą drugą rolą. */
        <div className="enter stack" data-gap="2">
          {/* Prawdziwa etykieta z `htmlFor`, nie `<span>` obok pola: bez powiązania czytnik
              ekranu czyta pole bez nazwy, a kliknięcie w napis nie ustawia w nim kursora.
              Stopień i barwa idą z prymitywu `.label` — etykieta pola jest zdaniowa, bo
              wersaliki nosi nadoczko sekcji i ma na to własny stopień drabinki (DESIGN §4). */}
          <label className="label" htmlFor="workspace-name">
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
          {/* `items-center`, bo prymitywy rozstrzygają dwie różne wysokości na jeden wiersz:
              przycisk podstawowy ma 36 px, drugoplanowy 32 (DESIGN §6). Bez tego para stoi
              wyrównana górą i czyta się jak dwa wiersze, a nie jak jedna decyzja. */}
          <div className="flex items-center gap-2">
            <button
              type="button"
              data-workspace-save
              onClick={() => {
                void act.save();
              }}
              className="btn-primary"
            >
              Save
            </button>
            <button
              type="button"
              data-workspace-cancel
              onClick={() => {
                act.cancelAdd();
              }}
              className="btn"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : all.length === 0 ? (
        /* PUSTY STAN JEST ZAPROSZENIEM, nie pustą listą wyboru (DESIGN §6: „pusty ekran to
         * zaproszenie do działania, nie komunikat o braku danych"). Rozwijana lista z zerem
         * pozycji jest gorsza niż nic: mówi „tu nic nie ma" i nie mówi, co z tym zrobić.
         *
         * WAGA ZESZŁA Z `.btn-primary` NA `.btn`, 2026-08-31. Wypełnienie akcentem robiło
         * z tego przycisku najgłośniejszą rzecz w całym oknie — głośniejszą od treści — mimo
         * że jego rola jest dokładnie odwrotna: naciska się go RAZ w życiu instalacji i znika
         * na zawsze. Akcent znaczy „to jest interaktywne" (DESIGN §3), a nie „patrz tutaj",
         * i należy się temu, co człowiek ma zrobić TERAZ; na pierwszym uruchomieniu jest to
         * krok w przewodniku w strefie pracy (`src/sections/run/first-run.tsx`), a nie
         * pozycja w bocznym menu. Kontrolka zostaje czynna i robi to samo — zmieniła się
         * głośność, nie czynność (niezmiennik 16). */
        <button
          type="button"
          data-workspace-new
          data-workspace-invite
          onClick={() => {
            act.startAdd();
          }}
          className="btn"
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
            /* LISTA WCHODZI SPRĘŻYNĄ, POJEDYNCZE WIERSZE NIE (DESIGN §7, ARCHITECTURE §7).
               Rozwinięcie listy jest JEDNYM zdarzeniem, więc animuje się JEDEN region —
               pojemnik. Osobne wejście na każdym wierszu dałoby tyle regionów, ile zakresów,
               a sufit wynosi dwa; kaskada w menu i tak czyta się jak usterka, bo człowiek
               czeka na pozycję, w którą chce kliknąć. */
            <div role="menu" className="enter flex flex-col">
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
                  className="row"
                >
                  {/* Nazwa w `<span>`, a nie wprost w przycisku: `.row` jest kontenerem
                      flex, a w nim `text-overflow` nie dotyczy tekstu anonimowego — bez tego
                      długa nazwa zakresu byłaby ucięta bez wielokropka. */}
                  <span className="truncate">{one.name}</span>
                </button>
              ))}
              {/* Ten sam wiersz listy, w barwie akcentu: to jest pozycja, która COŚ ROBI,
                  a nie jeden z zakresów. `text-accent` to jedyna rzecz ponad prymitywem. */}
              <button
                type="button"
                data-workspace-new
                onClick={() => {
                  act.startAdd();
                }}
                className="row text-accent"
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
