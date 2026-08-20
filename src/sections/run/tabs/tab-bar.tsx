/* Pasek kart: karty terminali i jedno zdanie o czekaniu na miejsce.
 *
 * 2026-08-18 — ZNAK `＋` STĄD ZNIKNĄŁ I WRÓCIŁ TEGO SAMEGO DNIA, a jego znaczenie ustaliło się
 * dopiero 2026-08-20. Rozumowanie, które go najpierw skasowało, stoi tu w całości, bo dwie z jego
 * trzech gałęzi są dalej prawdziwe i dalej zabraniają czegoś:
 *   „Nowy zakres"  byłby drugą drogą do kontrolki, która stoi w bocznym menu i ma tam własny
 *                  formularz z nazwą — czyli drugim miejscem tej samej czynności, i to tym
 *                  gorszym, bo bez nazwy (niezmiennik 13). Tym `＋` NIE jest.
 *   „Nowy bieg"    jest przyciskiem Start, a Start bierze DWIE rzeczy, których na tym pasku nie
 *                  ma: który workflow i ilu agentów naraz. `＋` musiałby wybrać je po swojemu
 *                  i cicho zignorować to, co człowiek ustawił obok — kontrolka, która przyjmuje
 *                  polecenie i wykonuje inne, jest gorsza niż jej brak (niezmiennik 16). Tym
 *                  `＋` też NIE jest.
 *   „Nowy terminal" — trzecia możliwość, której w tamtym rozumowaniu zabrakło, i to ona jest
 *                  właściwa: kolejne MIEJSCE DO PRACY w projekcie, który człowiek już wybrał.
 *                  Nie pyta o nic, bo nie ma o co: zakres jest wybrany, a workflow wybiera się
 *                  dopiero przy Starcie. Karty zakłada więc i człowiek, i bieg.
 *
 * Wysokość jest wydana z góry i nie podlega negocjacji: karty biorą 34 z 96 px budżetu chrome,
 * a pasek loadoutu drugie 56 — zostaje sześć (ARCHITECTURE §7). Zdanie o czekaniu musi się więc
 * zmieścić W pasku kart, a nie nad nim ani pod nim. To nie jest ciasnota dla ciasnoty: poprzedni prototyp
 * podniósł swój wymuszany limit do 2,4× wartości docelowej i skończył ze 149 px chrome na każdym
 * ekranie.
 *
 * MILCZĄCE CZEKANIE JEST NIEODRÓŻNIALNE OD ZAWIESZENIA. Kiedy pula jest pełna, karta czekająca
 * na miejsce wygląda dokładnie jak zepsuta i człowiek ubija bieg, który był zdrowy. Dlatego
 * pasek mówi to wprost, i mówi z liczbami: ile miejsc jest zajętych, ile ich w ogóle jest
 * i w którym folderze agent stoi w kolejce.
 *
 * TO ZDANIE WYSTĘPUJE DOKŁADNIE RAZ (niezmiennik 13). Liczba zajętych miejsc jest jednym
 * faktem, więc ma dokładnie jedno żywe miejsce na ekranie; poprzedni prototyp pokazywał stan połączenia
 * w sześciu. Powtórzenie jej pod suwakiem „ile naraz" byłoby drugim takim miejscem i pierwszą
 * okazją do tego, żeby dwa miejsca powiedziały co innego.
 *
 * Liczby wchodzą PROPSEM, nie z magazynu kart. Pula jest jedna na całą aplikację
 * (niezmiennik 11), więc „ile zajętych" jest faktem o limiterze, a nie o pasku — magazyn kart,
 * który by je trzymał, byłby ich drugim domem.
 *
 * ZDANIE JEST SKŁADANE Z LICZB, KTÓRE PRZYSZŁY. Napis wpisany na sztywno czyta się tak samo
 * przy dwóch zajętych miejscach i przy trzech, więc kłamie od pierwszego przesunięcia suwaka
 * — i najgłośniej wtedy, gdy miejsce właśnie się zwolniło. Element znika w całości, kiedy
 * nikt nie czeka: zdanie, które zostaje na pasku po końcu czekania, uczy ludzi nie czytać paska.
 */
import type { ReactElement } from 'react';
import type { WorkspaceTab } from '../../../state/run-tabs';
import { Tab } from './tab';

/** Wysokość paska kart: 34 z 96 px budżetu chrome (ARCHITECTURE §7). */
/* 34 -> 32 (T-46). Plywajaca kartka nawigacji dokladа wlasny odstep do budzetu chrome
 * z ARCHITECTURE §7, wiec dwa paski nad trescia musialy oddac po dwa piksele:
 * 8 (odstep okna) + 1 (obrys kartki tresci) + 32 (karty) + 52 (pasek) = 93 przy sufi 96.
 * §7 mowi wprost, ze kolejny pasek wymaga usuniecia innego, nie podniesienia limitu. */
export const TAB_BAR_HEIGHT = 32;

export interface TabBarProps {
  /** Karty w kolejności, w jakiej mają stać. */
  readonly tabs: readonly WorkspaceTab[];
  /** Która karta jest na wierzchu; `null`, dopóki żadnej nie ma. */
  readonly activeId: string | null;
  /** Ile miejsc puli jest zajętych **w całej aplikacji**. */
  readonly busy: number;
  /** Ile miejsc pula ma w ogóle — liczba spod suwaka „ile naraz". */
  readonly atOnce: number;
  /**
   * Nazwa folderu, w którym agent czeka na wolne miejsce; `null`, kiedy nikt nie czeka.
   *
   * Nazwa, nie `boolean`: „ktoś gdzieś czeka" nie mówi człowiekowi, gdzie zajrzeć, a to jest
   * jedyny powód, dla którego to zdanie w ogóle stoi na ekranie.
   */
  readonly waitingIn: string | null;
  /** Wymagany: wybranie karty (niezmiennik 16). */
  readonly onSelect: (id: string) => void;
  /** Wymagany: `×` na karcie. */
  readonly onClose: (id: string) => void;
  /**
   * `＋` na końcu paska: NOWY TERMINAL w projekcie, który już wybrano.
   *
   * 2026-08-18 — WRACA PO ZGŁOSZENIU WŁAŚCICIELA („nie mogę dodawać nowych tabów"). Zniknął tego
   * samego dnia z rozumowaniem, które stoi w nagłówku tego pliku i było niegłupie: karta znaczy
   * teraz bieg, nie folder, więc `＋` mógłby znaczyć albo „nowy zakres" (druga droga do kontrolki
   * z bocznego menu), albo „nowy bieg" (czyli Start, który bierze dwie rzeczy, których na pasku
   * nie ma). Zabrakło w tym rozumowaniu trzeciej możliwości — i ona okazała się właściwa, tylko
   * nie tą, którą wtedy wybrano: `＋` wołał wybór folderu.
   *
   * 2026-08-20 (T-71) — CO BYŁO ZŁEGO W TAMTYM WYBORZE, zmierzone z kodu. Wybrany folder stawał
   * się nowym ZAKRESEM i od razu aktywnym, a pasek pokazuje karty aktywnego zakresu — a w świeżym
   * zakresie nie ma żadnej. Kliknięcie w `＋` nie dokładało więc karty **nigdy**: wymieniało
   * projekt i pasek robił się pusty. Kontrolka, która przyjmuje polecenie i wykonuje inne, jest
   * gorsza niż jej brak (niezmiennik 16), a właściciel powiedział to wprost: „a nie tak jak teraz
   * ze scope wybieramy znowu".
   *
   * NAZWA TEGO PROPSA JEST ZAPISANYM DŁUGIEM, nie opisem. Handler znaczy dziś „otwórz miejsce do
   * pracy tutaj", a folderu pyta wyłącznie wtedy, kiedy nie ma go gdzie postawić — o czym wie
   * ekran, nie pasek (`../index.tsx`, `newTerminalHere`). Przemianowanie na `onNewTerminal` jest
   * jedną linią tutaj i czterema w cudzych kryteriach, które ten props podają
   * (`./tabs.test.tsx`, `./live-dot.test.tsx`, `../entry/caret.test.tsx`,
   * `../entry-row.test.tsx`) — a przepisanie cudzego kryterium przy okazji zmiany o czymś innym
   * jest zmianą tego kryterium. Zgłoszone zamiast zrobione (AGENTS.md §7).
   */
  readonly onOpenFolder: () => void;
}

/**
 * Zdanie o czekaniu: gdzie stoi agent i ile miejsc jest zajętych z ilu.
 *
 * Sufit stoi obok licznika, bo „2 in use" nigdy nie zdradza, ile miejsc jest w ogóle —
 * a to jest dokładnie ta liczba, którą człowiek zaraz podniesie albo obniży (ARCHITECTURE §6a).
 */
function waitingSentence(busy: number, atOnce: number, folder: string): string {
  return (
    busy + ' of ' + atOnce + ' slots in use — an agent in ' + folder + ' is waiting for a free one.'
  );
}

export function TabBar({
  tabs,
  activeId,
  busy,
  atOnce,
  waitingIn,
  onSelect,
  onClose,

  onOpenFolder,
}: TabBarProps): ReactElement {
  return (
    <div
      data-tab-bar
      className="flex shrink-0 items-center gap-1 border-b border-line bg-panel px-2"
      style={{ height: TAB_BAR_HEIGHT }}
    >
      {tabs.map((workspace) => (
        <Tab
          key={workspace.id}
          workspace={workspace}
          active={workspace.id === activeId}
          onSelect={onSelect}
          onClose={onClose}
        />
      ))}

      {/* Ten sam znak, co w makiecie (`.tabadd`). Co się po nim dzieje, rozstrzyga ekran —
          powód w całości stoi przy `TabBarProps.onOpenFolder`.

          NAZWA MÓWI, CO POWSTANIE, a nie czego się w tym celu wybiera. Etykieta „Open a folder"
          stała tu do 2026-08-20 i była zarazem przyczyną i opisem defektu: człowiek naciskał
          `＋` po drugie miejsce do pracy w projekcie, który już wybrał, a dostawał pytanie o ten
          projekt drugi raz. */}
      <button
        type="button"
        data-add-tab
        onClick={onOpenFolder}
        aria-label="New terminal"
        title="Open another terminal in this project"
        className="h-7 shrink-0 rounded-sm px-2 text-muted"
      >
        ＋
      </button>

      {/* Nikt nie czeka — nie ma elementu, nie ma pustego miejsca po nim. */}
      {waitingIn === null ? null : (
        <p data-slots-waiting className="ml-auto truncate text-muted">
          {waitingSentence(busy, atOnce, waitingIn)}
        </p>
      )}
    </div>
  );
}
