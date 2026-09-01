/* Czy kursor stoi w małpce, a jeśli tak — w jakiej.
 *
 * # Po co osobny plik
 *
 * To jedyna część podpowiedzi ścieżek, która nie potrzebuje ani przeglądarki, ani dysku: bierze
 * napis i pozycję, oddaje mention albo nic. Kryteria mogą więc pytać o zachowanie, a nie o to, co
 * się wyrenderowało — a wyrenderowana lista jest zgodna z tym, co ta funkcja powie, bo komponent
 * nie ma drugiego zdania na ten temat (niezmiennik 13).
 *
 * # Co jest małpką, a co nią nie jest
 *
 * Adres e-mail w wiadomości nie ma prawa otwierać listy plików. Dlatego `@` liczy się WYŁĄCZNIE
 * na początku tekstu albo po odstępie — `jakub@konghq.com` nie ma odstępu przed małpką, więc nie
 * jest tokenem. Odstęp W ŚRODKU tokenu też go kończy: człowiek, który napisał `@src coś`, już
 * skończył wskazywać miejsce i pisze zdanie.
 */

/** Małpka, w której stoi kursor. */
export interface AtMention {
  /** Gdzie w tekście stoi sama małpka. */
  readonly at: number;
  /** Co człowiek napisał po niej, bez małpki. To jest zapytanie do podpowiedzi. */
  readonly typed: string;
}

/** Małpka, w której stoi kursor, albo `null`, jeśli kursor nie jest w żadnym. */
export function mentionAt(text: string, caret: number): AtMention | null {
  const to = Math.max(0, Math.min(caret, text.length));
  const at = text.lastIndexOf('@', to - 1);
  if (at === -1) return null;

  const before = at === 0 ? '' : text.charAt(at - 1);
  if (before !== '' && !/\s/.test(before)) return null;

  const typed = text.slice(at + 1, to);
  if (/\s/.test(typed)) return null;

  return { at, typed };
}

/** Ten sam tekst z małpką zastąpioną wybraną ścieżką, plus miejsce dla kursora. */
export function chosen(
  text: string,
  mention: AtMention,
  path: string,
): { readonly text: string; readonly caret: number } {
  /* Katalog kończy się `/` i kursor zostaje TUŻ ZA nim, bo następne, co człowiek zrobi, to
   * najczęściej wejście głębiej. Plik jest końcem drogi, więc dostaje odstęp i kursor za nim —
   * inaczej pierwsze napisane słowo skleiłoby się z nazwą pliku. */
  const tail = path.endsWith('/') ? '' : ' ';
  const from = text.slice(0, mention.at);
  const rest = text.slice(mention.at + 1 + mention.typed.length);
  return { text: `${from}${path}${tail}${rest}`, caret: from.length + path.length + tail.length };
}
