/* Pasek kart: karty, jedno zdanie o czekaniu na miejsce i przycisk otwarcia folderu.
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
 * # Stan tego pliku: SZKIELET (2026-08-16)
 *
 * Pusty fragment: zdania nie ma, więc kryterium 5 pada na jego obecności, a przechodzi tę
 * połowę, która pyta o jego zniknięcie. Tak ma wyglądać ta warstwa.
 */
import type { ReactElement } from 'react';
import type { WorkspaceTab } from '../../../state/workspaces';

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
  /** Wymagany: `＋`, czyli menu wyboru folderu. */
  readonly onOpenFolder: () => void;
}

export function TabBar(_props: TabBarProps): ReactElement {
  return <></>;
}
