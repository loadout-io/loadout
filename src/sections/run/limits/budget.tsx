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
 * 2026-08-29 — I JEST TO WARTOŚĆ, KTÓRĄ SIĘ SŁYSZY. Do tego dnia pole otwierało się puste, więc
 * „bez limitu" było stanem, do którego wpadało się przez zapomnienie, a jedynym jego nośnikiem
 * była podpowiedź `no limit` — czytana raz, kiedy nikt nie patrzy. Dziś pole otwiera się kwotą
 * z Settings, a puste znaczy „człowiek go zdjął" i mówi to zdanie obok (`NO_CEILING_SAID`).
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

/**
 * Zdanie, które stoi na ekranie, kiedy człowiek zdjął sufit z TEGO biegu.
 *
 * 2026-08-29 — POWSTAŁO, BO PUSTE POLE BYŁO JEDYNYM NOŚNIKIEM TEGO FAKTU. Podpowiedź
 * (`placeholder="no limit"`) czyta się dokładnie raz — w chwili, w której pole jest puste i nikt
 * na nie nie patrzy — a bieg bez sufitu jest wtedy nie do odróżnienia od biegu, którego kwoty
 * jeszcze nie wpisano. To jest cała klasa wady, którą to zadanie usuwa: cichy bieg bez
 * ograniczenia. Zdjąć sufit nadal wolno; przemilczeć tego nie wolno.
 */
export const NO_CEILING_SAID = 'This run has no spending limit.';

/** Ta sama podłoga, co po stronie biegu: kwota poniżej centa nie jest sufitem, tylko pomyłką. */
function looksLikeACeiling(dollars: number): boolean {
  return Number.isFinite(dollars) && dollars >= SMALLEST;
}

export interface BudgetProps {
  /** Wybrany sufit w dolarach. `null` znaczy „bez limitu". */
  value?: number | null;
  /**
   * Czy bieg, który to pole opisuje, leci BEZ ograniczenia.
   *
   * Prop, a nie `value === null` policzone tutaj, bo pasek pokazuje w tym miejscu dwa różne
   * biegi: kiedy nic nie idzie — ten, który pojedzie z następnym Startem; kiedy coś idzie — ten,
   * który właśnie idzie (`../start.tsx`). Zdanie ma mówić o tym samym biegu, co liczba nad nim,
   * a tylko wołający wie, o którym.
   */
  noCeiling?: boolean;
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

export function Budget({
  value = null,
  noCeiling = false,
  onChange,
  disabled = null,
}: BudgetProps): ReactElement {
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
          const field = event.target;
          /* 2026-08-29, DRUGA POPRAWKA — POŁOWA LICZBY NIE JEST ZDJĘCIEM SUFITU. Pole `number`
           * oddaje PUSTY napis także wtedy, gdy trzyma coś, czego nie da się przeczytać jako
           * liczby („0.", „1e", „-"), i pierwsza wersja czytała to jako „bez limitu": człowiek
           * w połowie pisania kwoty zdejmował sufit i nie wiedział o tym. `badInput` odróżnia
           * te dwa stany i jest jedyną rzeczą, która je odróżnia. */
          if (field.validity.badInput) return;
          const typed = field.value.trim();
          /* PUSTE POLE TO JEDYNA DROGA DO „BEZ SUFITU", i to jest treść tej gałęzi. Puste pole
           * to `null`, nie zero: „nie ograniczam tego biegu" i „pozwalam wydać zero" to dwa różne
           * zdania, a drugie z nich znaczy bieg, który nigdy nie ruszy. */
          if (typed === '') {
            onChange(null);
            return;
          }
          /* Kwota, która nie jest sufitem — zero, liczba ujemna, grosz mniej niż grosz — jest
           * ODRZUCANA, a nie zamieniana na „bez limitu". Pierwsza wersja robiła to drugie, więc
           * wpisanie `0` cicho puszczało bieg bez ograniczenia: pole pokazywało zero, a jechało
           * „bez sufitu". React przywraca wtedy pokazaną wartość sam, bo `value` się nie zmienia,
           * więc na ekranie nigdy nie stoi liczba, której magazyn nie ma. */
          const dollars = Number(typed);
          if (!looksLikeACeiling(dollars)) return;
          onChange(dollars);
        }}
        className="h-control w-20 shrink-0 rounded-sm border border-line bg-raised px-2 text-right font-mono text-mono text-ink disabled:opacity-40"
      />

      {/* ZDANIE STOI TU, A NIE W `title`, i to jest cała różnica między T-94 a T-208. Podpowiedź
          w polu i tekst pod kursorem czyta się raz; bieg bez ograniczenia ma być widoczny przez
          cały czas, w którym trwa. Zero pikseli, dopóki sufit stoi — więc pas nie rośnie o nic
          za informację, która nie jest prawdą (`docs/ARCHITECTURE.md` §7).

          Kolor `--attend` odpowiada na pytanie „co czeka na moją uwagę" [DESIGN §3]: to nie jest
          awaria (człowiek tak powiedział), ale jest jedyną rzeczą na tym pasku, która może
          kosztować pieniądze bez granicy. */}
      {noCeiling ? (
        <p data-no-ceiling="" className="shrink-0 whitespace-nowrap text-label text-attend">
          {NO_CEILING_SAID}
        </p>
      ) : null}
    </div>
  );
}
