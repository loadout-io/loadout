/* Karta przeglądu: to, co człowiek czyta, ZANIM cudza umiejętność stanie się instrukcją dla
 * agenta [T5 §5.4, §8.3].
 *
 * Jedna reguła rządzi całym tym plikiem: nieufna treść jest TEKSTEM, nigdy znacznikami. Ciało
 * przyszło z sieci i jest dokładnie tym, co dostanie model — więc na ekranie ma wyglądać tak,
 * jak wygląda w pliku, ze wszystkim, co ktoś w nim schował. Wstrzyknięty `<script>` wykonany
 * w oknie aplikacji jest drugim atakiem, dołożonym za darmo do pierwszego. React ucieka znaki
 * we wszystkim, co wstawiamy jako dziecko węzła, i to jest jedyny mechanizm, na którym tu
 * stoimy — `dangerouslySetInnerHTML` nie ma prawa pojawić się w tym pliku ani obok.
 *
 * 2026-08-31 — LISTA ZNALEZISK WYSZŁA STĄD DO `./findings.tsx` i nie jest to porządkowanie.
 * Odkąd `ImportItem` niesie przegląd drutem, ekran importu pokazuje te same znaleziska tym
 * samym ludziom; zdania o regułach, cytat i tekst odzyskany z komentarza zostały więc w JEDNYM
 * miejscu (niezmiennik 23). Ta karta dokłada do nich to, czego tamten ekran nie ma: nazwę,
 * pochodzenie, ciało za `<details>` i zgodę, bez której nie ma instalacji.
 *
 * Drugi kierunek jest tak samo wiążący: karta, która ciała NIE POKAZUJE, przechodzi każde
 * sprawdzenie mówiące „nie ma tu znaczników" i jednocześnie kasuje jedyny powód, dla którego
 * ten ekran istnieje — człowiek zatwierdza wtedy w ciemno.
 *
 * Czysta funkcja propsów na markup, jak `SkillsRow`: bez własnego stanu i bez `invoke()`.
 * Odmowa instalacji mieszka w magazynie (`src/state/skills.ts`), nie tutaj — wyłączony przycisk
 * jest sugestią, a nie mechanizmem.
 *
 * DLACZEGO `<details>`, A NIE PRZYCISK ZE STANEM. Rozwijanie jest zachowaniem przeglądarki,
 * więc nie potrzebuje ani handlera, ani stanu wyżej (niezmiennik 16: kontrolka bez handlera nie
 * wchodzi do repo). Ciało siedzi w drzewie od pierwszego renderu, a nie jest dorysowywane po
 * kliknięciu — to, czy człowiek je widzi, rozstrzyga tu przeglądarka, nie arkusz stylów.
 */
import type { ReactElement } from 'react';
import type { Import } from '../../state/skills';
import { Findings, QUOTE } from './findings';

export interface ReviewCardProps {
  item: Import;
  /** Identyfikatory znalezisk, które człowiek już przeczytał. */
  acknowledged: readonly string[];
  onAcknowledge: (findingId: string) => void;
  onAdd: () => void;
}

/* TRZY STAŁE KLASOWE ZNIKŁY 2026-08-31 (DESIGN §6, warstwa prymitywów).
 *
 * `CHIP` opisywał pigułkę pochodzenia własnymi sześcioma nazwami; dziś to `.chip` z tonem
 * podanym atrybutem (`data-tone="attend"`), bo ton chipa zmienia wyłącznie barwę, a nie
 * geometrię — a `.chip-attend` obok `.chip` to dwa napisy, które trzeba trzymać zgodnie ręcznie.
 * `READ` był cichym przyciskiem, czyli `.btn-quiet`.
 *
 * `ADD` i `ADD_OFF` znikły OBA i to jest cały sens tej warstwy: stan wyłączony jest REGUŁĄ
 * (`.btn-primary:disabled` w `theme.css`), a nie drugim przyciskiem. Powód, dla którego nie ma
 * tu wariantu `disabled:` Tailwinda, nie zmienił się ani o słowo: wariant zostawia napis
 * `disabled` w atrybucie `class` także wtedy, gdy przycisk działa, więc „czy da się dodać" ma
 * w HTML-u dwie odpowiedzi, z których jedna kłamie (niezmiennik 13). Dziś odpowiedź jest jedna
 * i nosi ją atrybut `disabled`, który ten stan naprawdę egzekwuje.
 *
 * `QUOTE` ZOSTAJE i nie jest przeoczeniem — przeprowadził się tylko do `./findings.tsx`, bo
 * ta sama studnia stoi pod cytatem znaleziska. To jest blok nieufnej treści: tekst, który
 * człowiek CZYTA, żeby zdecydować. Prymitywu na to nie ma — `.card` jest pojemnikiem na tonie
 * panelu, a `.value` niesie tabelaryczne cyfry i rolę wartości maszynowej, nie cudzego
 * akapitu. Zgłoszone jako brakująca rola, nie obchodzone klasą o innym znaczeniu. */

/** „Includes N scripts" [T5 §8.3] — liczba jest liczona z tego, co przyszło. */
function scriptsLine(count: number): string {
  return count === 1
    ? 'Includes 1 script — it will not run unless an agent chooses to run it.'
    : 'Includes ' +
        String(count) +
        ' scripts — these will not run unless an agent chooses to run them.';
}

export function ReviewCard({
  item,
  acknowledged,
  onAcknowledge,
  onAdd,
}: ReviewCardProps): ReactElement {
  const waiting = item.reviewed.findings.filter(
    (finding) => finding.weight === 'block' && !acknowledged.includes(finding.id),
  );

  return (
    <section data-review-card className="stack" data-gap="3">
      <header className="flex items-center gap-2">
        <h2 className="text-heading text-ink">{item.name}</h2>
        {/* Znacznik pochodzenia stoi na karcie i zostaje po instalacji. To jedyna rzecz tutaj,
            która mówi, że ten tekst napisał ktoś obcy.

            2026-08-19 — WARUNEK, BO DO TEGO DNIA PLAKIETKA BYŁA WPISANA NA SZTYWNO i ignorowała
            `item.fromTheInternet`. Była to prawda przez konstrukcję: jedyną drogą, którą
            cokolwiek wchodziło do tej karty, było `review_skill(url)`, czyli link. Od chwili,
            w której człowiek może napisać umiejętność sam, to samo zdanie mówi o JEGO tekście,
            że przyszedł od obcego — a plakietka zastępuje w v1 podpisy i weryfikację
            pochodzenia, których nie ma. Zdanie zawsze prawdziwe nie niesie informacji; zdanie
            czasem nieprawdziwe uczy je ignorować, i to jest droższe z dwojga.

            Zgaszenie jej CAŁKIEM byłoby drugą połową tego samego defektu: umiejętność napisana
            przez obcego przestałaby różnić się od napisanej ręką. Dlatego warunek, a nie
            usunięcie. */}
        {item.fromTheInternet ? (
          <span className="chip" data-tone="attend">
            From the internet
          </span>
        ) : null}
      </header>

      {/* Bez klasy: `--t-body` jest stopniem prozy i `body` już go ma. Stało tu
          `text-body text-body`, co czytało się jak literówka i nią było (DESIGN §6). */}
      <p>{item.summary}</p>

      <details className="rounded-md border border-line p-2">
        <summary className="text-ui text-body">Show what it tells the agent to do</summary>
        <pre className={QUOTE}>{item.reviewed.body}</pre>
      </details>

      {/* Zgoda per znalezisko jedzie propsem, bo na tym ekranie ona coś zmienia: instalacja
          czeka, aż każde blokujące zostanie odklikane. Ekran importu tej zgody nie ma i podaje
          w to miejsce zdanie — powód stoi w nagłówku `./findings.tsx`. */}
      <Findings
        findings={item.reviewed.findings}
        acknowledged={acknowledged}
        onAcknowledge={onAcknowledge}
      />

      {item.scripts > 0 ? <p>{scriptsLine(item.scripts)}</p> : null}

      <div className="flex items-center gap-2">
        <button
          type="button"
          data-add
          disabled={waiting.length > 0}
          className="btn-primary"
          onClick={onAdd}
        >
          Add this skill
        </button>
      </div>
    </section>
  );
}
