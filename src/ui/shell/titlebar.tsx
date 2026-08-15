/* Pasek tytułu — całe chrome, jakie ta aplikacja ma nad pierwszą treścią.
 *
 * JEDEN pasek, nie dwa. Sufit gęstości z ARCHITECTURE §7 to 96 px chrome i jedna metafora
 * nawigacji; drugi pasek „bo tam pasuje" ustala sufit po fakcie, czyli tam, gdzie akurat
 * jesteśmy [raport 03 §4.1].
 *
 * SZKIELET (faza kontraktowa T-01): pusty nagłówek. Przełącznik sekcji, strefa przeciągania
 * i odstęp na światła dopisuje faza implementacji.
 */
import type { ReactElement } from 'react';

/** Wysokość paska. Poniżej sufitu 96 px z ARCHITECTURE §7 i to jest cały budżet chrome. */
export const TITLEBAR_HEIGHT = 48;

/**
 * Lewy odstęp paska: trzy światła zajmują ~52 px, plus `--s-4` odstępu, licząc od
 * `trafficLightPosition.x` z `tauri.conf.json`. Pierwsza kontrolka zaczyna się dopiero za nim,
 * inaczej przełącznik sekcji leży pod światłami i nie da się w niego kliknąć.
 *
 * 0 = jeszcze nieustawiony (szkielet fazy kontraktowej).
 */
export const CHROME_INSET_LEFT = 0;

export function TitleBar(): ReactElement {
  return <header />;
}
