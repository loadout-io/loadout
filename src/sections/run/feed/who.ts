/* Kim jest ten, kto mówi — dwie rzeczy, które podpis wiersza musi wiedzieć o linii.
 *
 * DLACZEGO OSOBNY PLIK, a nie dwie funkcje w komponencie. Obie odpowiadają na pytanie o DANE,
 * nie o rysunek, i obie mają dwóch czytelników: wiersz strumienia i — od chwili, w której
 * powstanie — kafelek listy agentów. Funkcja licząca inicjały wpisana w komponent jest drugim
 * miejscem, w którym powstaje ten sam podpis, i rozjeżdża się z pierwszym po cichu
 * (niezmiennik 13).
 *
 * CZEGO TU NIE MA: koloru. Barwę tożsamości przydziela `../rail/colour.ts` i to jest jedyne
 * miejsce, w którym wolno ją przydzielić — kwadrat na kafelku agenta i kwadrat w strumieniu
 * mają być tego samego koloru, bo są tym samym agentem.
 */

/** Kiedy podpisu nie ma czym złożyć. Znak zapytania, nigdy puste miejsce — kwadrat bez treści
 * czyta się jak wiersz, któremu czegoś brakuje, a nie jak agenta bez nazwy. */
const UNKNOWN = '?';

/**
 * Dwie litery, którymi podpisuje się agent w kwadracie tożsamości.
 *
 * DWA SŁOWA DAJĄ PO LITERZE, jedno słowo daje dwie pierwsze. To nie jest ozdoba: `Second reader`
 * i `Scout` obcięte do dwóch pierwszych liter są oba `Se`/`Sc` — a przy `Scout` i `Second reader`
 * w jednym biegu (czyli w każdym biegu z tego katalogu workflow) dwa kwadraty przestają
 * rozróżniać dwóch agentów, czyli robią dokładnie to, przeciwko czemu istnieją.
 *
 * Wielkość liter jest narzucona, nie przepisana z nazwy: `Sc`, nigdy `SC` ani `sc`. Makieta
 * (`.msg .sig`) rysuje je właśnie tak, a nazwa agenta bywa zapisana, jak komu wygodnie.
 */
export function initialsOf(agent: string): string {
  const words = agent.split(/[\s_\-/]+/u).filter((word) => word.length > 0);
  const first = words[0];
  if (first === undefined) return UNKNOWN;
  const second = words[1];
  const tail = second === undefined ? first.slice(1, 2) : second.slice(0, 1);
  return first.slice(0, 1).toUpperCase() + tail.toLowerCase();
}

/** Dwie cyfry, zawsze — `9` w godzinie przesuwa cały wiersz o znak. */
function two(value: number): string {
  return String(value).padStart(2, '0');
}

/**
 * Zegar wiersza — `hh:mm:ss` czasu LOKALNEGO, z chwili, w której linia napłynęła.
 *
 * CZAS ZDARZENIA, NIGDY CZAS RENDERU. `Date.now()` w komponencie dałby liczbę, która zmienia
 * się przy każdym przerysowaniu i nie mówi nic o biegu (niezmiennik 17) — a różnica między
 * „agent milczał cztery minuty" a „przerysowało się cztery minuty temu" jest całą treścią
 * tej kolumny.
 *
 * LOKALNY, nie UTC, i to jest świadome: katalogi biegów mają w nazwie UTC, ale zegar na ekranie
 * odpowiada na pytanie „kiedy to się stało DLA MNIE". Składane ręcznie, a nie przez
 * `toLocaleTimeString`, bo tamto oddaje różny napis w różnych ustawieniach systemu — a to jest
 * wartość maszynowa w kolumnie o stałej szerokości, nie zdanie do przetłumaczenia.
 */
export function clockOf(at: number): string {
  const when = new Date(at);
  return two(when.getHours()) + ':' + two(when.getMinutes()) + ':' + two(when.getSeconds());
}
