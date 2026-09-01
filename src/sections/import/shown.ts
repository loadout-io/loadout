/* Które pozycje tabela importu pokazuje i co mówi, kiedy filtr schował je WSZYSTKIE.
 *
 * PO CO OSOBNY MODUŁ. Stan „pusta tabela, bo filtr" powstaje dopiero po dwóch kliknięciach
 * (pigułka, potem Scan), a w tym repo nie ma jsdom i `renderToStaticMarkup` nie odpala
 * `onClick`. Niezmiennik 29 dopuszcza czysty moduł jako dowód TREŚCI zdania — więc reguła
 * i zdanie stoją tutaj, gdzie da się je zapytać wprost, a ekran je tylko wypisuje.
 *
 * ZMIERZONE 2026-08-31: `scan()` nie ruszał `inventoryView`, więc skan puszczony przy filtrze
 * „Needs attention" nad projektem z samymi gotowymi pozycjami renderował `<tbody>` PUSTY, bez
 * ani jednego zdania. Liczniki nad tabelą mówiły wtedy „17 Skills", a tabela pod nimi była
 * pusta — jedyne, co się z tego czyta, to że skan się zepsuł.
 */

/** Trzy pigułki nad tabelą i ani jednej więcej. */
export type InventoryView = 'all' | 'ready' | 'attention';

/** Napis na pigułce. Zdanie o schowanych pozycjach MUSI nazwać filtr tym samym słowem, które
 *  stoi na kontrolce — inaczej człowiek ma trzy pigułki do zgadywania (niezmiennik 13). */
export const FILTER_NAMES: Readonly<Record<InventoryView, string>> = {
  all: 'All',
  ready: 'Ready',
  attention: 'Needs attention',
};

/** Napis na kontrolce, która zdejmuje filtr jednym ruchem. */
export const SHOW_ALL = 'Show all items';

/**
 * Pozycje, które zostają po filtrze.
 *
 * Ogólne po `T`, bo tę samą regułę stosuje wektor typowany (`ImportItem`) i stary wektor
 * (`SourceItem` + zgodność z raportu). Dwie kopie warunku to dwa znaczenia słowa „Ready".
 */
export function keptBy<T>(
  all: readonly T[],
  view: InventoryView,
  isReady: (one: T) => boolean,
): T[] {
  return all.filter((one) => view === 'all' || (view === 'ready' ? isReady(one) : !isReady(one)));
}

/**
 * Czy tabela jest pusta WYŁĄCZNIE dlatego, że filtr schował całą zawartość.
 *
 * Skan, który niczego nie znalazł, ma swoje własne zdanie w stopce („No setup files were found
 * in this project.") i nie wolno go zastąpić zdaniem o filtrze: to są dwie różne rzeczy, a
 * pomylenie ich wysyła człowieka szukać usterki tam, gdzie jej nie ma.
 */
export function hidesEverything(kept: number, all: number, view: InventoryView): boolean {
  return kept === 0 && all > 0 && view !== 'all';
}

/** Zdanie dla tabeli, która pusta nie jest — po prostu nic z niej nie widać. */
export function hiddenSays(all: number, view: InventoryView): string {
  return `${String(all)} item(s) came back from the scan, and the ${FILTER_NAMES[view]} filter hides every one of them.`;
}
