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
import { type ReactElement, useState } from 'react';
import { identityToken } from '../rail/colour';
import { authorityOf } from '../rail/say';
import type { HistoryRow } from './model';
import { runSuggestion, suggestion } from './suggested';

export interface LineProps {
  row: HistoryRow;
  /** Wymagany: `+` bez tego jest ozdobą, a ozdób z kształtem przycisku to repo nie przyjmuje. */
  onToggle: (rowId: number) => void;
  /**
   * Komenda, którą przyniósł wiersz propozycji — znak w znak taka, jaką napisał lider.
   *
   * SKĄD PRZYJEŻDŻA: z pola `command` w linii z drutu, przez `HistoryRow.command`, które podaje
   * tu `./feed.tsx` — jeden przewóz, bez ani jednej gałęzi po drodze. Nieobecna znaczy „ten
   * wiersz nie jest propozycją": tak wygląda każdy inny rodzaj i tak wygląda wiersz zbudowany
   * przez kogoś, kto o propozycjach nie wie.
   *
   * PROPS, A NIE ODCZYT `row.command` W ŚRODKU, i to jest różnica na jeden fakt: wiersz jest
   * jedynym miejscem, w którym ta komenda mieszka, a komponent ją tylko dostaje. Wersja czytająca
   * pole samodzielnie miałaby ją z dwóch stron naraz w chwili, w której ktokolwiek poda inną
   * (niezmiennik 13).
   *
   * CZEGO TEN PROP NIE ROZSTRZYGA: czy przycisk w ogóle jest. To rozstrzyga `row.kind`, czyli
   * decyzja podjęta w Ruście. Wiersz `note` z tą samą komendą przycisku nie dostaje — inaczej
   * okno dorysowywałoby go każdemu, kto napisze `/run` w prozie, i wracalibyśmy do kuracji
   * w CSS-ie (niezmiennik 15).
   */
  command?: string | undefined;
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

/**
 * Akcent, bo od T-45 znaczy dokładnie jedno: „to jest interaktywne" [DESIGN §3]. Pastylka
 * i ten sam stopień co przy `+` obok, żeby wiersz zachował swój rytm — kontrolka startu jest
 * czynnością w wierszu rozmowy, nie przyciskiem Start z paska pracy.
 */
const PROPOSE =
  'h-[17px] rounded-pill border border-accent-edge px-[7px] font-mono text-meta text-accent';

/**
 * Rodzaje wiersza, których treść jest PROZĄ do przeczytania, a nie etykietą czynności.
 *
 * Tylko one dostają czytelną miarę wiersza. Reszta jest etykietą — komenda, ścieżka, licznik —
 * a etykieta zawinięta w połowie kolumny czyta się gorzej, nie lepiej.
 *
 * JEDEN RODZAJ, i to nie jest niedopatrzenie. Makieta zawęża regułę do `.ln.note`, a plik
 * fikstur nazywa `note` „jedyną prozą w widoku" — dwa źródła mówią to samo. Lista, a nie
 * porównanie do jednej wartości, bo `handoff` dołączy do niej, kiedy dostanie producenta
 * (projekt `2026-08-30-rozmowa-jest-kregoslupem`, pozycja 4).
 */
const PROSE: readonly string[] = ['note'];

/**
 * Czytelna miara wiersza dla prozy — **wartość z makiety**, nie wymyślona tutaj.
 *
 * `docs/mockup/index.html` ma `.ln.note .t{max-width:64ch}` od początku i okno jej nigdy nie
 * zastosowało. Zmierzone 2026-08-30 na zrzucie właściciela: odpowiedź agenta szła przez całą
 * szerokość kolumny strumienia, czyli grubo ponad 200 znaków w wierszu — a oko gubi początek
 * następnego wiersza, kiedy musi po niego wracać przez pół ekranu.
 *
 * Kryterium `run-matches-mockup.test.tsx` sądzi wyłącznie dwie siatki, więc ta wartość odjechała
 * od wyroczni bez ani jednej czerwieni. Dlatego stoi tu z nazwą pliku, z którego pochodzi.
 */
const MEASURE = 'max-w-[64ch]';

/** Co wiersz propozycji daje przyciskowi: nazwę do przeczytania i komendę do uruchomienia. */
interface Proposal {
  readonly workflow: string;
  readonly command: string;
}

/**
 * Co ten wiersz proponuje — albo `null`, kiedy niczego nie proponuje.
 *
 * RODZAJ ROZSTRZYGA, NIE TREŚĆ. Wiersz `note` z tą samą prozą i tą samą komendą przycisku nie
 * dostaje, bo o tym, czy proza lidera jest propozycją, zdecydował Rust w mapowaniu
 * zdarzenie → linia (niezmiennik 15). Okno, które dorysowuje przycisk każdemu, kto napisze
 * `/run` w akapicie, jest kuracją w CSS-ie: da się ją zepsuć arkuszem stylów i nie ma jej
 * w `run.json`.
 *
 * Nazwa workflow pochodzi z `./suggested`, nie z rozbioru napisanego tutaj: to jest polityka,
 * a polityka w komponencie jest kodem, którego kryterium nie umie dotknąć — to repo nie ma
 * jsdom, więc `onClick` nie odpala się w żadnym teście.
 */
function proposalOf(row: HistoryRow, command: string | undefined): Proposal | null {
  if (row.kind !== 'suggested' || command === undefined) return null;
  const proposes = suggestion(command);
  return proposes === null ? null : { workflow: proposes.workflow, command };
}

export function Line({ row, onToggle, command }: LineProps): ReactElement {
  const { glyph, tone } = marker(row);
  /* DWIE RZECZY MOGĄ STAĆ ZA WIERSZEM i jedna kontrolka je otwiera: wyjście komendy, która
     padła, i proza, która nie zmieściła się w wierszu. Kontrolka jest jedna, bo pytanie
     człowieka jest jedno — „pokaż mi resztę"; różnią się dopiero tym, jak są narysowane. */
  const hasMore = row.output.length > 0 || row.body.length > 0;
  const proposal = proposalOf(row, command);
  /**
   * Zdanie, które wróciło z próby uruchomienia; `null`, dopóki nie ma o czym mówić.
   *
   * TRZYMANE TUTAJ, bo dotyczy TEGO wiersza i tego kliknięcia: zdanie odmowy nie jest stanem
   * strumienia i nie ma prawa przeżyć wiersza, przy którym powstało. Odmowa porzucona po drodze
   * jest gorsza niż brak przycisku: człowiek klika, nie dzieje się nic, o czym da się przeczytać,
   * i to czyta się jak zepsuta aplikacja (DESIGN §8).
   */
  const [said, setSaid] = useState<string | null>(null);

  return (
    <div
      data-line={row.id}
      className="grid grid-cols-[18px_minmax(0,1fr)_auto] gap-2 px-[18px] py-[5px]"
    >
      <span className={`text-center font-mono text-mono ${tone}`}>{glyph}</span>

      <span
        className={`min-w-0 text-body text-ink${PROSE.includes(row.kind) ? ' ' + MEASURE : ''}`}
      >
        {/* TWOJE ZDANIE JEST PODPISANE TOBĄ, nie agentem, i to jest cała treść tej gałęzi.

            2026-08-19 — zgłoszenie właściciela: „a może odpisuje on, ale na pewno nie widać moich
            wiadomości". Wiersz `told` niesie w polu `agent` ADRESATA (bo tak niesie go każdy inny
            wiersz tego kroku), więc narysowany zwykłą drogą wyglądałby jak zdanie, które
            powiedział agent — czyli gorzej niż brak wiersza: strumień przypisywałby Twoje słowa
            komuś innemu.

            „Kto mówi" bierzemy z `authorityOf`, czyli z jedynego miejsca, w którym ta polityka
            mieszka (`rail/say.ts`) — drugi warunek `kind === 'told'` tutaj byłby drugą odpowiedzią
            na to samo pytanie (niezmiennik 13).

            Kolor `--human` od 2026-08-19, wcześniej `--accent`. Ten wiersz odpowiada na pytanie
            „co zrobiła osoba, a nie maszyna", i dokładnie na to pytanie odpowiada `--human`;
            akcent od T-45 znaczy wyłącznie „to jest interaktywne", a Twoje zdanie nie jest
            kontrolką. Nazwa adresata zostaje w SWOIM kolorze tożsamości, żeby było widać,
            do kogo to poszło. */}
        {authorityOf(row.kind) === 'you' ? (
          <>
            <span className="mr-1 font-mono text-mono-strong text-human">You →</span>
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
        {/* ZDANIE AGENTA ZACHOWUJE SWOJE WIERSZE.
            2026-08-23, zgłoszenie właściciela o czytelności strumienia: „ten tekst niech też
            będzie jakoś fajnie i ładnie formatowany aby było to przyjemniejsze".

            Model przepuszcza tekst agenta NIETKNIĘTY (`feed/model.ts`, `sentence`), więc jego
            przełamania dojeżdżały aż tutaj i ginęły dopiero w CSS: domyślne `white-space`
            zamienia każdy przełam w spację, a akapit wypunktowany przez agenta sklejał się
            w jeden blok bez ani jednej przerwy. To nie jest brak renderera markdown — to była
            utrata rzeczy, którą model faktycznie napisał.

            `pre-line`, nie `pre`: zwija ciągi spacji (więc wcięcia z modelu nie robią schodów
            w wąskiej kolumnie) i zostawia przełamania. Zawijanie długich wierszy zostaje, bo
            `pre` odbierałoby je i wypychało kolumnę w bok.

            `break-words` dla adresów i ścieżek bez spacji: bez tego jedno długie słowo rozpycha
            kolumnę strumienia i wypycha kolumnę agentów poza krawędź okna.

            WŁASNY ELEMENT, nie klasa na rodzicu: rodzic niesie też podpis „kto mówi", a ten ma
            zostać w jednym wierszu z początkiem zdania. */}
        <span className="whitespace-pre-line break-words">{row.label}</span>
      </span>

      {/* Prawa kolumna: albo kontrolka startu propozycji, albo liczba, którą ta czynność
          zostawiła, albo `+` do wyjścia, które padło, albo nic. Nigdy dwa naraz — jedna
          propozycja jest jedną kontrolką, a dwie rzeczy wyglądające na klikalne w jednym
          wierszu nie mówią, która z nich zaczyna pracę.

          NAZWA WORKFLOW JEST W NAZWIE PRZYCISKU, nie w tekście obok: „Run" samo nie mówi, co
          się stanie — a stanie się to, że ruszą agenci i zaczną się pieniądze. To jest ta jedna
          rzecz, którą kontrolka musi powiedzieć, zanim ktoś ją naciśnie. */}
      {proposal !== null ? (
        <button
          type="button"
          onClick={() => {
            /* JEDNA DROGA STARTU, ta sama, co Enter w wierszu wejścia (niezmiennik 23):
               `runSuggestion` oddaje ją `startFromLine`, więc limit „ile naraz", folder zakresu
               i zdania odmowy mają jedno miejsce. Wynik NIE jest porzucany — odmowa wraca tu
               i staje w wierszu pod przyciskiem. */
            void runSuggestion(proposal.command).then(setSaid);
          }}
          className={PROPOSE}
        >
          {'Run ' + proposal.workflow}
        </button>
      ) : hasMore ? (
        <button
          type="button"
          onClick={() => onToggle(row.id)}
          aria-label={row.expanded ? 'Show less' : 'Show more'}
          className="h-[17px] rounded-pill border border-line px-[5px] font-mono text-meta text-muted"
        >
          {row.expanded ? '−' : '+'}
        </button>
      ) : (
        <span className="font-mono text-mono whitespace-nowrap text-muted">{row.metric}</span>
      )}

      {row.expanded && row.body.length > 0 ? (
        /* PROZA, NIE WYJŚCIE MASZYNY. Ten sam blok co niżej, ale bez monospace'u i bez czerwonej
           krawędzi: to jest zdanie agenta i czyta się je jak tekst, a czerwona krawędź znaczy
           w tym widoku „to padło". Miara wiersza ta sama, co w nagłówku — proza nie zmienia
           szerokości od tego, że ją rozwinięto. */
        <p
          data-line-body
          className={`col-start-2 whitespace-pre-line break-words text-body text-body ${MEASURE}`}
        >
          {row.body.join('\n')}
        </p>
      ) : null}

      {row.expanded && row.output.length > 0 ? (
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

      {/* CO POWIEDZIAŁA PRÓBA URUCHOMIENIA. Stoi w tym wierszu, bo dotyczy tego przycisku:
          workflow, którego nie ma na dysku, kończy się zdaniem, a nie ciszą. Kontrolka, po
          której nic nie widać, jest nieodróżnialna od zepsutej (DESIGN §8), a ta zaczyna pracę
          za pieniądze. Gaśnie razem z wierszem — nie ma tu drugiego żywego regionu na fakt,
          o którym mówi pasek pracy (niezmiennik 13). */}
      {said === null ? null : (
        <p data-line-said className="col-start-2 text-body text-fail">
          {said}
        </p>
      )}
    </div>
  );
}
