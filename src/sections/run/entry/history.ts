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
  /** Wysłane linie, NAJSTARSZA pierwsza — czyli w kolejności, w jakiej je napisano. */
  const sent: string[] = [];

  /**
   * Jak głęboko stoi chodzenie: `0` znaczy „w szkicu", `1` najmłodsza linia, `2` przedostatnia.
   *
   * Głębokość, nie indeks tablicy, i to jest cała oszczędność tego pliku: najstarsza linia
   * wypada z głowy [`sent`] przy sufcie, więc indeks liczony od zera przesuwałby się pod
   * chodzeniem za każdym `remember`, a głębokość liczona od końca nie przesuwa się nigdy.
   */
  let deep = 0;

  /**
   * Szkic — to, co stało w polu, zanim człowiek pierwszy raz sięgnął wstecz.
   *
   * Powód, dla którego to pole w ogóle istnieje, stoi w nagłówku pliku: bez niego krok naprzód
   * poniżej najmłodszej linii czyści pole, czyli po cichu kasuje zdanie, które ktoś pisał.
   */
  let held = '';

  return {
    remember(line: string): void {
      /* Chodzenie wraca na początek przy KAŻDEJ wysłanej linii, razem ze szkicem: pole jest
       * już puste, a głębokość z poprzedniego chodzenia opisywałaby historię o jeden wpis
       * krótszą, niż ta, po której miałaby chodzić. */
      deep = 0;
      held = '';
      /* DWIE IDENTYCZNE POD RZĄD ZAJMUJĄ JEDEN WPIS — porównujemy z ostatnią, nie z całą
       * historią: `/stop` wysłane teraz i `/stop` wysłane pół godziny temu to dwie różne
       * rzeczy, które człowiek zrobił, i chodzenie ma minąć obie. */
      if (sent[sent.length - 1] === line) return;
      sent.push(line);
      /* Wypada NAJSTARSZA. `splice` z policzoną liczbą, nie `shift` w pętli: sufit da się
       * przekroczyć tylko o jeden na raz, ale kod, który to zakłada, jest kodem, który
       * przestaje być prawdziwy w dniu, w którym ktoś zasieje historię hurtem. */
      if (sent.length > HISTORY_LIMIT) sent.splice(0, sent.length - HISTORY_LIMIT);
    },

    back(draft: string): string | null {
      /* Pusta historia i dno historii dają tę samą odpowiedź, i to jest poprawne: w obu
       * wypadkach nie ma czego oddać, więc pole ma zostać nietknięte. `null`, nigdy pusty
       * napis — pusty napis WCHODZI do pola i kasuje to, co w nim stało. */
      if (deep >= sent.length) return null;
      /* SZKIC ZAPAMIĘTUJEMY WYŁĄCZNIE PRZY PIERWSZYM KROKU. Przy drugim w polu stoi już cudza
       * linia, więc zapisanie jej tutaj zgubiłoby to jedno zdanie, którego ten argument
       * ma pilnować. */
      if (deep === 0) held = draft;
      deep += 1;
      return sent[sent.length - deep] ?? null;
    },

    forward(): string | null {
      /* W szkicu nie ma dokąd iść naprzód. Oddanie szkicu drugi raz wstawiłoby go w pole,
       * w którym człowiek zdążył już napisać coś innego. */
      if (deep === 0) return null;
      deep -= 1;
      if (deep === 0) return held;
      return sent[sent.length - deep] ?? null;
    },
  };
}
