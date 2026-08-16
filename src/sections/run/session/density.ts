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
import type { TranscriptLine } from './filter';
import type { Section } from './layout';

/**
 * Ile elementów niosących tekst stoi za jednym wierszem transkryptu.
 *
 * Zwinięty wiersz to JEDEN element, choćby stał za sześcioma odczytami — i o to w suficie
 * chodzi: zwijanie jest tym, co utrzymuje ekran pod limitem, więc licznik, który tego nie
 * widzi, mierzy coś innego niż to, czym ekran jest.
 *
 * Rozwinięty oddaje to, co skleił, więc liczy się tyloma, iloma naprawdę jest — a wiersz
 * niepowodzenia dokłada jeszcze swoje wyjście, bo te dwadzieścia linii to tekst na ekranie,
 * nie metadane. Ta jedna gałąź jest całym powodem, dla którego ten plik cokolwiek mierzy:
 * bez niej licznik nie umiałby się przewrócić, a licznik, który nie umie się przewrócić,
 * stoi w miejscu pomiaru i zajmuje je (niezmiennik 19).
 */
function lineNodes(line: TranscriptLine): number {
  if (!line.expanded) return 1;
  return line.ids.length + line.output.length;
}

/** Ile elementów niosących tekst ma ten ekran. Limit z tabeli: 60. */
export function countTextNodes(sections: readonly Section[]): number {
  let count = 0;
  for (const section of sections) {
    count += 1; // nagłówek sekcji
    if (section.empty !== null) count += 1; // zdanie pustego stanu
    /* Dwa na wiersz faktów: etykieta i wartość. Obie są widoczne, obie zajmują miejsce
     * i obie rosną razem z każdym wierszem, który ktoś tu dopisze. */
    count += section.rows.length * 2;
    for (const line of section.lines) count += lineNodes(line);
  }
  return count;
}

/**
 * Ile oznaczonych regionów ma ten ekran. Limit z tabeli: 8.
 *
 * Region to sekcja — `given`, `produced`, `transcript`. Nagłówka ekranu (imię agenta, rola,
 * przyciski) ta liczba nie obejmuje i nie ma udawać, że obejmuje: nie ma go w tym modelu,
 * a licznik zgadujący rzeczy, których nie widzi, jest gorszy od licznika o znanym zasięgu.
 */
export function countRegions(sections: readonly Section[]): number {
  return sections.length;
}
