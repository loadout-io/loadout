/* Kto odezwał się w tym strumieniu i jak zawęzić go do jednego z nich.
 *
 * PO CO TO ISTNIEJE. Bieg tego produktu prowadzi kilku agentów naraz — to jest cała jego
 * przesłanka — a ich zdania wpadają do jednej kolumny w kolejności czasu. Wątku jednego z nich
 * nie da się z niej wyczytać okiem: linie Buildera stoją co trzecia, między linią Scouta
 * a wyjściem sprawdzeń. Makieta `polecenie.html` odpowiada na to rzędem chipów w nagłówku
 * (`.sthead .fchip`), a te dwie funkcje są całą polityką, która za nimi stoi.
 *
 * CHIPY LICZONE Z HISTORII, NIGDY WPISANE. Lista `All / Scout / Builder / Needle` z makiety jest
 * przykładem jednego biegu, nie zbiorem: chip agenta, który się nie odezwał, obiecuje wątek,
 * którego nie ma (niezmiennik 17), a naciśnięty daje pusty strumień — czyli wygląda jak
 * zepsuta aplikacja.
 *
 * POLITYKA STOI TU, A NIE W KOMPONENCIE, bo to repo nie ma jsdom: funkcja w środku widoku jest
 * kodem, którego kryterium nie umie dotknąć inaczej niż przez markup jednego przypadku.
 */
import { authorityOf } from '../rail/say';
import type { HistoryRow } from './model';

/**
 * Chip, który niczego nie zawęża.
 *
 * Napis, nie `null`, i to nie jest wygoda: „pokaż wszystko" jest JEDNYM z chipów tego rzędu
 * i ma dokładnie tak samo pokazywać, czy jest w mocy. Dwa kształty na jedno pole (`null` obok
 * nazw) dałyby gałąź, przez którą nie da się przejść inaczej niż przez pomyłkę.
 */
export const EVERYONE = 'All';

/**
 * Podpisy, które padły w tym strumieniu, w kolejności pierwszego wiersza.
 *
 * KOLEJNOŚĆ POJAWIENIA SIĘ, nie alfabetyczna: rząd chipów ma się nie przestawiać pod palcem,
 * kiedy do biegu dołączy agent o nazwie na `A`. Pierwszy, który się odezwał, zostaje pierwszy
 * do końca biegu.
 *
 * WYŁĄCZNIE ZDANIA AGENTÓW. Wiersz `told` niesie w polu `agent` ADRESATA (bo tak niesie go każdy
 * inny wiersz tego kroku), więc liczony jako mówca dawałby chip komuś, kto jeszcze nic nie
 * powiedział — a `handoff`, `problem` czy `ran` to zdania Loadouta o agencie, nie jego własne.
 * „Kto mówi" bierzemy z `../rail/say.ts`, czyli z jedynego miejsca, w którym ta polityka mieszka.
 */
export function speakersIn(history: readonly HistoryRow[]): readonly string[] {
  const seen: string[] = [];
  for (const row of history) {
    if (authorityOf(row.kind) !== 'agent') continue;
    if (row.agent.length === 0 || seen.includes(row.agent)) continue;
    seen.push(row.agent);
  }
  return seen;
}

/**
 * Ten strumień zawężony do jednego podpisu; `EVERYONE` nie zawęża niczego.
 *
 * PORÓWNANIE IDZIE PO POLU `agent`, a nie po tym, kto zdanie wypowiedział, i to jest wybór:
 * zawężenie do Scouta ma pokazać CAŁY jego wątek — także to, co Loadout powiedział o nim
 * (`Read 18 files`) i to, co Ty napisałeś DO niego. Wersja porównująca autorstwo zostawiłaby
 * z wątku jednego agenta same jego akapity, bez ani jednej rzeczy, którą naprawdę zrobił.
 *
 * NIE ZMIENIA KOLEJNOŚCI I NIE ZMIENIA WIERSZY. Filtr, który przy okazji cokolwiek przelicza,
 * byłby drugim miejscem, w którym powstaje wiersz historii (niezmiennik 13).
 */
export function onlyFrom(history: readonly HistoryRow[], showing: string): readonly HistoryRow[] {
  if (showing === EVERYONE) return history;
  return history.filter((row) => row.agent === showing);
}
