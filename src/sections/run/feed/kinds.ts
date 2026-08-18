/* Rejestr rodzajów linii — zamknięty na czternastu [T2 §7.2].
 *
 * „Zamknięty" jest tu słowem operacyjnym. Rejestr, obok którego stoi gałąź `default`
 * drukująca to, czego nie rozpoznał, nie jest zamknięty: pierwszy nowy typ zdarzenia
 * od vendora wyświetla wtedy surowy enum z drutu na ekranie użytkownika (niezmiennik 14).
 * Dlatego rodzaj spoza tego zbioru nie ma renderera — jest PORZUCANY w modelu.
 *
 * Wpis niesie dwie rzeczy i ani jednej więcej:
 *   `route`     dokąd wiersz idzie. Dokładnie jeden rodzaj (`thinking`) trafia do strefy
 *               TERAZ, bo `Thinking…` to status, nie linia [T2 §7.3 reguła 5]; reszta
 *               dopisuje się do historii.
 *   `expanded`  czy jest rozwinięty domyślnie [T2 §7.3 reguła 2].
 *
 * Czego tu NIE ma: etykiety. Etykieta zależy od licznika (`Read 3 files`), więc jest funkcją,
 * nie stałą, i mieszka w modelu razem ze sklejaniem.
 */
import type { Line } from '../../../ipc/types';

/** Czternaście rodzajów, wyprowadzonych z lustra drutu — nie przepisanych obok niego. */
export type Kind = Line['kind'];

/** Dokąd idzie wiersz tego rodzaju. */
export type Route = 'now' | 'history';

export interface KindEntry {
  readonly route: Route;
  /** Rozwinięty domyślnie? Osiem tak, sześć nie [T2 §7.3 reguła 2]. */
  readonly expanded: boolean;
}

export type Registry = Readonly<Record<Kind, KindEntry>>;

/*
 * Rejestr. Typ `Record<Kind, KindEntry>` jest tu całą obroną przed luką: `Kind` pochodzi
 * z lustra drutu, więc rodzaj dodany po stronie Rusta przestaje się TU kompilować, zamiast
 * po cichu wypaść z widoku jako wiersz, którego nikt nie umie narysować.
 *
 * Kolejność wpisów jest kolejnością z T2 §7.2, nie alfabetyczna: tak czyta się je razem
 * z tabelą, z której pochodzą. Kryterium porównuje ZBIÓR kluczy, więc kolejność jest
 * czytelnością, nie kontraktem.
 *
 * `Object.freeze`, bo „czytany, nigdy modyfikowany" ma być własnością obiektu, a nie prośbą
 * w komentarzu: rejestr jest jeden na cały moduł i wskaźnik na niego dostaje każdy, kto woła
 * `kinds()`. Jedno `registry.read.expanded = true` w cudzym kodzie zmieniłoby domyślny stan
 * widoku wszystkim naraz i nie zostawiłoby po sobie ani jednej linii w diffie tego pliku.
 */
const REGISTRY: Registry = Object.freeze({
  /* ── rozwinięte domyślnie: proza, pytania, błędy, struktura ── */
  run: { route: 'history', expanded: true },
  step: { route: 'history', expanded: true },
  agent: { route: 'history', expanded: true },
  note: { route: 'history', expanded: true },
  /* Zdanie czlowieka wchodzi do historii i jest widoczne od razu. Zwiniete byloby jedynym
   * wierszem, ktory czlowiek musi rozwinac, zeby przeczytac to, co sam napisal. */
  told: { route: 'history', expanded: true },
  asked: { route: 'history', expanded: true },
  handoff: { route: 'history', expanded: true },
  problem: { route: 'history', expanded: true },
  done: { route: 'history', expanded: true },

  /* ── zwinięte: mechanika. Rozwija ją człowiek albo niepowodzenie [T2 §7.3 reguła 3] ── */
  read: { route: 'history', expanded: false },
  search: { route: 'history', expanded: false },
  edit: { route: 'history', expanded: false },
  ran: { route: 'history', expanded: false },
  memory: { route: 'history', expanded: false },

  /* ── dwa rodzaje, które nie wchodzą do historii [T2 §7.3 reguła 5] ── */
  thinking: { route: 'now', expanded: false },
  /* Stan kroku jest FAKTEM O TERAZ, nie zdarzeniem do przeczytania: przestawia blok paska
   * loadoutu i chip na kafelku agenta. Wpisany do historii byłby czterema wierszami na krok
   * („ready", „running", „succeeded", i to samo dla kopii), czyli ścianą, którą teza z DESIGN §1
   * istnieje żeby skasować: „widok pracy nie przyrasta, aktualizuje się w miejscu". */
  stepState: { route: 'now', expanded: false },
});

/** Rejestr. Czytany, nigdy modyfikowany. */
export function kinds(): Registry {
  return REGISTRY;
}
