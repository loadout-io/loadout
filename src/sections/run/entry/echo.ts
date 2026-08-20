/* Wiersz, który okno dopisuje samo: to, co wysłałeś, i to, co ten wiersz odpowiedział.
 *
 * PO CO TO ISTNIEJE. Zgłoszenie właściciela 2026-08-20: komendy nie zostawiają po sobie ani
 * jednego wiersza w strumieniu. Cicha porażka, przed którą stoi ten plik: terminal, w którym
 * wpisana komenda nie zostawia śladu, jest NIEODRÓŻNIALNY od terminala, który tej komendy nie
 * przyjął. Właściciel miał dokładnie tę wątpliwość 2026-08-19 przy prozie („a może odpisuje on,
 * ale na pewno nie widać moich wiadomości") i wtedy powstał `Line::Told` po stronie Rusta.
 * Komendy zostały z tą samą wadą, bo drut ich nie widzi nigdy: `/run`, `/open`, `/stop`
 * i odpowiedzi samego wiersza (`NOT_KNOWN`, `NOTHING_RUNS`, odmowy) obsługuje okno.
 *
 * CZEGO TEN PLIK NIE ROBI: nie dubluje prozy. Zdanie bez ukośnika ma nośnik na drucie od
 * 2026-08-19 — `Line::Told` wystawia `commands/chat.rs` i `commands/run.rs`, a widok podpisuje
 * je `You →` (`../feed/line.tsx`). Dwa wiersze o jednym zdaniu to dwa miejsca prawdy
 * (niezmiennik 13), więc [`echoOf`] oddaje wtedy `null`.
 *
 * DLACZEGO IDENTYFIKATOR JEST UJEMNY. Stempel powstaje na granicy i osobno dla każdej pompy:
 * `../io.ts` liczy od 1 w `start()` i od 1 w `openChat()`, więc dodatnie numery już dziś potrafią
 * się powtórzyć w jednym oknie. Wiersz składany tutaj wchodzi do tej samej historii, więc
 * potrzebuje przestrzeni numerów, której żadna z tych dwóch pomp nie tknie — a przy okazji
 * NIESIE swoje pochodzenie: numer ujemny nie ma prawa udawać zdarzenia biegu (niezmiennik 4).
 * Tego wiersza nie ma w `run.json` i nie przeżyje przeładowania okna.
 */
import type { Line } from '../../../ipc/types';
import type { Stamped } from '../../../state/run';

/**
 * Wiersz dopisany przez OKNO — linia strumienia, która zawsze niesie zdanie.
 *
 * `Extract`, a nie konkretny rodzaj: kryterium sprawdza podpis WOŁAJĄC `authorityOf(kind)`,
 * więc wybór rodzaju należy do implementacji, a nie do typu. Odsiane są dokładnie te dwa
 * rodzaje, które zdania nie niosą i nie wchodzą do historii (`thinking`, `stepState`) —
 * wiersz bez tekstu nie jest echem niczego.
 */
export type WindowLine = Extract<Line, { text: string }> & Stamped;

/*
 * Podkreślenia przy argumentach schodzą razem z `throw`: szkielet ma pozwolić kryterium
 * PADNĄĆ na asercji, a `noUnusedParameters` nie przepuszcza nazwy, której nikt nie czyta.
 */

/**
 * Wiersz dla linii, którą człowiek właśnie wysłał — albo `null`, kiedy okno nie ma czego
 * dopisywać.
 *
 * Rozstrzyga UKOŚNIK i tylko on: komendy oraz literówki w komendach są niewidoczne dla drutu,
 * więc ich jedynym śladem jest to, co dopisze okno. Proza wraca z drutu jako `told`.
 */
export function echoOf(_typed: string): WindowLine | null {
  throw new Error('not implemented');
}

/**
 * Wiersz dla tego, co odpowiedział sam wiersz wejścia — nieznana komenda, „nic nie biegnie",
 * odmowa startu.
 *
 * TEN SAM KSZTAŁT, co echo linii, i to jest cała treść przeprowadzki: rozmowa z Loadoutem jest
 * JEDNĄ historią, a nie połową pod polem. Do 2026-08-20 te zdania lądowały w `data-entry-said`,
 * czyli w drugim, znikającym miejscu na to samo — a przy trzech zdaniach z rzędu widać było
 * ostatnie.
 */
export function saidOf(_said: string): WindowLine {
  throw new Error('not implemented');
}
