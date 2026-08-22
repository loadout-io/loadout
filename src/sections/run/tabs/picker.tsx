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
 * ZDANIE JEST SKŁADANE Z LICZBY, KTÓRA PRZYSZŁA, a liczba pojedyncza ma własne brzmienie:
 * „1 agents are working" czyta się jak usterka i podważa całą resztę zdania dokładnie w chwili,
 * w której człowiek ma mu zaufać.
 */
import type { ReactElement } from 'react';
import type { PendingClose } from '../../../state/run-tabs';

export interface CloseConfirmProps {
  /** Karta, o której zamknięcie pytamy, razem z liczbą pracujących w niej agentów. */
  readonly pending: PendingClose;
  /** Wymagany: „tak" — anuluj bieg, a potem zamknij kartę. */
  readonly onConfirm: () => void;
  /** Wymagany: „nie" — nie zmieniaj niczego. */
  readonly onDismiss: () => void;
}

/** Ile pracy stoi po drugiej stronie tego kliknięcia — zdanie, nie liczba obok słowa. */
function question(pending: PendingClose): string {
  const working =
    pending.agents === 1 ? '1 agent is working' : String(pending.agents) + ' agents are working';
  return working + ' in ' + pending.name + '. Closing this tab stops that work.';
}

export function CloseConfirm({ pending, onConfirm, onDismiss }: CloseConfirmProps): ReactElement {
  return (
    <div
      role="dialog"
      aria-label={'Close ' + pending.name}
      className="flex flex-col gap-3 rounded-md border border-line bg-overlay p-4"
    >
      <p data-close-confirm className="text-body text-ink">
        {question(pending)}
      </p>

      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={onDismiss}
          className="h-control rounded-sm border border-line px-3 text-ui text-body"
        >
          Keep it open
        </button>
        {/* Accent jest jedynym kolorem interaktywnym w całej aplikacji (DESIGN §3), także tam,
         * gdzie przycisk kończy czyjąś pracę: to jest przycisk podstawowy tego pytania, a nie
         * piąty sens dołożony do koloru ostrzeżenia. */}
        <button
          type="button"
          onClick={onConfirm}
          className="h-control rounded-sm bg-accent px-3 text-ui text-bg"
        >
          Stop and close
        </button>
      </div>
    </div>
  );
}
