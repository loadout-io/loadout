/* Ujawnienie: jedna linia zamiast ośmiu wierszy, i ta linia MÓWI, ile ich jest.
 *
 * 2026-08-31 — POWSTAŁO, BO PANEL KROKU MONTOWAŁ 21 KONTROLEK STAŁYCH, a realistycznie 34
 * (sześć umiejętności, pięć pozycji do pożyczenia) w kolumnie 330 px. To jest ~60 elementów
 * niosących tekst — czyli CAŁY sufit widoku z `docs/ARCHITECTURE.md` §7 — zjedzony przez jedną
 * kolumnę. Żaden z tych wierszy nie doszedł błędnie: przyrastały po jednym, każdy z osobna do
 * obrony, i dokładnie tak wygląda ta awaria za każdym razem.
 *
 * DLACZEGO `<details>`, A NIE PRZYCISK ZE STANEM — ten sam powód, co w `sections/skills/
 * review-card.tsx`: rozwijanie jest zachowaniem przeglądarki, więc nie potrzebuje ani handlera,
 * ani pola stanu wyżej (niezmiennik 16: kontrolka bez handlera nie wchodzi do repo). Treść
 * siedzi w drzewie od pierwszego renderu; to, czy człowiek ją widzi, rozstrzyga przeglądarka.
 *
 * DLACZEGO ZWINIĘTE NAZYWA LICZBĘ. Ujawnienie bez liczby jest gorsze niż wiersze, które chowa:
 * człowiek nie ma jak odróżnić pustego od trzymającego osiem ustawień, więc otwiera je za
 * każdym razem — czyli płaci klik za tę samą ścianę. Druga liczba, „ile zmieniono", jest tym
 * samym faktem, który do dziś nosił znacznik przy „Who does this". Stoi TUTAJ, a nie w obu
 * miejscach naraz: dwa żywe regiony na jeden fakt to niezmiennik 13, a rozjazd między nimi
 * wygląda wiarygodnie z obu stron.
 */
import type { ReactElement, ReactNode } from 'react';

export interface MoreSettingsProps {
  /** Ile wierszy naprawdę stoi w środku. Liczy je wołający, bo to on wie, które powstały. */
  inside: number;
  /** Ile z nich różni się od agenta. Zero nie ma prawa być napisane. */
  changed: number;
  children: ReactNode;
}

/** Zdanie zwiniętego ujawnienia.
 *
 * Osobne i eksportowane, bo jest jedyną rzeczą, którą to ujawnienie mówi, kiedy jest zamknięte —
 * a wtedy jest zamknięte przy każdym pierwszym otwarciu panelu. „0 changed" nie powstaje: zdanie
 * o tym, że nic się nie zmieniło, stałoby przy każdym nietkniętym kroku w całym workflow. */
export function moreSettingsSays(inside: number, changed: number): string {
  const things = `${String(inside)} more setting${inside === 1 ? '' : 's'}`;
  return changed === 0 ? things : `${things}, ${String(changed)} changed`;
}

export function MoreSettings({ inside, changed, children }: MoreSettingsProps): ReactElement {
  return (
    <details data-more-settings className="rounded-md border border-line p-2">
      {/* `.label` niesie stopień i barwę drugoplanową; kursor mówi, że to jest do kliknięcia,
          bo `<summary>` sam z siebie zostaje strzałką z tekstem. */}
      <summary className="label cursor-pointer">{moreSettingsSays(inside, changed)}</summary>
      <div className="stack pt-2" data-gap="3">
        {children}
      </div>
    </details>
  );
}
