/* `empty-state` z DESIGN §6: wyśrodkowany znak `◇` w ramce `1px dashed --line-strong`
 * i JEDNO zdanie.
 *
 * Bez przycisku. DESIGN §6 przewiduje tu jeden przycisk podstawowy, ale w T-01 nie ma jeszcze
 * czego tworzyć — „Create", który nic nie robi, jest kontrolką bez handlera (niezmiennik 16)
 * i na zrzucie ekranu wygląda lepiej niż wersja poprawna. poprzedni prototyp ma trzy takie przyciski
 * [03 §7.3]. Przycisk wraca w tym samym commicie, w którym pojawia się rzecz do stworzenia.
 */
import type { ReactElement } from 'react';

export interface EmptyStateProps {
  children: string;
}

export function EmptyState({ children }: EmptyStateProps): ReactElement {
  return (
    <div data-empty className="flex h-full flex-col items-center justify-center gap-3">
      <span className="flex size-8 items-center justify-center rounded-sq border border-dashed border-line-strong text-muted">
        ◇
      </span>
      <p className="text-ink">{children}</p>
    </div>
  );
}
