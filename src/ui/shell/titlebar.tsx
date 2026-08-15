/* Pasek tytułu — całe chrome, jakie ta aplikacja ma nad pierwszą treścią.
 *
 * JEDEN pasek, nie dwa. Sufit gęstości z ARCHITECTURE §7 to 96 px chrome i jedna metafora
 * nawigacji; drugi pasek „bo tam pasuje" ustala sufit po fakcie, czyli tam, gdzie akurat
 * jesteśmy [raport 03 §4.1].
 *
 * Która sekcja jest otwarta, jest powiedziane DOKŁADNIE RAZ: przez `aria-current` na
 * przełączniku (niezmiennik 13). Wygląd aktywnego przycisku bierze się z tego samego atrybutu
 * — wariant `aria-[current=true]:` czyta DOM, zamiast trzymać drugą kopię tej samej prawdy
 * w klasie. poprzedni prototyp pokazywał stan połączenia w sześciu miejscach naraz [03 §4.4].
 */
import type { ReactElement } from 'react';
import type { Section } from '../sections';
import { SECTIONS } from '../sections';
import { FIRST_SECTION, useSectionStore } from './section-store';

/** Wysokość paska. Poniżej sufitu 96 px z ARCHITECTURE §7 i to jest cały budżet chrome. */
export const TITLEBAR_HEIGHT = 48;

/**
 * Lewy odstęp paska: trzy światła zajmują ~52 px, plus `--s-4` odstępu, licząc od
 * `trafficLightPosition.x` z `tauri.conf.json`. Pierwsza kontrolka zaczyna się dopiero za nim,
 * inaczej przełącznik sekcji leży pod światłami i nie da się w niego kliknąć.
 *
 * 16 (`trafficLightPosition.x`) + 52 + 16 = 84. Zmiana `trafficLightPosition` w
 * `tauri.conf.json` bez zmiany tej liczby jest czerwona w kryterium okna — te dwie wartości
 * są związane i mierzone razem, bo osobno każda wygląda rozsądnie [T8 §11, 2026-08-15].
 */
export const CHROME_INSET_LEFT = 84;

export interface TitleBarProps {
  section?: Section;
}

export function TitleBar({ section = FIRST_SECTION }: TitleBarProps): ReactElement {
  return (
    <header
      data-chrome
      data-tauri-drag-region
      className="flex shrink-0 items-center border-b border-line bg-panel"
      style={{ height: TITLEBAR_HEIGHT, paddingLeft: CHROME_INSET_LEFT }}
    >
      <nav className="flex items-center gap-1">
        {SECTIONS.map((entry) => (
          <button
            key={entry.id}
            type="button"
            data-section-switch={entry.id}
            aria-current={entry.id === section ? 'true' : undefined}
            onClick={() => useSectionStore.getState().go(entry.id)}
            className="h-7 rounded-sq px-3 text-ui text-muted aria-[current=true]:bg-raised aria-[current=true]:text-ink"
          >
            {entry.label}
          </button>
        ))}
      </nav>
    </header>
  );
}
