/* Jedna wypowiedź w strumieniu — wiersz `.msg` z makiety `polecenie.html`.
 *
 * # Po co to istnieje obok `./line.tsx`
 *
 * Bo to są dwie różne powierzchnie do czytania, nie dwa style tej samej. `Line` jest WIERSZEM
 * TRANSKRYPTU: gęsta siatka `18px 1fr auto`, po pięć pikseli na wiersz, i mieszka w szufladzie
 * kroku, w panelu historii biegów i na ekranie jednego agenta — tam, gdzie człowiek przegląda
 * setki linii naraz. `Message` jest PIERWSZĄ powierzchnią biegu: kolumną, w którą człowiek patrzy,
 * kiedy praca się dzieje, i w której przez większość czasu stoi kilkanaście zdań, nie tysiąc.
 *
 * Zmierzona przyczyna, dla której nie wystarczył jeden: czterech agentów mówiących naraz było
 * w tamtej siatce czterema identycznymi szarymi akapitami. Podpis mieścił się w niej jako
 * przedrostek w tym samym wierszu co zdanie, więc oko nie miało za co złapać ani „kto", ani
 * „kiedy" — a to jest jedyne pytanie, które ma się dać odpowiedzieć bez czytania.
 *
 * ŻADNA POLITYKA NIE MIESZKA TUTAJ DRUGI RAZ. Barwa tożsamości idzie z `../rail/colour.ts`,
 * „kto mówi" z `../rail/say.ts`, inicjały i zegar z `./who.ts`, rozbiór propozycji z
 * `./suggested.ts`, a proza z `./answer.tsx`. Ten plik rozstrzyga wyłącznie UKŁAD — i to jest
 * jedyna rzecz, którą różni się od `./line.tsx`.
 */
import { type ReactElement, useState } from 'react';
import { identityToken } from '../rail/colour';
import { authorityOf } from '../rail/say';
import { Answer, AnswerLine } from './answer';
import type { HistoryRow } from './model';
import { runSuggestion, suggestion } from './suggested';
import { clockOf, initialsOf } from './who';

export interface MessageProps {
  row: HistoryRow;
  /** Wymagany: `+` bez tego jest ozdobą w kształcie przycisku (niezmiennik 16). */
  onToggle: (rowId: number) => void;
  /**
   * Komenda, którą przyniósł wiersz propozycji — znak w znak taka, jaką napisał lider.
   * Powód, dla którego jedzie propsem, a nie jest czytana z wiersza, stoi przy `LineProps`
   * w `./line.tsx`: wiersz jest jedynym miejscem, w którym ta komenda mieszka.
   */
  command?: string | undefined;
}

/**
 * Czytelna miara wiersza — **wartość z makiety** (`.ln.note .t{max-width:64ch}`, `.msg p`).
 *
 * Ta sama liczba stoi w `./line.tsx` i jest tam opisana w całości. Oba pliki biorą ją z tej
 * samej reguły makiety, a `./prose-keeps-the-mockups-measure.test.tsx` czyta ją stamtąd
 * w każdym biegu — więc rozjazd któregokolwiek z nich z makietą jest czerwony.
 */
const MEASURE = 'max-w-[64ch]';

/** Rodzaje, których treść jest PROZĄ do przeczytania, a nie etykietą czynności. */
const PROSE: readonly string[] = ['note', 'told', 'suggested', 'asked', 'handoff'];

/**
 * Kwadrat z inicjałami w barwie tożsamości tego agenta.
 *
 * PRZYGASZONY, NIGDY NASYCONY [DESIGN §3 „Tożsamość ≠ stan"]. Makieta rysuje ten kwadrat
 * nasyconym gradientem, i tego jednego z niej nie bierzemy: nasycone barwy w tej aplikacji
 * odpowiadają na pytanie „co się dzieje", a kwadrat odpowiada na „kto". Referencyjny poprzedni prototyp
 * dał agentowi dokładnie ten pomarańcz, który na sąsiednim kafelku znaczył „czeka na twoją
 * decyzję", i to jest zdarzenie, z którego ta reguła powstała.
 *
 * `color-mix` zamiast drugiego tokenu na wypełnienie: tło jest tą samą barwą, tylko rzadszą,
 * a nazwa koloru zostaje jedna (niezmiennik 13). Hex w komponencie jest zakazany [DESIGN §9]
 * i nie pada tu ani razu.
 */
function Signature({ agent }: { agent: string }): ReactElement {
  /* Nazwa zmiennej celowo NIE brzmi „token": `checks/vocabulary.sh` skanuje KAŻDY napis prozy,
     a `var(${…})` wkleja identyfikator do literału napisowego — słowo z rejestru vendorów zapaliłoby
     go tam, gdzie mowa o barwie. */
  const hue = identityToken(agent);
  return (
    <span
      data-sig={initialsOf(agent)}
      aria-hidden="true"
      className="grid h-[26px] w-[26px] shrink-0 place-items-center rounded-sm border font-mono text-meta font-semibold"
      style={{
        color: `var(${hue})`,
        borderColor: `color-mix(in srgb, var(${hue}) 46%, transparent)`,
        background: `color-mix(in srgb, var(${hue}) 18%, transparent)`,
      }}
    >
      {initialsOf(agent)}
    </span>
  );
}

/**
 * Kreska z podpisem biegu — wiersz `run` narysowany jako `.startline` z makiety.
 *
 * OSOBNY KSZTAŁT, bo to nie jest niczyja wypowiedź: `run` mówi, że bieg się zaczął, i podpisany
 * kwadratem agenta przypisywałby ten fakt komuś, kto go nie ogłosił. W makiecie jest to
 * wycentrowany napis między dwiema włoskowatymi kreskami i dokładnie tak czyta się w strumieniu:
 * jako granica, nie jako zdanie.
 */
function StartLine({ row }: { row: HistoryRow }): ReactElement {
  return (
    <div
      data-line={row.id}
      data-start-line
      className="fade-in flex items-center gap-3 px-[18px] py-2"
    >
      <span className="h-px flex-1 bg-line" />
      <span className="value whitespace-nowrap">
        {row.label} · <time data-at={row.at}>{clockOf(row.at)}</time>
      </span>
      <span className="h-px flex-1 bg-line" />
    </div>
  );
}

export function Message({ row, onToggle, command }: MessageProps): ReactElement {
  /* DWIE RZECZY MOGĄ STAĆ ZA WIERSZEM i jedna kontrolka je otwiera — powód w całości
     w `./line.tsx`. */
  const hasMore = row.output.length > 0 || row.body.length > 0;
  const proposes = row.kind === 'suggested' && command !== undefined ? suggestion(command) : null;
  /** Zdanie, które wróciło z próby uruchomienia; `null`, dopóki nie ma o czym mówić. */
  const [said, setSaid] = useState<string | null>(null);
  const yours = authorityOf(row.kind) === 'you';

  if (row.kind === 'run') return <StartLine row={row} />;

  return (
    /* WCHODZI PRZEZ ROZJAŚNIENIE, nie skokiem: strumień przyrasta wierszami, a element, który
       jeszcze DOJEŻDŻA, przesuwałby zdanie spod oczu w chwili, w której człowiek zaczyna je
       czytać (DESIGN §7). */
    <article data-line={row.id} data-message className="fade-in flex gap-3 px-[18px] py-[7px]">
      {/* TWOJE ZDANIE JEST PODPISANE TOBĄ, nie agentem. Wiersz `told` niesie w polu `agent`
          ADRESATA, więc kwadrat z jego barwą przypisywałby Twoje słowa komuś innemu — pełny
          powód stoi przy tej samej gałęzi w `./line.tsx`. */}
      {yours ? (
        <span
          data-sig="You"
          aria-hidden="true"
          className="grid h-[26px] w-[26px] shrink-0 place-items-center rounded-sm border border-human-edge bg-human-soft font-mono text-meta font-semibold text-human"
        >
          You
        </span>
      ) : (
        <Signature agent={row.agent} />
      )}

      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          {/* KTO MÓWI, w barwie tożsamości tego agenta — ta sama mapa, z której żyje kwadrat
              obok i kafelek na liście agentów. Kwadrat jest pomocą dla oka i przy szóstym
              agencie barwy zaczynają się powtarzać z rozmysłu; nazwa obok jest tym, co je
              rozróżnia zawsze. */}
          <b
            data-who={yours ? 'You' : row.agent}
            className="text-subhead"
            style={
              yours
                ? { color: 'var(--color-human)' }
                : { color: `var(${identityToken(row.agent)})` }
            }
          >
            {yours ? 'You → ' + row.agent : row.agent}
          </b>
          <time data-at={row.at} className="value text-meta">
            {clockOf(row.at)}
          </time>
          {/* Prawa strona nagłówka: liczba, którą ta czynność zostawiła po sobie. Nigdy razem
              z kontrolką — dwie rzeczy wyglądające na klikalne w jednym wierszu nie mówią,
              która z nich zaczyna pracę. */}
          {proposes !== null || hasMore ? null : (
            <span className="value ml-auto whitespace-nowrap">{row.metric}</span>
          )}
          {proposes === null ? null : (
            <button
              type="button"
              data-tone="accent"
              className="chip ml-auto"
              onClick={() => {
                /* JEDNA DROGA STARTU, ta sama, co Enter w wierszu wejścia (niezmiennik 23).
                   Wynik NIE jest porzucany — odmowa wraca tu i staje pod wierszem. */
                void runSuggestion(command ?? '').then(setSaid);
              }}
            >
              {'Run ' + proposes.workflow}
            </button>
          )}
          {proposes !== null || !hasMore ? null : (
            <button
              type="button"
              onClick={() => {
                onToggle(row.id);
              }}
              aria-label={row.expanded ? 'Show less' : 'Show more'}
              className="chip ml-auto"
            >
              {row.expanded ? '−' : '+'}
            </button>
          )}
        </div>

        {/* ZDANIE AGENTA. Miarę wiersza dostaje wyłącznie proza — etykieta czynności zawinięta
            w połowie kolumny czyta się gorzej, nie lepiej. Markdownem, bo tak agenci piszą
            naprawdę; renderer składa ELEMENTY z tokenów i nigdy nie tyka HTML-a (`./answer.tsx`). */}
        <p
          data-said
          className={`mt-[3px] whitespace-pre-line break-words text-body text-ink${
            PROSE.includes(row.kind) ? ' ' + MEASURE : ''
          }`}
        >
          {PROSE.includes(row.kind) ? <AnswerLine text={row.label} /> : row.label}
        </p>

        {row.expanded && row.body.length > 0 ? (
          <div data-line-body className={`mt-1 text-body ${MEASURE}`}>
            <Answer text={row.body.join('\n')} />
          </div>
        ) : null}

        {/* MIARA WIERSZA JEST Z DRABINKI, nie z liczby wpisanej w klasę (2026-08-31). Stało tu
            `leading-[1.75]` — makieta daje temu blokowi `line-height:1.7` — i `checks/tokens.sh`
            słusznie to zapala: wartość w nawiasach kwadratowych jest tą samą ucieczką, co literał
            w CSS, tylko schowaną w nazwie klasy. Drabinka typografii nie ma dziś stopnia luźniejszego
            niż `--text-mono--line-height` (1.45), który niesie `.value`, więc blok czyta się o tyle
            ciaśniej niż makieta. Stopnia nie dopisuję: nowy wiersz w drabince jest decyzją z DESIGN §4,
            a nie naprawą bramki. */}
        {/* WYNIK SPRAWDZEŃ — blok `.term` z makiety: mono, lewa kreska, do zaznaczenia
            i skopiowania. To jest wyjście narzędzia, znak w znak takie, jakie napisało ono samo;
            okno nie liczy tu ani jednej z tych liczb i nie ma z czego (niezmiennik 17: na drucie
            stoi `detail: Vec<String>`, nie licznik zdanych i niezdanych). Lewa krawędź
            w `--color-fail` jest jedyną rzeczą, która odróżnia ten blok od zwykłego akapitu
            tekstu maszynowego. */}
        {row.expanded && row.output.length > 0 ? (
          <pre
            data-copyable
            data-checks
            className="value mt-2 overflow-x-auto border-l-2 border-l-fail py-[7px] pl-3"
          >
            {row.output.join('\n')}
          </pre>
        ) : null}

        {said === null ? null : (
          <p data-line-said data-tone="fail" className="lead mt-1">
            {said}
          </p>
        )}
      </div>
    </article>
  );
}

export interface AnsweredProps {
  /** Komu ta odpowiedź poszła — podpis, pod którym agent zadał pytanie. */
  agent: string;
  /** Co człowiek wybrał, znak w znak. */
  option: string;
}

/**
 * Co CZŁOWIEK odpowiedział — wiersz strumienia pod pytaniem, na które padła ta odpowiedź.
 *
 * # Zmierzona wada, którą to naprawia
 *
 * Odpowiedź nie zostawiała po sobie ANI JEDNEGO śladu na ekranie. `Feed.answer()` zapisuje ją
 * w `view.answers` i zdejmuje przypięcie — więc po naciśnięciu `1` karta znikała, a strumień
 * wyglądał dokładnie tak, jak przed naciśnięciem. Kontrolka, po której nie widać nic, jest
 * nieodróżnialna od zepsutej (DESIGN §8), a ta puszcza dalej bieg za pieniądze. Do tego wybrana
 * opcja jest JEDYNYM zapisem tego, w którą stronę bieg został skierowany: bez niej transkrypt
 * biegu nie odpowiada na pytanie „dlaczego poszło tędy".
 *
 * # Czego ten wiersz nie ma i dlaczego
 *
 * Zegara. `Answer` w `./model.ts` niesie trzy pola — pytanie, opcję i kto — i ani jednego
 * stempla. Godzina przepisana z pytania byłaby czasem, w którym agent ZAPYTAŁ, postawionym pod
 * zdaniem, które padło później; godzina odczytana przy renderze zmieniałaby się przy każdym
 * przerysowaniu. Obie są liczbą, której nikt nie zmierzył (niezmiennik 17), więc nie ma żadnej.
 */
export function Answered({ agent, option }: AnsweredProps): ReactElement {
  return (
    <article data-answered={agent} className="fade-in flex gap-3 px-[18px] py-[7px]">
      <span
        data-sig="You"
        aria-hidden="true"
        className="grid h-[26px] w-[26px] shrink-0 place-items-center rounded-sm border border-human-edge bg-human-soft font-mono text-meta font-semibold text-human"
      >
        You
      </span>
      <div className="min-w-0 flex-1">
        <b data-who="You" className="text-subhead" style={{ color: 'var(--color-human)' }}>
          {'You → ' + agent}
        </b>
        <p className={`mt-[3px] break-words text-body text-ink ${MEASURE}`}>{option}</p>
      </div>
    </article>
  );
}
