/* Co otwiera się NAD paskiem kart. Dzisiaj: pytanie o zamknięcie karty, w której ktoś pracuje.
 *
 * Dlaczego ten plik nazywa się `picker`, skoro trzyma potwierdzenie: wybór folderu i zamknięcie
 * folderu to ta sama czynność widziana z dwóch stron — decydujesz, w których folderach Loadout
 * ma pracować. Menu wyboru (`＋`, lista ostatnio używanych, „Choose a folder…") wchodzi tutaj
 * razem z komendą, która czyta rejestr po stronie Rusta; kryterium, które sądzi ten plik, jest
 * o zamykaniu, więc zamykanie jest tu pierwsze.
 *
 * PYTANIE MA NAZYWAĆ LICZBĘ. „Are you sure?" nie jest pytaniem — nie mówi, co się stanie,
 * więc jedyną możliwą odpowiedzią jest odruch. „3 agents are working in meetnotes" mówi, ile
 * pracy stoi po drugiej stronie tego kliknięcia, i dopiero na to da się odpowiedzieć.
 *
 * ANULOWANIE JEST WARTOŚCIĄ, NIE BŁĘDEM (niezmiennik 7). Zamknięcie karty z żywym biegiem
 * kończy go jako anulowany po jawnym potwierdzeniu — nigdy po cichu i nigdy jako awarię.
 * Kolejność (najpierw anulowanie, potem zniknięcie karty) mieszka w magazynie kart, bo tam
 * da się ją zmierzyć; ten komponent jest czystą funkcją stanu na markup.
 *
 * Oba handlery są WYMAGANE (niezmiennik 16). Okno dialogowe z martwym „Cancel" jest gorsze niż
 * jego brak: obiecuje wyjście, którego nie ma.
 *
 * # Stan tego pliku: SZKIELET (2026-08-16)
 *
 * Pusty fragment — pytania nie ma, więc nie ma w nim też żadnej liczby.
 */
import type { ReactElement } from 'react';
import type { PendingClose } from '../../../state/workspaces';

export interface CloseConfirmProps {
  /** Karta, o której zamknięcie pytamy, razem z liczbą pracujących w niej agentów. */
  readonly pending: PendingClose;
  /** Wymagany: „tak" — anuluj bieg, a potem zamknij kartę. */
  readonly onConfirm: () => void;
  /** Wymagany: „nie" — nie zmieniaj niczego. */
  readonly onDismiss: () => void;
}

export function CloseConfirm(_props: CloseConfirmProps): ReactElement {
  return <></>;
}
