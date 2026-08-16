/* Ekran sekcji `run`. Powłoka znajduje go po ŚCIEŻCE — `src/sections/<id>/index.tsx` — więc
 * ten plik jest całym wpisem do rejestru i nie ma żadnego drugiego miejsca, w którym trzeba by
 * go zadeklarować (T-25, HARNESS-QUEUE.md Q-5).
 *
 * Cienki z premedytacją: składa pasek loadoutu i widok pracy, i nic poza tym. Druga
 * implementacja czegokolwiek z `feed/` albo `strip/` tutaj byłaby drugim miejscem prawdy
 * o tej samej rzeczy (niezmiennik 23).
 *
 * Zaślepka fazy kontraktu: pusty fragment. Implementacja wstawia tu trzy oznaczone regiony —
 * pasek, historię i strefę TERAZ — i to ich obecność w wyrenderowanym dokumencie jest dowodem,
 * że mechanizm montowania sekcji naprawdę działa, a nie tylko przechodzi własny test.
 */
import type { ReactElement } from 'react';

export default function Run(): ReactElement {
  return <></>;
}
