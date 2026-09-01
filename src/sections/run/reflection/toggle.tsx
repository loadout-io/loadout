import type { ReactElement } from 'react';

export const REFLECTION_LABEL = 'Learn from this run';

/**
 * Co się stanie, jeśli człowiek zostawi tę kontrolkę włączoną.
 *
 * 2026-08-29 (T-165) — DO DZIŚ STAŁ TU SAM PTASZEK. Kontrolka nazywała czynność („learn"),
 * a nie jej skutek, więc jedyną drogą do odpowiedzi na pytanie „co on z tym zrobi" było
 * przeczytanie `commands::run::what_this_run_taught_us`. Zdanie mówi trzy rzeczy, których
 * z samego napisu nie da się zgadnąć: że powstają NOTATKI, że jest ich najwyżej trzy
 * (`AT_MOST_KEPT` po tamtej stronie) i że nie wchodzą do niczego, dopóki człowiek ich nie
 * przyjmie w sekcji Memory.
 *
 * JEDNO ZDANIE, nie akapit: pasek loadoutu ma 52 px i tyle mu zostaje (`../strip/strip.tsx`).
 */
export const REFLECTION_EXPLAINED =
  'Left on, it keeps up to three notes from this run for you to approve in Memory.';

/**
 * Czym to zdanie jest dla ptaszka: OPISEM, nigdy jego nazwą.
 *
 * 2026-08-29 — ROZRÓŻNIENIE ZMIERZONE, NIE TEORETYCZNE. Pierwsza wersja postawiła zdanie
 * WEWNĄTRZ `<label>`, obok napisu — i nazwa dostępna ptaszka stała się przez to całym
 * akapitem, bo nazwą kontrolki zawiniętej w etykietę jest cała treść tej etykiety. Zobaczyło
 * to cudze kryterium `e2e/tests/t126-reflection-choice-real-routes.spec.ts`, które szuka
 * kontrolki po nazwie DOKŁADNEJ, i wywaliło się siedem razy. Czytający ekranem usłyszałby to
 * samo, co ten selektor: pole wyboru o nazwie długości zdania.
 *
 * Zdanie stoi więc OBOK etykiety i wraca do kontrolki przez `aria-describedby` — czyli tam,
 * gdzie mieszkają opisy. Napis nazywa, opis wyjaśnia; jedno pole na jedno i jedno na drugie.
 */
const EXPLAINED_ID = 'reflection-explained';

export interface ReflectionToggleProps {
  readonly enabled: boolean;
  readonly disabled?: boolean;
  readonly onChange: (enabled: boolean) => void;
}

/** The visible owner of the private post-run learning choice. */
export function ReflectionToggle({
  enabled,
  disabled = false,
  onChange,
}: ReflectionToggleProps): ReactElement {
  return (
    <span className="flex min-w-0 items-center gap-2">
      {/* PTASZEK ZOSTAJE W ETYKIECIE, nie przy `htmlFor`: cudze kryterium sięga po niego
          selektorem `label:has-text(…) input[type="checkbox"]`
          (`e2e/tests/t161-long-workflow-stays-inside-run.spec.ts`), a poza tym kliknięcie
          w napis ma przełączać ptaszek i tak robi to samo zawinięcie. */}
      <label className="flex items-center gap-2 whitespace-nowrap text-ui text-ink">
        <input
          type="checkbox"
          checked={enabled}
          disabled={disabled}
          aria-describedby={EXPLAINED_ID}
          onChange={(event) => {
            onChange(event.target.checked);
          }}
        />
        <span>{REFLECTION_LABEL}</span>
      </label>
      {/* ZDANIE STOI NA EKRANIE, nie tylko w `title`, i to jest różnica mierzona, nie
          estetyczna: podpowiedź odpowiada wyłącznie temu, kto już zatrzymał mysz nad ptaszkiem,
          a pytanie „co on z tym zrobi" ma się nasunąć PRZED włączeniem. `title` zostaje mimo to,
          bo przy wąskim oknie `truncate` utnie ogon i wtedy jest gdzie przeczytać całość — ta
          sama para, co przy ostrzeżeniu o pamięci (`../limits/at-once.tsx`).

          `text-label`, a nie `text-ui`: wiersz zachowuje wysokość ptaszka, więc pasek zostaje
          jednorzędowy i sufit chrome z `docs/ARCHITECTURE.md` §7 się nie rusza. Nadmiar
          szerokości przejmuje wycinek kontrolek w pasku (`../strip/strip.tsx`). */}
      <span
        id={EXPLAINED_ID}
        className="min-w-0 truncate text-label text-muted"
        title={REFLECTION_EXPLAINED}
      >
        {REFLECTION_EXPLAINED}
      </span>
    </span>
  );
}
