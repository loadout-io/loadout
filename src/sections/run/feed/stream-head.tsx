/* Nagłówek kolumny strumienia — `.sthead` z makiety `polecenie.html`.
 *
 * Trzy rzeczy i ani jednej więcej: czy strumień jest ŻYWY, KOGO w nim słychać, i czy widok
 * NADĄŻA za najnowszym wierszem. Każda z nich odpowiada na pytanie, którego sam strumień
 * zadać nie umie — bieg, który stanął, i bieg, w którym przez minutę nikt się nie odezwał,
 * wyglądają w kolumnie identycznie.
 *
 * CHIPY SĄ PRAWDZIWYMI PRZYCISKAMI i naprawdę zawężają — polityka stoi w `./speakers.ts`,
 * a to, że kliknięcie do niej dochodzi, sprawdza prawdziwa mysz w
 * `e2e/tests/a-key-answers-and-a-chip-narrows.spec.ts`. Rząd chipów, który tylko pokazuje,
 * kto mówi, byłby czterema kontrolkami bez skutku (niezmiennik 16).
 *
 * NIC TU NIE JEST WPISANE NA STAŁE. Lista podpisów przyjeżdża policzona z historii, więc chip
 * agenta, który się nie odezwał, nie ma jak powstać (niezmiennik 17).
 */
import type { ReactElement } from 'react';
import { EVERYONE } from './speakers';

export interface StreamHeadProps {
  /** Podpisy, które padły w tym strumieniu — w kolejności pierwszego wiersza. */
  speakers: readonly string[];
  /** Który chip jest w mocy: `EVERYONE` albo jeden z podpisów. */
  showing: string;
  onShow: (who: string) => void;
  /** Czy cokolwiek jeszcze pracuje. Kropka bije wyłącznie wtedy. */
  live: boolean;
  /** Czy widok stoi przy najnowszym wierszu. */
  following: boolean;
}

export function StreamHead({
  speakers,
  showing,
  onShow,
  live,
  following,
}: StreamHeadProps): ReactElement {
  return (
    <header
      data-stream-head
      className="flex shrink-0 flex-wrap items-center gap-2 border-b border-line px-[14px] py-[9px]"
    >
      {/* NAPIS NAZYWA KOLUMNĘ, KROPKA MÓWI O STANIE, i to są dwa różne fakty. `Live` odróżnia tę
          kolumnę od panelu biegów zapisanych, więc stoi zawsze; puls i barwa odpowiadają na
          „czy coś teraz idzie" i gasną razem z ostatnim pracującym agentem. Bijąca kropka nad
          biegiem, który zszedł, byłaby zdaniem o pracy, której nikt nie wykonuje
          (niezmiennik 17) — a to jest ostatnia rzecz na tym ekranie, którą człowiek by podważył. */}
      <span
        data-stream-live={live ? 'yes' : 'no'}
        /* Stopień nadoczka niesie wersaliki sam (`src/styles/theme.css`, `.text-eyebrow`);
           drugi napis tutaj byłby drugą kopią jednego faktu (niezmiennik 13). Barwa idzie stylem,
           bo odpowiada na inne pytanie niż stopień: czy TERAZ coś pracuje. */
        className="flex items-center gap-2 font-mono text-eyebrow"
        style={{ color: live ? 'var(--color-live)' : 'var(--color-muted)' }}
      >
        <i
          aria-hidden="true"
          className={`h-[6px] w-[6px] rounded-full${live ? ' working' : ''}`}
          style={{ background: 'currentColor' }}
        />
        Live
      </span>

      {speakers.length === 0
        ? null
        : [EVERYONE, ...speakers].map((who) => (
            <button
              key={who}
              type="button"
              data-speaker={who}
              aria-pressed={who === showing}
              {...(who === showing ? { 'data-tone': 'accent' } : {})}
              onClick={() => {
                onShow(who);
              }}
              className="chip"
            >
              {who}
            </button>
          ))}

      {/* CZY WIDOK NADĄŻA. Przypięcie do dołu robi układ (`./feed.tsx`), więc dopóki w strumieniu
          są wiersze, nowe lądują pod okiem — i to jest cała treść tego napisu. Bez niego człowiek,
          który odjechał w górę, nie ma na ekranie ani jednego zdania mówiącego, dlaczego wiersze
          przestały dochodzić na dole. */}
      {following ? (
        <span data-following className="value ml-auto flex items-center gap-2 text-meta">
          <i
            aria-hidden="true"
            className="h-[6px] w-[6px] rounded-full"
            style={{ background: 'var(--color-ok)' }}
          />
          following
        </span>
      ) : null}
    </header>
  );
}
