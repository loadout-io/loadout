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
 * NA EKRAN NIE TRAFIA ANI LICZBA Z DRUTU, ANI JEJ ZAPIS MASZYNOWY. „resets at 1786800600"
 * i „2026-08-16T13:30:00Z" są gorsze niż brak paska, bo wyglądają na odpowiedź: pierwsze
 * nie znaczy nic, drugie znaczy godzinę w cudzej strefie.
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

/** Sekundy uniksowe z drutu, milisekundy w `Date` — jedyne miejsce, w którym ta zamiana żyje. */
const MS_PER_SECOND = 1000;

/* Dwie spacje nierozdzielające, zapisane numerem, a nie znakiem: w źródle różnica między nimi
 * a zwykłą spacją jest niewidoczna dla oka i dla recenzji. */
const NARROW_NO_BREAK_SPACE = String.fromCharCode(0x202f);
const NO_BREAK_SPACE = String.fromCharCode(0x00a0);

/** Strefa maszyny, kiedy nikt nie podał własnej. */
function machineZone(): string {
  return new Intl.DateTimeFormat().resolvedOptions().timeZone;
}

/** Ta sama chwila, czytana zegarem czytelnika. */
function localHour(unixSeconds: number, zone: string): string {
  const shown = new Intl.DateTimeFormat('en-US', {
    timeZone: zone,
    hour: 'numeric',
    minute: '2-digit',
  }).format(new Date(unixSeconds * MS_PER_SECOND));
  return shown.replaceAll(NARROW_NO_BREAK_SPACE, ' ').replaceAll(NO_BREAK_SPACE, ' ');
}

/** Całe zdanie paska, składane w jednym miejscu. */
function waitingSentence(unixSeconds: number, zone: string): string {
  return `Waiting for your Claude usage to reset at ${localHour(unixSeconds, zone)}.`;
}

export function PausedBanner({
  run,
  zone = machineZone(),
}: PausedBannerProps): ReactElement | null {
  // Bieg, który wysyła, nie ma paska — nie pustego paska. Pusty trzyma swoje miejsce na ekranie
  // i uczy ludzi przestać czytać tę część okna.
  if (run.waitingUntil === null) {
    return null;
  }

  // `--attend` znaczy „czeka na ciebie" (DESIGN §3). Nie `--fail`: nic się nie zepsuło, bieg
  // czeka — a pasek w kolorze błędu zamienia poprawną pauzę w wywrócony bieg.
  return (
    <p
      data-paused-banner=""
      className="rounded-sq border border-attend-edge bg-attend-wash px-3 py-2 text-attend"
    >
      {waitingSentence(run.waitingUntil, zone)}
    </p>
  );
}
