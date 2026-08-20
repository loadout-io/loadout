/* Historia wpisanych linii — to, co oddaje strzałka w górę.
 *
 * PO CO TO ISTNIEJE. Zgłoszenie właściciela 2026-08-20, jedna z czterech wad w tym wierszu:
 * „strzałka w górę nie cofa do poprzedniej linii". Wiersz wejścia jest nazwany terminalem
 * i wygląda jak terminal, a jedyny sposób powtórzenia komendy to przepisanie jej z pamięci —
 * przy `/run <workflow> <całe zdanie zadania>` to jest przepisywanie akapitu.
 *
 * DLACZEGO OSOBNY, CZYSTY MODUŁ, A NIE `useState` W KOMPONENCIE. To repo nie ma jsdom, więc
 * naciśnięcia klawisza nie da się odpalić w kryterium (ten sam powód stoi przy `suggestions`
 * i przy `../run-command.ts`). Polityka chodzenia po historii zamknięta w `onKeyDown` byłaby
 * kodem, którego żadne kryterium nie umie dotknąć — a to jest ta rodzina, z której wzięło się
 * siedemnaście kłamiących kontrolek w poprzednim prototypie. Tutaj jest funkcją od stanu do napisu.
 *
 * SZKIC JEST CZĘŚCIĄ TEJ HISTORII, i to jest jedyna nieoczywista rzecz w tym pliku. Człowiek
 * pisze pół zdania, sięga strzałką wstecz po komendę, rozmyśla się i wraca naprzód — i musi
 * dostać SWOJE pół zdania, nie puste pole. Wersja, która przy powrocie czyści pole, kasuje
 * zdanie, które ktoś właśnie pisał, i robi to cicho: nie ma na ekranie żadnego śladu, że coś
 * przepadło. Dlatego szkic wchodzi argumentem do [`History.back`] i wychodzi z [`History.forward`].
 */

/**
 * Ile linii pamiętamy naraz.
 *
 * Sufit, nie „rośnie do końca sesji okna": historia wiersza wejścia jest wygodą, a nie zapisem
 * — zapisem jest strumień (`../feed/model.ts`) i pliki biegu (niezmiennik 4). Sto linii to
 * kilka godzin pracy przy tym wierszu, a wypadnięcie NAJSTARSZEJ jest jedyną utratą, której
 * człowiek nie zauważy: po nią sięga się chodzeniem, a nikt nie chodzi sto kroków wstecz.
 */
export const HISTORY_LIMIT = 100;

/** Chodzenie po tym, co już zostało wysłane — plus szkic, który czeka na powrót. */
export interface History {
  /**
   * Zapamiętuje wysłaną linię i ustawia chodzenie na początek.
   *
   * DWIE IDENTYCZNE LINIE POD RZĄD ZAJMUJĄ JEDEN WPIS. Powtórzenie tej samej komendy jest
   * w terminalu normalną rzeczą (`/stop`, `/stop`), a historia, w której trzeba przejść dwa
   * kroki wstecz, żeby minąć jedno zdanie, przestaje odpowiadać na pytanie „co robiłem
   * przedtem".
   */
  remember(line: string): void;

  /**
   * Krok wstecz. Oddaje linię do wstawienia w pole albo `null`, kiedy nie ma czego oddać.
   *
   * `null`, nigdy pusty napis: „nie ma historii" znaczy, że pole ma zostać NIETKNIĘTE, a pusty
   * napis wstawiony w pole jest skasowaniem tego, co człowiek pisał.
   *
   * @param draft to, co stoi w polu w tej chwili. Zapamiętywane przy PIERWSZYM kroku wstecz
   *   i tylko wtedy: przy drugim kroku w polu stoi już cudza linia, więc zapisanie jej jako
   *   szkicu zgubiłoby to jedno zdanie, którego ten argument ma pilnować.
   */
  back(draft: string): string | null;

  /**
   * Krok naprzód. Poniżej najmłodszej linii oddaje SZKIC — to, co stało w polu, zanim
   * człowiek pierwszy raz sięgnął wstecz.
   */
  forward(): string | null;
}

/**
 * Nowa, pusta historia.
 *
 * Fabryka, nie stan na poziomie modułu: dwa kryteria w jednym pliku dzieliłyby historię
 * i drugie z nich chodziłoby po liniach pierwszego.
 */
export function createHistory(): History {
  return {
    remember(): void {
      throw new Error('not implemented');
    },
    back(): string | null {
      throw new Error('not implemented');
    },
    forward(): string | null {
      throw new Error('not implemented');
    },
  };
}
