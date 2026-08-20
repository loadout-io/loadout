/* Do kogo idzie zdanie bez ukośnika — i to jest ZMIANA POLITYKI, nie porządki.
 *
 * CO BYŁO. `sayIt` w `./index.tsx` rozstrzygało jednym warunkiem: ktoś pracuje → zdanie idzie
 * do NIEGO, nikt nie pracuje → do lidera. Skutek jest tą wadą, którą zgłosił właściciel
 * 2026-08-20: „proza w trakcie biegu znika z rozmowy z liderem, bo leci do pracującego agenta".
 * Lider znikał dokładnie wtedy, kiedy jest najbardziej potrzebny — w środku biegu, kiedy człowiek
 * chce zapytać, co się właściwie dzieje, i nie chce tego pytania wysyłać agentowi, który pisze kod.
 *
 * CO JEST. Zdanie bez ukośnika idzie do lidera ZAWSZE, a do agenta wyłącznie wtedy, gdy człowiek
 * zaadresował je jego nazwą na początku linii. Konwencja już istnieje i nie jest wymyślona tutaj:
 * tak każe adresować Rust, kiedy pracuje kilku (`RunError::SeveralAreWorking`), więc to samo
 * słowo znaczy to samo po obu stronach granicy.
 *
 * DOPASOWANIE NA CAŁYM SŁOWIE, nigdy na prefiksie. `Plan` nie adresuje kroku `Planner`: adres
 * zdejmowany z treści zmienia zdanie, które pojedzie dalej, więc pomyłka nie jest tu „wysłaniem
 * do złego agenta" — jest wysłaniem do złego agenta ZDANIA, z którego zniknęło pierwsze słowo.
 *
 * DLACZEGO CZYSTY MODUŁ OBOK EKRANU. To repo nie ma jsdom, więc Enter jest dla kryterium
 * nieosiągalny — polityka zamknięta w `sayIt` byłaby kodem, którego nie umie dotknąć żadne
 * kryterium (ten sam powód stoi przy `./run-command.ts`). Ekran przewozi tekst i woła krawędź.
 */

/**
 * Komu doręczyć to zdanie i z jaką treścią.
 *
 * Zamknięty kształt dwóch wariantów, a nie `agent: string | null`: `null` znaczy już coś innego
 * o jedną warstwę niżej — `sayToAgent(text, null)` mówi „ten jeden, który pracuje" (`./io.ts`).
 * Dwa różne `null` w jednej ścieżce to gałąź, w którą wchodzi się przez pomyłkę.
 */
export type Addressee =
  | { readonly to: 'lead'; readonly text: string }
  | { readonly to: 'agent'; readonly agent: string; readonly text: string };

/**
 * Adresat zdania bez ukośnika.
 *
 * @param typed cała linia, jak ją napisał człowiek.
 * @param working nazwy kroków, które PRACUJĄ — ten sam zbiór, z którego bieg buduje swoją
 *   odmowę po stronie Rusta (`RunControl::step_can_hear`). Krok, który nie pracuje, nie jest
 *   adresem: jego nazwa na początku linii jest wtedy zwykłym słowem i jedzie do lidera razem
 *   z resztą zdania, bo zdanie wysłane komuś, kto nie słucha, przepada bez śladu.
 */
export function addresseeOf(typed: string, working: readonly string[]): Addressee {
  const text = typed.trim();
  /* PIERWSZE SŁOWO, nie prefiks linii, i to jest cała obrona przed pomyłką z nagłówka:
   * `Plan` nie adresuje kroku `Planner`. Dopasowanie po prefiksie wysłałoby „Plan the work"
   * do Plannera ZE ZDJĘTYM pierwszym słowem — zły czytelnik i zdanie, które nie mówi już
   * tego, co mówiło. */
  const first = text.split(/\s+/)[0] ?? '';
  const addressed = working.find((name) => name === first);

  /* KROK, KTÓRY NIE PRACUJE, NIE JEST ADRESEM. Jego nazwa jest wtedy zwykłym słowem i jedzie
   * do lidera RAZEM z resztą zdania: zdjęcie jej po drodze zmieniłoby zdanie, które człowiek
   * napisał, i nic na ekranie by o tym nie powiedziało. */
  if (addressed === undefined) return { to: 'lead', text };

  return {
    to: 'agent',
    agent: addressed,
    /* Adres SCHODZI z treści: krok, do którego dojdzie „Forge use tabs", jest adresowany
     * własną nazwą — czyta się to jak ktoś, kto cytuje mu ją z powrotem. */
    text: text.slice(addressed.length).trimStart(),
  };
}
