/* Pasek pauzy biegu: „Waiting for your Claude usage to reset at 3:30 PM."
 *
 * JEDEN pasek na BIEG (niezmiennik 13). „Bieg czeka na odnowienie limitu" to jeden fakt, więc
 * ma jedno żywe miejsce na ekranie: nie jeden pasek na krok i nie dodatkowo kropka przy każdym
 * agencie. poprzedni prototyp pokazywał stan połączenia w sześciu miejscach.
 *
 * Czysta funkcja stanu na markup, bez własnego stanu i bez `invoke()`. Strefa czasowa wchodzi
 * propem z wartością domyślną, żeby dało się ją przypiąć bez ustawiania zmiennej środowiskowej
 * całego procesu.
 *
 * Godzina bierze się z `Intl.DateTimeFormat`, a ICU od wersji 72 stawia przed AM/PM wąską
 * spację nierozdzielającą (U+202F). Zdanie z DESIGN-u ma zwykłą spację, więc sprowadź ją do
 * zwykłej, zanim trafi na ekran — inaczej to samo zdanie w kodzie i w markupie różni się
 * znakiem, którego nie widać.
 *
 * STAN TEGO PLIKU: SZKIELET (2026-08-16) — patrz nagłówek `at-once.tsx`.
 */
import type { ReactElement } from 'react';

/** Stan kroku, tymi samymi siedmioma nazwami, którymi mówi o nim silnik. */
export type StepStatus =
  'pending' | 'ready' | 'running' | 'succeeded' | 'failed' | 'cancelled' | 'skipped';

/** Bieg tak, jak widzi go pasek pauzy, i ani o pole więcej. */
export interface RunView {
  /**
   * Kiedy limit wraca — sekundy uniksowe, prosto z drutu. `null` znaczy, że bieg wysyła
   * i paska nie ma wcale.
   *
   * Sama liczba nigdy nie trafia na ekran: użytkownik czyta godzinę u siebie, nie epokę
   * i nie ISO.
   */
  waitingUntil: number | null;
  /** Statusy kroków biegu. Pasek ich nie rysuje — dostaje je, żeby było widać, że mimo trzech
   *  trwających kroków jest jeden na bieg. */
  steps: readonly StepStatus[];
}

export interface PausedBannerProps {
  run: RunView;
  /** Strefa czasowa czytelnika. Domyślnie ta, w której stoi maszyna. */
  zone?: string;
}

export function PausedBanner(props: PausedBannerProps): ReactElement | null {
  // SZKIELET — pusty element zamiast paska i zamiast braku paska naraz: przy czekającym biegu
  // nie ma tu zdania, a przy wysyłającym jest element, którego ma nie być.
  void props;
  return <div />;
}
