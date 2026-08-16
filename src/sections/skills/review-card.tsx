/* Karta przeglądu: to, co człowiek czyta, ZANIM cudza umiejętność stanie się instrukcją dla
 * agenta [T5 §5.4, §8.3].
 *
 * Jedna reguła rządzi całym tym plikiem: nieufna treść jest TEKSTEM, nigdy znacznikami. Ciało
 * przyszło z sieci i jest dokładnie tym, co dostanie model — więc na ekranie ma wyglądać tak,
 * jak wygląda w pliku, ze wszystkim, co ktoś w nim schował. Wstrzyknięty `<script>` wykonany
 * w oknie aplikacji jest drugim atakiem, dołożonym za darmo do pierwszego.
 *
 * Drugi kierunek jest tak samo wiążący: karta, która ciała NIE POKAZUJE, przechodzi każde
 * sprawdzenie mówiące „nie ma tu znaczników" i jednocześnie kasuje jedyny powód, dla którego
 * ten ekran istnieje — człowiek zatwierdza wtedy w ciemno.
 *
 * Czysta funkcja propsów na markup, jak `SkillsRow`: bez własnego stanu i bez `invoke()`.
 * Odmowa instalacji mieszka w magazynie (`src/state/skills.ts`), nie tutaj — wyłączony przycisk
 * jest sugestią, a nie mechanizmem.
 *
 * Ciało jest jeszcze puste i ma paść na asercji, nie na braku modułu (AGENTS.md §2a).
 */
import type { ReactElement } from 'react';
import type { Import } from '../../state/skills';

export interface ReviewCardProps {
  item: Import;
  /** Identyfikatory znalezisk, które człowiek już przeczytał. */
  acknowledged: readonly string[];
  onAcknowledge: (findingId: string) => void;
  onAdd: () => void;
}

export function ReviewCard(_props: ReviewCardProps): ReactElement {
  return <></>;
}
