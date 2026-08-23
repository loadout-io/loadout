/* Kontrolka „Spend at most $" — sufit wydatku jednego biegu.
 *
 * Stoi obok suwaka „ile naraz", bo odpowiada na to samo pytanie, tylko drugą walutą: tamten
 * mówi, ile maszyny wolno zająć, ten — ile pieniędzy. Obie decyzje podejmuje się PRZY KAŻDYM
 * biegu, a nie raz w ustawieniach, bo obie zależą od tego, co ten bieg ma zrobić.
 *
 * ZMIERZONE, PO CO TO JEST. 96-minutowy bieg właściciela kosztował ~$40 u Claude'a i nikt nie
 * mógł powiedzieć „stop po $20": jedynym limitem biegu był limit czasu, a minuty nie są ceną.
 * Czas i pieniądze rozjeżdżają się dokładnie wtedy, kiedy to boli — model, który myśli dłużej,
 * kosztuje więcej za tę samą minutę.
 *
 * PUSTE ZNACZY „BEZ LIMITU", i to jest wartość, nie brak wartości. Zero znaczyłoby bieg, który
 * nie ma prawa ruszyć — czyli kontrolkę, którą da się ustawić w stan bez sensu.
 *
 * ZDANIE POMOCY SCHODZI DO `title`, dokładnie jak przy suwaku obok i z tego samego zmierzonego
 * powodu: pas nad obszarem pracy ma sufit 96 px (`docs/ARCHITECTURE.md` §7), a to zdanie czyta
 * się raz. Nie znika — mówi o LUCE W POMIARZE, a nie o działaniu kontrolki, więc człowiek, który
 * go nigdy nie zobaczy, czyta sufit jako obietnicę, której produkt nie może dotrzymać.
 *
 * Czysta funkcja stanu na markup: nie trzyma własnego stanu i nie wie, że istnieje `invoke()`.
 * `onChange` jest WYMAGANY (niezmiennik 16) — dokładnie z tego powodu, co przy suwaku obok.
 */
import type { ChangeEvent, ReactElement } from 'react';

/** Etykieta i pole muszą wskazywać na siebie nawzajem, a do tego potrzebny jest identyfikator. */
const FIELD_ID = 'budget-usd';

/** Najmniejsza kwota, jaką da się postawić. Poniżej centa nie ma czego ograniczać. */
const SMALLEST = 0.01;

/**
 * Zdanie pomocy — i mówi ono o LUCE W POMIARZE, nie o działaniu kontrolki.
 *
 * Krok Codeksa nie mówi, ile kosztowała jego tura, więc liczy się do sumy jako zero: bieg
 * z samych takich kroków nigdy nie dobije do sufitu, choć naprawdę kosztuje. Zdanie zniknie
 * stąd w dniu, w którym tamten vendor zacznie podawać cenę (T-97).
 */
export const BUDGET_HELP =
  'Codex steps do not say what they cost, so they count as zero against this.';

/** Etykieta pola — pytanie zadane słowami, którymi zadałby je człowiek (DESIGN §8). */
export const BUDGET_LABEL = 'Spend at most $';

/** Ta sama podłoga, co po stronie biegu: kwota poniżej centa nie jest sufitem, tylko pomyłką. */
function withinReason(dollars: number): number | null {
  return Number.isFinite(dollars) && dollars >= SMALLEST ? dollars : null;
}

export interface BudgetProps {
  /** Wybrany sufit w dolarach. `null` znaczy „bez limitu". */
  value?: number | null;
  /** Wymagany: kontrolka bez handlera nie wchodzi do repo (niezmiennik 16). */
  onChange: (budgetUsd: number | null) => void;
  /**
   * Powód, dla którego tej liczby nie da się teraz zmienić — albo `null`, kiedy da się.
   *
   * ZDANIE, NIE `boolean`, i z tego samego powodu, co przy suwaku obok: wygaszona kontrolka
   * bez powodu jest zagadką, a typ ma wymusić powód razem z wygaszeniem.
   *
   * Powód jedzie do `title`, a nie do drugiego akapitu na pasku: to samo zdanie stoi już przy
   * suwaku, a jeden fakt ma jedno miejsce na ekranie (niezmiennik 13).
   */
  disabled?: string | null;
}

export function Budget({ value = null, onChange, disabled = null }: BudgetProps): ReactElement {
  return (
    <div className="flex min-w-0 shrink-0 items-center gap-2">
      <label className="min-w-0 truncate text-label text-muted" htmlFor={FIELD_ID}>
        {BUDGET_LABEL}
      </label>
      {/* Kwota jest wartością maszynową, więc mono — reguła semantyczna z DESIGN §4.
          `data-budget` jest kotwicą dla kryterium: bez niej „czy TO pole jest wygaszone"
          rozstrzygałoby się po policzeniu słowa `disabled` w całym pasku. */}
      <input
        data-budget=""
        id={FIELD_ID}
        type="number"
        inputMode="decimal"
        min={SMALLEST}
        step={SMALLEST}
        placeholder="no limit"
        value={value === null ? '' : String(value)}
        disabled={disabled !== null}
        title={disabled ?? BUDGET_HELP}
        onChange={(event: ChangeEvent<HTMLInputElement>) => {
          /* Puste pole to `null`, nie zero: „nie ograniczam tego biegu" i „pozwalam wydać zero"
           * to dwa różne zdania, a drugie z nich znaczy bieg, który nigdy nie ruszy. */
          const typed = event.target.value.trim();
          onChange(typed === '' ? null : withinReason(Number(typed)));
        }}
        className="h-control w-20 shrink-0 rounded-sm border border-line bg-raised px-2 text-right font-mono text-mono text-ink disabled:opacity-40"
      />
    </div>
  );
}
