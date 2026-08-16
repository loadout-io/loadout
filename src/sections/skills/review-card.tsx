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
import type { Finding, Import } from '../../state/skills';

export interface ReviewCardProps {
  item: Import;
  /** Identyfikatory znalezisk, które człowiek już przeczytał. */
  acknowledged: readonly string[];
  onAcknowledge: (findingId: string) => void;
  onAdd: () => void;
}

/* Zdanie na każdą regułę. Id reguły NIGDY nie trafia na ekran (niezmiennik 14): nazywa
 * sprawdzenie, a nie niebezpieczeństwo — a człowiek, który czyta `role-manipulation`, wie
 * dokładnie tyle, ile wiedział przedtem. Nieznana reguła (skaner przyniósł swoją) dostaje
 * zdanie ogólne zamiast wypaść z listy: znalezisko bez tłumaczenia dalej jest znaleziskiem. */
const SAYS: Readonly<Record<string, string>> = {
  'hidden-text': 'This skill carries text you cannot see on screen.',
  'instruction-override': 'A line here tells the agent to drop the rules it was given.',
  exfiltration: 'A line here sends something off this machine.',
  'role-manipulation': 'A line here is written to look like part of the conversation.',
  escalation: 'This skill asks for tools of its own.',
  'deep-scan-unavailable': "Deep scan didn't run.",
};

const OTHERWISE = 'There is something here worth reading before you add this skill.';

const CHIP = 'rounded-sq border border-attend-edge bg-attend-wash px-2 text-label text-attend';
const QUOTE = 'overflow-x-auto rounded-sq bg-well p-2 text-mono text-ink';
const READ = 'h-7 rounded-sq border border-line px-3 text-ui text-body';

/* Klasa przycisku „Add" zależy od stanu i jest wybierana TUTAJ, a nie wariantem `disabled:`
 * Tailwinda. Wariant zostawiłby słowo `disabled` w atrybucie `class` także wtedy, gdy przycisk
 * działa — czyli „czy da się dodać" miałoby w HTML-u dwie odpowiedzi, z których jedna kłamie
 * (niezmiennik 13). */
const ADD = 'h-9 rounded-sq bg-accent px-4 text-ui text-bg';
const ADD_OFF = 'h-9 rounded-sq bg-raised px-4 text-ui text-muted';

function sentenceFor(finding: Finding): string {
  return SAYS[finding.rule] ?? OTHERWISE;
}

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
    <section data-review-card className="flex flex-col gap-3">
      <header className="flex items-center gap-2">
        <h2 className="text-heading text-ink">{item.name}</h2>
        {/* Znacznik pochodzenia stoi na karcie i zostaje po instalacji. To jedyna rzecz tutaj,
            która mówi, że ten tekst napisał ktoś obcy. */}
        <span className={CHIP}>From the internet</span>
      </header>

      <p className="text-body text-body">{item.summary}</p>

      <details className="rounded-sq border border-line p-2">
        <summary className="text-ui text-body">Show what it tells the agent to do</summary>
        <pre className={QUOTE}>{item.reviewed.body}</pre>
      </details>

      {item.reviewed.findings.length > 0 ? (
        <ul className="flex flex-col gap-2">
          {item.reviewed.findings.map((finding) => (
            <li key={finding.id} className="flex flex-col gap-1">
              <span className="text-body text-ink">{sentenceFor(finding)}</span>
              {finding.line === null ? null : (
                <span className="text-label text-muted">{`Line ${String(finding.line)}`}</span>
              )}
              {/* Cytat i tekst odzyskany z komentarza jadą jako dzieci węzła, więc React
                  ucieka w nich znaki. Odzyskany tekst MUSI tu być: został zdjęty z ciała, więc
                  jeśli karta go nie pokaże, atak zniknie z ekranu i pojedzie do modelu. */}
              {finding.quoted.length > 0 ? <pre className={QUOTE}>{finding.quoted}</pre> : null}
              {finding.recovered === null ? null : <pre className={QUOTE}>{finding.recovered}</pre>}
              {finding.weight === 'block' && !acknowledged.includes(finding.id) ? (
                <button
                  type="button"
                  data-acknowledge={finding.id}
                  className={READ}
                  onClick={() => {
                    onAcknowledge(finding.id);
                  }}
                >
                  I have read this
                </button>
              ) : null}
            </li>
          ))}
        </ul>
      ) : null}

      {item.scripts > 0 ? <p className="text-body text-body">{scriptsLine(item.scripts)}</p> : null}

      <div className="flex items-center gap-2">
        <button
          type="button"
          data-add
          disabled={waiting.length > 0}
          className={waiting.length > 0 ? ADD_OFF : ADD}
          onClick={onAdd}
        >
          Add this skill
        </button>
      </div>
    </section>
  );
}
