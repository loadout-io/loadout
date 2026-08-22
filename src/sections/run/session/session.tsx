/* Ekran JEDNEGO agenta — dwa bloki faktów, potem to, co ten agent powiedział [makieta 491–536].
 *
 * TEN PLIK NIE PODEJMUJE ANI JEDNEJ DECYZJI O TREŚCI. Bierze gotowe sekcje z `layout.ts`
 * i gotowy kafelek z `rail/card.ts` i zamienia je na markup. Gdyby wybierał wiersze albo liczył
 * cokolwiek, „co dostał" i „co wyprodukował" istniałoby w dwóch miejscach (niezmiennik 23) —
 * a to jest dokładnie ten ekran, na którym rozjazd między nimi kosztuje najwięcej: rubryka
 * faktów karmiona czymś innym niż fakty czyta się jak fakt [00-SYNTHESIS §2.2].
 *
 * CZEGO Z MAKIETY TU NIE MA, i każdy brak ma powód, nie przeoczenie:
 *
 *   `Stop this agent`   po stronie Rusta nie ma komendy zatrzymującej JEDNEGO agenta:
 *                       `stop_run` nie bierze żadnego argumentu i ubija cały bieg
 *                       (`commands.golden.txt`). Przycisk o tej nazwie zatrzymywałby więc
 *                       wszystkich — kontrolka, która robi coś INNEGO niż mówi, jest gorsza
 *                       od jej braku (niezmiennik 16). Zgłoszone jako brak komendy.
 *   `Open its files`    otwarcie plików agenta nie ma dziś żadnej drogi w oknie.
 *   trzecia kolumna     `.sz` w makiecie niesie rozmiar pliku (`1.2 KB`) albo `—`. Rozmiarów
 *                       nie ma na drucie, a `—` w tej samej siatce i tym samym krojem co
 *                       prawdziwa liczba jest wierszem zastępczym, którego nie da się odróżnić
 *                       od wiersza z wartością (niezmiennik 17).
 *   wiersze jako linki  `<a href="#">` w makiecie otwiera panel szczegółów, którego to repo nie
 *                       ma. Wiersz z kształtem linku, który nie prowadzi nigdzie, jest martwą
 *                       kontrolką z dodatkowym krokiem.
 *
 * STAN AGENTA STOI W PODPISIE NAGŁÓWKA i nie jest to drugie żywe miejsce obok kafelka w liście
 * agentów: ten ekran zakrywa listę, więc te dwa napisy nigdy nie są widoczne naraz. Słowo,
 * nigdy kolor kwadratu — kwadrat jest tożsamością [DESIGN §3].
 */
import type { ReactElement } from 'react';
import { Line } from '../feed/line';
import type { RailCard } from '../rail/card';
import { statusToken } from '../rail/colour';
import type { Section } from './layout';

export interface SessionProps {
  /** Ten sam kafelek, który stoi w liście agentów — nazwa, rola, kolor, stan, jedno źródło. */
  readonly card: RailCard;
  /** Sekcje policzone przez `sessionSections()`. Kolejność jest ich kolejnością. */
  readonly sections: readonly Section[];
  /** Droga powrotna. Wymagana: ekran bez wyjścia jest pułapką, nie ekranem. */
  readonly onBack: () => void;
  /** Rozwinięcie wiersza transkryptu — ten sam handler, co w strumieniu głównym. */
  readonly onToggle: (rowId: number) => void;
  /**
   * Powtórzenie tego kroku. Brak propsu znaczy „ten ekran nie umie tego uruchomić" i wtedy
   * przycisku nie ma wcale (niezmiennik 16).
   *
   * 2026-08-23 — kontrolka stoi TUTAJ, a nie na kafelku w liście agentów, i to nie jest wybór
   * estetyczny: kafelek ma cztery wiersze tekstu i sześć pól, a te dwie liczby są mierzone
   * (`rail/card.test.ts`, `rail-shows-agents.test.tsx`) i wynikają z sufitu gęstości
   * z `docs/ARCHITECTURE.md` §7. Piąty napis w wierszu listy łamie układ przy czwartym agencie.
   * Ekran otwartego agenta jest zresztą właściwszym miejscem: człowiek klika „powtórz" wtedy,
   * gdy patrzy na to, co ten krok zrobił.
   */
  readonly onRunAgain?: () => void;
}

/** `button-quiet` z DESIGN §6, ta sama fraza co w strumieniu (`feed/feed.tsx`). */
const QUIET = 'h-7 rounded-sm border border-line px-3 text-ui text-body';

/** Wiersz bloku faktów: etykieta i wartość. Dwie kolumny, bo trzecia nie ma danych. */
function FactRow({ label, value }: { label: string; value: string }): ReactElement {
  return (
    <div
      data-fact
      className="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-[9px] border border-line bg-well px-2 py-[5px] font-mono text-mono text-body"
    >
      <span className="text-label text-muted">{label}</span>
      <span className="min-w-0 break-words">{value}</span>
    </div>
  );
}

/** Blok faktów: nagłówek i wiersze albo jedno zdanie o tym, że ich nie ma. */
function Facts({ section }: { section: Section }): ReactElement {
  return (
    <section data-facts={section.id} className="mb-4 border border-line bg-panel">
      <h2 className="border-b border-line px-3 py-[9px] font-mono text-eyebrow text-muted">
        {section.heading}
      </h2>
      <div className="grid gap-[7px] px-3 py-[11px]">
        {section.rows.map((fact) => (
          <FactRow
            key={fact.kind + fact.label + fact.value}
            label={fact.label}
            value={fact.value}
          />
        ))}
        {section.empty === null ? null : (
          <p data-empty className="text-body text-muted">
            {section.empty}
          </p>
        )}
      </div>
    </section>
  );
}

export function Session({
  card,
  sections,
  onBack,
  onToggle,
  onRunAgain,
}: SessionProps): ReactElement {
  return (
    /* `fixed inset-0`, bo ten ekran ZAKRYWA widok pracy, zamiast go przestawiać: bieg pod nim
     * idzie dalej, strumień dalej przyjmuje linie, a powrót nie kosztuje ani jednego odczytu.
     * Prawdziwy szew ekranu — rząd w siatce sekcji Bieg — należy do pliku, którego to zadanie
     * nie posiada, i jest zgłoszony razem z kształtem propsów. */
    <div
      data-agent-screen={card.id}
      className="fixed inset-0 z-10 grid grid-rows-[auto_minmax(0,1fr)] bg-bg"
    >
      <div className="flex h-13 shrink-0 items-center gap-3 border-b border-line bg-panel px-[18px]">
        <button type="button" aria-label="Back to the run" onClick={onBack} className={QUIET}>
          ←
        </button>

        {/* Napis mówi, CO SIĘ STANIE, i mówi o KROKU: powtarza się kafelek grafu, nie rola,
            która bywa w kilku miejscach naraz. */}
        {onRunAgain === undefined ? null : (
          <button type="button" data-run-again onClick={onRunAgain} className={QUIET}>
            Run this step again
          </button>
        )}

        {/* Kwadrat tożsamości — ta sama mapa agent→kolor, z której żyje kafelek i podpis
            w strumieniu (`rail/colour.ts`). `aria-hidden`, bo to nazwa jeszcze raz, skrócona
            do litery. */}
        <span
          aria-hidden
          className="grid size-[22px] place-items-center font-mono text-mono-strong text-ink"
          style={{ background: `var(${card.square})` }}
        >
          {card.name.slice(0, 1)}
        </span>

        <h1 className="text-title text-ink">{card.name}</h1>

        {card.role === '' ? null : (
          <span className="font-mono text-mono text-muted">{card.role}</span>
        )}

        <span
          data-status
          className="ml-auto font-mono text-label"
          style={{ color: `var(${statusToken(card.status)})` }}
        >
          {card.status}
        </span>
      </div>

      <div className="min-h-0 overflow-auto p-[18px]">
        {sections.map((section) =>
          section.id === 'transcript' ? (
            <section key={section.id} data-said>
              {/* Nagłówek trzeciej sekcji stoi bez ramki, jak w makiecie (`.ln.rule`): transkrypt
                  jest tym samym strumieniem co ekran pracy, więc nie ma prawa wyglądać jak
                  blok faktów. */}
              <h2 className="border-b border-line px-[18px] py-[9px] font-mono text-eyebrow text-muted">
                {section.heading}
              </h2>
              {section.empty === null ? null : (
                <p data-empty className="px-[18px] py-2 text-body text-muted">
                  {section.empty}
                </p>
              )}
              {/* TEN SAM komponent wiersza, co strumień główny, i to jest treść tezy „widok
                  agenta to ten sam strumień z filtrem" [T2 §9.1]. Drugi renderer wiersza
                  pokazywałby przy tej samej linii inny podział na grupy albo inne rozwinięcie,
                  a nic na ekranie nie mówiłoby, który z dwóch obrazów jest prawdziwy. */}
              {section.lines.map((row) => (
                <Line key={row.id} row={row} onToggle={onToggle} />
              ))}
            </section>
          ) : (
            <Facts key={section.id} section={section} />
          ),
        )}
      </div>
    </div>
  );
}
