/* Osłona wokół ekranu sekcji: błąd renderu kosztuje JEDNĄ sekcję, nie całe okno.
 *
 * PO CO TO ISTNIEJE, zmierzone 2026-08-18. `src/main.tsx` miał 19 linii i montował root bez
 * ani jednej osłony — `grep -rn 'ErrorBoundary|componentDidCatch|getDerivedStateFromError' src/`
 * dawało zero. Skutkiem był biały ekran BEZ nawigacji: React odmontowuje całe drzewo od korzenia,
 * kiedy render rzuci, więc jedna pomyłka w jednej sekcji zabierała także boczne menu, czyli
 * jedyną drogę wyjścia z tej sekcji. Aplikacji nie dało się wtedy uratować z okna — tylko
 * restartem.
 *
 * DLACZEGO WOKÓŁ SEKCJI, A NIE WOKÓŁ ROOTA. Ta sama zasada, którą `src/ui/screens.ts` zapisuje
 * dla odkrywania ekranów: „cudzy plik nie ma prawa zabrać całego okna" (niezmiennik 5 w duchu,
 * po stronie frontu). Tamten plik pilnuje tego przy WCZYTYWANIU modułu — katalog bez eksportu
 * jest pomijany w ciszy — a ten pilnuje tego samego przy RENDERZE. Bez drugiej połowy pierwsza
 * chroni tylko od pomyłek w nazwie pliku, a nie od pomyłek w kodzie.
 *
 * DLACZEGO KLASA. `componentDidCatch` i `getDerivedStateFromError` nie mają odpowiednika w
 * hakach i React 19 tego nie zmienił. To jedyny komponent klasowy w tym repo i jedyny powód,
 * dla którego wolno mu tu być.
 *
 * CZEGO TO NIE ŁAPIE, i to jest granica, nie przeoczenie: odrzuconej obietnicy. Boundary widzi
 * wyłącznie rzuty z renderu i z cyklu życia. Odmowa z `invoke`, której nikt nie złapał, leci
 * jako `unhandledrejection` i łapie ją `src/main.tsx` — dwa różne zdarzenia, dwa różne miejsca.
 */
import { Component } from 'react';
import type { ErrorInfo, ReactElement, ReactNode } from 'react';

import { saidBy } from '../../ipc/why';

export interface ScreenBoundaryProps {
  /** Identyfikator sekcji — wchodzi w zdanie dla człowieka, bo mówi, CO się nie narysowało. */
  section: string;
  /** Ekran sekcji. */
  children: ReactNode;
  /**
   * Wyjście awaryjne: przełącz na inną sekcję. `null` znaczy „nie ma gdzie pójść" i wtedy
   * osłona nie rysuje przycisku — kontrolka bez roboty nie wchodzi do repo (niezmiennik 16).
   */
  onLeave: (() => void) | null;
}

interface ScreenBoundaryState {
  /** Zdanie o tym, co padło, albo `null`, kiedy nic nie padło. */
  broke: string | null;
}

export class ScreenBoundary extends Component<ScreenBoundaryProps, ScreenBoundaryState> {
  public override state: ScreenBoundaryState = { broke: null };

  public static getDerivedStateFromError(error: unknown): ScreenBoundaryState {
    /* Zdanie wyjęte tą samą funkcją, co odmowy z Rusta: rzut w renderze bywa napisem, bywa
     * `Error`, a wołający ma w obu przypadkach dostać to samo pole. */
    return { broke: saidBy(error) };
  }

  public override componentDidCatch(error: unknown, info: ErrorInfo): void {
    /* Do konsoli okna, bo to jedyne miejsce, z którego człowiek wyjmie ślad stosu — a bez
     * śladu zdanie „this screen could not be drawn" mówi, ŻE coś padło, i nie mówi gdzie.
     * `componentStack` jest tu istotniejszy niż sam błąd: pokazuje, który komponent w drzewie
     * sekcji rzucił, a to jest pierwsze pytanie przy naprawie. */
    console.error('[loadout] screen ' + this.props.section + ' could not render', error, info);
  }

  public override render(): ReactNode {
    const { broke } = this.state;
    if (broke === null) {
      return this.props.children;
    }
    return this.fallback(broke);
  }

  /**
   * Co widzi człowiek, kiedy sekcja nie chce się narysować.
   *
   * Zdanie mówi, co się stało i co z tym zrobić (DESIGN §8): nazwa sekcji, powód, jeśli jakiś
   * przyjechał, i droga wyjścia. Nie przeprasza i nie jest ogólne.
   */
  private fallback(broke: string): ReactElement {
    const { section, onLeave } = this.props;
    return (
      <div
        data-screen-broke={section}
        className="flex h-full flex-col items-center justify-center gap-3 p-4 text-center"
      >
        <span className="flex size-8 items-center justify-center rounded-sq border border-dashed border-fail-edge text-fail">
          ◇
        </span>
        <p className="text-ink">This screen stopped working, so Loadout kept the rest running.</p>
        {broke === '' ? null : (
          <p data-screen-broke-why className="max-w-prose font-mono text-mono text-muted">
            {broke}
          </p>
        )}
        {onLeave === null ? null : (
          <button
            type="button"
            className="h-8 rounded-sq border border-line-strong bg-raised px-3 text-ui text-ink"
            onClick={onLeave}
          >
            Go to Run
          </button>
        )}
      </div>
    );
  }
}
