/* Jeden wiersz historii — siatka `history-line` z DESIGN §6: `20px 1fr auto`, padding `6px 16px`.
 *
 * Wiersz nie wie, ile linii za nim stoi ani czy jest rozwinięty: dostaje `HistoryRow` i rysuje
 * jego pola. Licznik jest już w etykiecie (`Read 6 files`), bo policzył go model — komponent,
 * który dokleja liczbę obok tekstu, jest drugim miejscem, w którym powstaje ta sama fraza.
 *
 * `+` POJAWIA SIĘ TYLKO TAM, GDZIE MA CO POKAZAĆ. Dziś jest dokładnie jedna taka rzecz:
 * ostatnie 20 linii wyjścia polecenia, które padło. Ścieżki sklejonych odczytów i zmiany
 * w plikach otwierają się w panelu szczegółów, którego w tej wersji nie ma — a przycisk
 * rozwijający wiersz w nic jest kontrolką bez handlera z dodatkowym krokiem (niezmiennik 16).
 */
import type { ReactElement } from 'react';
import type { HistoryRow } from './model';

export interface LineProps {
  row: HistoryRow;
  /** Wymagany: `+` bez tego jest ozdobą, a ozdób z kształtem przycisku to repo nie przyjmuje. */
  onToggle: (rowId: number) => void;
}

/**
 * Znacznik w pierwszej kolumnie.
 *
 * Trzy znaki, nie czternaście: znacznik odpowiada na pytanie „czy coś się zepsuło", a nie
 * powtarza rodzaj — rodzaj jest już nazwany w etykiecie. Tabela znaków per rodzaj byłaby
 * drugim słownikiem obok rejestru i rozjechałaby się z nim przy pierwszym nowym wierszu.
 */
function marker(row: HistoryRow): { glyph: string; tone: string } {
  if (row.kind === 'problem' || row.output.length > 0) return { glyph: '✕', tone: 'text-fail' };
  if (row.kind === 'done') return { glyph: '✓', tone: 'text-muted' };
  return { glyph: '·', tone: 'text-muted' };
}

export function Line({ row, onToggle }: LineProps): ReactElement {
  const { glyph, tone } = marker(row);
  const hasMore = row.output.length > 0;

  return (
    <div data-line={row.id} className="grid grid-cols-[20px_1fr_auto] gap-2 px-4 py-1.5">
      <span className={`text-body ${tone}`}>{glyph}</span>

      <span className="text-body text-ink">
        {/* Kto to zrobił, w mono i w kolorze przygaszonym — kolor tożsamości agenta nadaje
            szyna agentów (T-09), żeby mapa agent→kolor powstała w jednym miejscu. */}
        <span className="mr-2 font-mono text-mono-strong text-muted">{row.agent}</span>
        {row.label}
      </span>

      {hasMore ? (
        <button
          type="button"
          onClick={() => onToggle(row.id)}
          aria-label={row.expanded ? 'Show less' : 'Show more'}
          className="h-7 rounded-sq border border-line px-3 text-ui text-body"
        >
          {row.expanded ? '−' : '+'}
        </button>
      ) : null}

      {row.expanded && hasMore ? (
        /* Wyjście jest wartością maszynową: mono, do zaznaczenia i skopiowania. Model dał tu
           OSTATNIE dwadzieścia linii — to ta połowa logu, w której stoi powód. */
        <pre className="col-start-2 overflow-x-auto rounded-sq bg-well p-3 font-mono text-mono text-body">
          {row.output.join('\n')}
        </pre>
      ) : null}
    </div>
  );
}
