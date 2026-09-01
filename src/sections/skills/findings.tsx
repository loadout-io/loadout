/* Znaleziska przeglądu: jedyne miejsce, w którym znalezisko dostaje SŁOWA — i jedyne, w którym
 * cudza linia trafia na ekran.
 *
 * # Dlaczego to wyszło z karty przeglądu (2026-08-31)
 *
 * `SKILL.md` wchodzi do produktu dwiema drogami i obie kończą się tą samą decyzją człowieka:
 * wklejony linkiem staje przed kartą przeglądu (`./review-card.tsx`), znaleziony w cudzym
 * projekcie — przed wierszem w „Import setup" (`src/sections/import/setup.tsx`). Do tego dnia
 * druga droga nie miała czego pokazać, bo znaleziska nie miały drutu do okna; odkąd mają
 * (`ImportItem::reviewed`), drugą kopią tej listy byłoby drugie miejsce, w którym znalezisko
 * się numeruje, tłumaczy i cytuje (niezmiennik 23). Dlatego lista jest tutaj, a nie tam.
 *
 * # Jedna reguła rządzi całym tym plikiem
 *
 * Nieufna treść jest TEKSTEM, nigdy znacznikami. Cytat i tekst odzyskany z komentarza przyszły
 * z cudzego pliku i są dokładnie tym, co dostanie model — więc na ekranie mają wyglądać tak,
 * jak wyglądają w pliku, ze wszystkim, co ktoś w nich schował. Wstrzyknięty `<script>` wykonany
 * w oknie aplikacji jest drugim atakiem, dołożonym za darmo do pierwszego. React ucieka znaki
 * we wszystkim, co wstawiamy jako dziecko węzła, i to jest jedyny mechanizm, na którym tu
 * stoimy — `dangerouslySetInnerHTML` nie ma prawa pojawić się w tym pliku ani obok.
 *
 * Drugi kierunek jest tak samo wiążący: lista, która cytatu NIE POKAZUJE, przechodzi każde
 * sprawdzenie mówiące „nie ma tu znaczników" i jednocześnie kasuje jedyny powód, dla którego
 * ta lista istnieje — człowiek zatwierdza wtedy w ciemno.
 *
 * # Waga znaleziska ma nośnik na obu ekranach, ale nie ten sam
 *
 * `Weight::Block` znaczy dwie różne rzeczy w dwóch różnych miejscach i dlatego nie da się jej
 * opisać jednym napisem wpisanym tutaj na sztywno. W sekcji Umiejętności instalacja CZEKA, aż
 * człowiek to odklika — więc nośnikiem jest przycisk („I have read this"), a zgoda jest per
 * znalezisko. W imporcie nie czeka nic: umiejętność z blokującym znaleziskiem jest po stronie
 * Rusta `Unsupported` i nie wchodzi wcale, cokolwiek człowiek kliknie — więc przycisk byłby
 * kontrolką bez skutku (niezmiennik 16), a nośnikiem jest zdanie, które ten ekran podaje sam.
 *
 * Stąd dwa propsy, które się WYKLUCZAJĄ: `onAcknowledge` dla ekranu, który zgodę ma, i
 * `blockingSays` dla ekranu, który jej nie ma. Blokujące znalezisko bez żadnego z nich stoi
 * nieodróżnialne od ostrzegawczego — i to jest jedyny stan, którego ten plik nie powinien
 * zobaczyć.
 */
import type { ReactElement } from 'react';
import type { Finding } from '../../state/skills';

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

/** Napis pod blokującym znaleziskiem tam, gdzie zgoda człowieka coś zmienia.
 *
 * Stąd, a nie z dwóch plików: import wypisuje te same słowa nad całą pozycją
 * (`src/sections/import/skill-review.ts`), a dwa napisy na jedną czynność uczą, że to dwie
 * różne czynności. */
export const READ_IT = 'I have read this';

/* Blok nieufnej treści — studnia pod tekstem, który człowiek CZYTA, żeby zdecydować.
 * Prymitywu na to nie ma: `.card` jest pojemnikiem na tonie panelu, a `.value` niesie
 * tabelaryczne cyfry i rolę wartości maszynowej, nie cudzego akapitu. Zgłoszone jako brakująca
 * rola, nie obchodzone klasą o innym znaczeniu. */
export const QUOTE = 'overflow-x-auto rounded-md bg-well p-2 text-mono text-ink';

/** Zdanie dla człowieka o tym jednym znalezisku.
 *
 * BEZ `export`, i to jest część reguły „jedno miejsce": eksport zaprasza drugi ekran, żeby
 * ułożył sobie tę listę po swojemu, a wtedy zdania są wspólne i wszystko inne — cytat, numer
 * linii, kolejność — już nie. Kto potrzebuje tych zdań, bierze `Findings`. */
function sentenceFor(finding: Finding): string {
  return SAYS[finding.rule] ?? OTHERWISE;
}

export interface FindingsProps {
  findings: readonly Finding[];
  /** Identyfikatory znalezisk, które człowiek już przeczytał. Bez `onAcknowledge` bez znaczenia. */
  acknowledged?: readonly string[];
  /** Podaje ekran, na którym odklikanie NAPRAWDĘ coś zmienia. */
  onAcknowledge?: (findingId: string) => void;
  /** Podaje ekran, na którym nie zmienia nic — zdaniem o tym, co blokujące znalezisko robi TAM. */
  blockingSays?: string;
}

/** Lista znalezisk: co znaleziono, w której linii, i co dokładnie tam stało. */
export function Findings({
  findings,
  acknowledged = [],
  onAcknowledge,
  blockingSays,
}: FindingsProps): ReactElement | null {
  if (findings.length === 0) return null;
  return (
    <ul className="stack" data-gap="2">
      {findings.map((finding) => (
        <li key={finding.id} className="stack">
          <span className="text-ink">{sentenceFor(finding)}</span>
          {finding.line === null ? null : (
            <span className="label">{`Line ${String(finding.line)}`}</span>
          )}
          {/* Cytat i tekst odzyskany z komentarza jadą jako dzieci węzła, więc React ucieka
              w nich znaki. Odzyskany tekst MUSI tu być: został zdjęty z ciała, więc jeśli lista
              go nie pokaże, atak zniknie z ekranu i pojedzie do modelu. */}
          {finding.quoted.length > 0 ? <pre className={QUOTE}>{finding.quoted}</pre> : null}
          {finding.recovered === null ? null : <pre className={QUOTE}>{finding.recovered}</pre>}
          {finding.weight !== 'block' ? null : onAcknowledge === undefined ? (
            blockingSays === undefined ? null : (
              <span className="lead block">{blockingSays}</span>
            )
          ) : acknowledged.includes(finding.id) ? null : (
            <button
              type="button"
              data-acknowledge={finding.id}
              className="btn-quiet"
              onClick={() => {
                onAcknowledge(finding.id);
              }}
            >
              {READ_IT}
            </button>
          )}
        </li>
      ))}
    </ul>
  );
}
