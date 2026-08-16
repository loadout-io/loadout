/* Sufit gęstości jest MIERZONY, nie oceniany okiem (niezmiennik 18) [ARCHITECTURE §7].
 *
 * Dwie liczby i jedna zasada: baseline może tylko maleć.
 *
 * Zasada, którą łamie się najłatwiej, dotyczy jednak samego licznika, nie limitu. Licznik
 * chodzący po kluczach najwyższego poziomu modelu zwraca trzy i będzie zielony na zawsze,
 * przy każdym ekranie, jaki ktokolwiek kiedykolwiek napisze. Zielone bez dowodu wykonania
 * jest czerwone (niezmiennik 19), więc licznik, którego nie da się przewrócić rozwinięciem
 * transkryptu, nie jest pomiarem — jest ozdobą, a ozdoba w miejscu pomiaru jest gorsza niż
 * brak pomiaru, bo zajmuje jego miejsce.
 *
 * Co liczymy: KAŻDY element modelu niosący tekst, rekurencyjnie. Nagłówek sekcji, etykietę
 * wiersza, jego wartość, zdanie pustego stanu, chip. Jedna ZWINIĘTA linia transkryptu to
 * jeden element — rozwinięta oddaje to, co skleiła, i liczy się tyloma, iloma naprawdę
 * jest. Na tym stoi test kontrolny i tylko dzięki temu ten plik cokolwiek mierzy.
 */
import type { Section } from './layout';

/** Ile elementów niosących tekst ma ten ekran. Limit z tabeli: 60. */
export function countTextNodes(_sections: readonly Section[]): number {
  throw new Error('not implemented');
}

/** Ile oznaczonych regionów ma ten ekran. Limit z tabeli: 8. */
export function countRegions(_sections: readonly Section[]): number {
  throw new Error('not implemented');
}
