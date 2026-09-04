/* „Nothing is running." wolno powiedzieć wyłącznie wtedy, kiedy powiedział to bieg.
 *
 * 2026-08-23 — ZE ZRZUTU WŁAŚCICIELA, cztery wiersze pod rząd w jednym terminalu:
 *
 *   Loadout   A run is already going, and Loadout leads one at a time so that Stop always
 *             reaches the one that is working. Press Stop first, then ask again.
 *   Loadout ❯ /stop
 *   Loadout   Nothing is running.
 *   Loadout ❯ /run deep-reaserch …
 *   Loadout   A run is already going… Press Stop first, then ask again.
 *   Loadout ❯ /stop
 *   Loadout   Nothing is running.
 *
 * Bieg pracował przez cały ten czas. Odmowa nazywa następny ruch, a tego ruchu nie było.
 *
 * PRZYCZYNA: zdanie „nic nie biegnie" mówiło OKNO, z własnej pamięci (`workflow !== ''` w sesji
 * zakresu). Ta pamięć jest ulotna — gubi ją przeładowanie strony — a zapadka biegu jest JEDNA
 * NA APLIKACJĘ i mieszka w Ruście. Dwie odpowiedzi na jedno pytanie mogły się rozjechać i
 * rozjechały się dokładnie tam, gdzie boli (niezmiennik 13).
 *
 * CZEGO TEN PLIK NIE SĄDZI: samego wiersza. To repo nie ma jsdom, więc kryterium wymagające
 * wysłania formularza nigdy by nie świeciło. Sądzona jest REGUŁA, która za tym stoi i która ma
 * dać się osądzić bez okna — dokładnie tak, jak `../addressee.ts`. Że Rust odpowiada na to
 * pytanie zamiast wieszać, dowodzi `close_stops_the_run.rs` na prawdziwym uchwycie biegu.
 *
 * SŁABA WERSJA: „przy `false` pada zdanie". Przechodzi ją implementacja, która mówi je ZAWSZE —
 * a wtedy każde udane zatrzymanie kończy się zdaniem, że nie było czego zatrzymywać. Dlatego
 * drugi punkt żąda ciszy przy `true`.
 */
import { describe, expect, it } from 'vitest';

import { NOTHING_RUNS, whatStopSaid } from './entry';

describe('what Stop says comes from the run, not from what the window remembers', () => {
  it('says nothing was running only when the run itself said so', () => {
    expect(
      whatStopSaid(false),
      'Stop came back saying it found nothing to stop, and the person was told nothing at all. ' +
        'Silence reads like a broken key on a control they just pressed',
    ).toBe(NOTHING_RUNS);
  });

  it('stays quiet when a run really was stopped', () => {
    expect(
      whatStopSaid(true),
      'a run was stopped and Loadout answered "Nothing is running." Said after a successful ' +
        'Stop, that sentence is how the window taught the person to distrust it: the same words ' +
        'appeared over a working run, which is the report this fix comes from',
    ).toBeNull();
  });

  /* 2026-09 (Z-35) — ZDANIE NAZYWA FOLDER, odkąd Stop jest folderem adresowany. `false` znaczy
   * od tego dnia „nic nie biegnie TUTAJ", a nie „w tej aplikacji": przy dwóch kartach zdanie bez
   * nazwy jest po prostu nieprawdziwe — człowiek widzi obok pracującego agenta i czyta pod ręką,
   * że nic nie idzie. To jest to samo, co złapało zgłoszenie wyżej, o jedną kartę dalej. */
  /* NAZWA FOLDERU W TEJ FIKSTURZE TO `invoices`, NIE `ledger`, i to nie jest gust: „ledger" jest
   * po stronie tego repo ŻARGONEM (FOUNDATIONS §2.2 — mówimy „activity"), więc `quick-vocabulary`
   * czyta je jako zdanie dla człowieka i słusznie świeci na czerwono. Sprawdzenie nie odróżnia
   * nazwy katalogu w fikstury od słowa na ekranie i nie ma jak — więc zmienia się fikstura. */
  it('names the folder the person is standing in', () => {
    expect(
      whatStopSaid(false, 'invoices'),
      'Stop pressed on the invoices card found nothing to stop and answered without saying ' +
        'where. With a run going in the next card that sentence contradicts what the person ' +
        'can see',
    ).toBe('Nothing is running in invoices.');
  });

  it('falls back to the plain sentence when the window has no name for the card', () => {
    expect(
      whatStopSaid(false, ''),
      'a card the window cannot name has to get the plain sentence. A sentence with a hole in ' +
        'it ("Nothing is running in .") reads as a defect, not as an answer',
    ).toBe(NOTHING_RUNS);
  });

  it('says nothing at all when a run in a named folder really was stopped', () => {
    expect(
      whatStopSaid(true, 'invoices'),
      'the name is part of the sentence, never a reason to say one: a stopped run is answered ' +
        'with silence in every folder',
    ).toBeNull();
  });
});
