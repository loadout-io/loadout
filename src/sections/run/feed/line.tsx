/* Jeden wiersz historii — siatka `.ln` z makiety: `18px minmax(0,1fr) auto`, padding `5px 18px`.
 *
 * Wiersz nie wie, ile linii za nim stoi ani czy jest rozwinięty: dostaje `HistoryRow` i rysuje
 * jego pola. Licznik jest już w etykiecie (`Read 6 files`), bo policzył go model — komponent,
 * który dokleja liczbę obok tekstu, jest drugim miejscem, w którym powstaje ta sama fraza.
 *
 * `+` POJAWIA SIĘ TYLKO TAM, GDZIE MA CO POKAZAĆ. Dziś jest dokładnie jedna taka rzecz:
 * ostatnie 20 linii wyjścia polecenia, które padło. Ścieżki sklejonych odczytów i zmiany
 * w plikach otwierają się w panelu szczegółów, którego w tej wersji nie ma — a przycisk
 * rozwijający wiersz w nic jest kontrolką bez handlera z dodatkowym krokiem (niezmiennik 16).
 *
 * 2026-08-18 — TRZY RZECZY Z MAKIETY, KTÓRYCH TU NIE BYŁO, i każda z nich niosła treść:
 *
 *   1. PODPIS AGENTA W KOLORZE TOŻSAMOŚCI (`.ln .who` z `--id-1`…`--id-5`). Cztery agenci
 *      w jednym strumieniu byli czterema identycznymi szarymi napisami, więc jedyną drogą do
 *      pytania „kto to zrobił" było czytanie liter. Kolor przydziela `rail/colour.ts` — TA SAMA
 *      funkcja, z której żyje kwadrat na kafelku agenta, żeby mapa agent→kolor powstała raz
 *      (niezmiennik 13). Przygaszony i nigdy nasycony: tożsamość ≠ stan [DESIGN §3].
 *   2. PRAWA KOLUMNA Z METRYKĄ (`.ln .m`). `+42 −8` i `3 of 40` przyjeżdżają z drutu i do dziś
 *      nie miały gdzie wylądować; liczy je model, ten plik ją tylko stawia.
 *   3. BLOK BŁĘDU NA LEWEJ KRAWĘDZI W `--fail` (`.detail`). Wyjście, które padło, wyglądało
 *      identycznie jak zwykły blok tekstu.
 */
import type { ReactElement } from 'react';
import { identityToken } from '../rail/colour';
import { authorityOf } from '../rail/say';
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
    <div
      data-line={row.id}
      className="grid grid-cols-[18px_minmax(0,1fr)_auto] gap-2 px-[18px] py-[5px]"
    >
      <span className={`text-center font-mono text-mono ${tone}`}>{glyph}</span>

      <span className="min-w-0 text-body text-ink">
        {/* TWOJE ZDANIE JEST PODPISANE TOBĄ, nie agentem, i to jest cała treść tej gałęzi.

            2026-08-19 — zgłoszenie właściciela: „a może odpisuje on, ale na pewno nie widać moich
            wiadomości". Wiersz `told` niesie w polu `agent` ADRESATA (bo tak niesie go każdy inny
            wiersz tego kroku), więc narysowany zwykłą drogą wyglądałby jak zdanie, które
            powiedział agent — czyli gorzej niż brak wiersza: strumień przypisywałby Twoje słowa
            komuś innemu.

            „Kto mówi" bierzemy z `authorityOf`, czyli z jedynego miejsca, w którym ta polityka
            mieszka (`rail/say.ts`) — drugi warunek `kind === 'told'` tutaj byłby drugą odpowiedzią
            na to samo pytanie (niezmiennik 13).

            Kolor `--accent`, ten sam, co znak zachęty `❯` w wierszu wejścia: zdanie ma czytać się
            jako pochodzące z tego pola, w które je wpisałeś. Nazwa adresata zostaje w SWOIM
            kolorze tożsamości, żeby było widać, do kogo to poszło. */}
        {authorityOf(row.kind) === 'you' ? (
          <>
            <span className="mr-1 font-mono text-mono-strong text-accent">You →</span>
            <span
              className="mr-2 font-mono text-mono-strong"
              style={{ color: `var(${identityToken(row.agent)})` }}
            >
              {row.agent}
            </span>
          </>
        ) : (
          /* Kto to zrobił, w mono i w kolorze TOŻSAMOŚCI tego agenta — ta sama mapa, z której
             żyje kwadrat na kafelku w liście agentów. */
          <span
            className="mr-2 font-mono text-mono-strong"
            style={{ color: `var(${identityToken(row.agent)})` }}
          >
            {row.agent}
          </span>
        )}
        {row.label}
      </span>

      {/* Prawa kolumna: albo liczba, którą ta czynność zostawiła, albo `+` do wyjścia, które
          padło, albo nic. Nigdy oba — `+` stoi przy wierszu, który ma co pokazać, a metryka
          przy tym, który ma co powiedzieć liczbą. */}
      {hasMore ? (
        <button
          type="button"
          onClick={() => onToggle(row.id)}
          aria-label={row.expanded ? 'Show less' : 'Show more'}
          className="h-[17px] rounded-sq border border-line px-[5px] font-mono text-meta text-muted"
        >
          {row.expanded ? '−' : '+'}
        </button>
      ) : (
        <span className="font-mono text-mono whitespace-nowrap text-muted">{row.metric}</span>
      )}

      {row.expanded && hasMore ? (
        /* Wyjście jest wartością maszynową: mono, do zaznaczenia i skopiowania. Model dał tu
           OSTATNIE dwadzieścia linii — to ta połowa logu, w której stoi powód. Lewa krawędź
           w `--fail` jest z makiety (`.detail`) i jest jedyną rzeczą, która odróżnia ten blok
           od zwykłego akapitu tekstu maszynowego. */
        <pre
          data-copyable
          className="col-start-2 overflow-x-auto border-l-2 border-l-fail bg-well px-[11px] py-[9px] font-mono text-mono text-muted"
        >
          {row.output.join('\n')}
        </pre>
      ) : null}
    </div>
  );
}
