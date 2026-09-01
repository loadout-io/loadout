/* Gdzie w wiadomości stoi wklejony obraz.
 *
 * Do 2026-09-01 obraz był listą OBOK tekstu: `EntryDraft` trzymał `text` i `images`, a wiadomość
 * mówiła agentowi „masz tu trzy obrazy" bez ani jednego słowa o tym, którego zdania dotyczą.
 * Właściciel: „chce miec w tekscie miejsce ze akurat tam zostalo dodane bo czesto odnosze sie do
 * danego miejsca". Zdanie „tu przycisk jest ucięty" nie znaczy nic, dopóki „tu" nie ma adresu.
 *
 * # Numer bierze się z KOLEJNOŚCI W TEKŚCIE, nie z kolejności wklejania
 *
 * To jest cała trudność tego pliku. Człowiek wkleja obraz, pisze zdanie, cofa kursor NAD nie
 * i wkleja drugi. Numerowanie po kolejności wklejenia postawiłoby wtedy `[image 2]` przed
 * `[image 1]`, a pasek miniatur pokazałby je w odwrotnej kolejności niż zdanie, które je opisuje.
 * Dlatego po każdej zmianie znaczniki są przenumerowane od lewej do prawej, a lista obrazów
 * układa się pod nie — tekst jest źródłem prawdy o porządku, nie odwrotnie.
 *
 * # Dlaczego `[image N]`, a nie identyfikator
 *
 * Ten napis czyta AGENT, nie tylko ekran. `[image a41f]` jest jednoznaczne i nieczytelne;
 * `[image 1]` mówi to samo, co miniatura podpisana „Pasted image 1" pod polem, więc człowiek
 * i model widzą ten sam numer.
 */

/** Znacznik jednego obrazu w treści wiadomości. */
const MARKER = /\[image (\d+)\]/g;

/** Gdzie stoi każdy znacznik, od lewej. */
export function markersIn(
  text: string,
): readonly { readonly start: number; readonly end: number }[] {
  return Array.from(text.matchAll(MARKER)).map((found) => ({
    start: found.index,
    end: found.index + found[0].length,
  }));
}

/** Ten sam tekst z numerami 1..n nadanymi od lewej. */
export function renumbered(text: string): string {
  let next = 0;
  return text.replace(MARKER, () => {
    next += 1;
    return `[image ${String(next)}]`;
  });
}

/** Co się dzieje z wiadomością, kiedy w miejscu kursora ląduje obraz. */
export interface Placed {
  /** Treść ze wstawionym i przenumerowanym znacznikiem. */
  readonly text: string;
  /** Miejsce w liście obrazów, w które ten obraz ma wejść, żeby lista szła jak tekst. */
  readonly index: number;
  /** Gdzie postawić kursor, żeby człowiek pisał DALEJ, a nie przed znacznikiem. */
  readonly caret: number;
}

/**
 * Wstawia znacznik w miejscu kursora.
 *
 * Spacje wokół znacznika są treścią, nie ozdobą. Bez tej z tyłu pierwsze słowo napisane po
 * wklejeniu skleja się w `[image 1]tu` i wzorzec przestaje pasować — znacznik ginie razem
 * z adresem, po który tu jest. Bez tej z przodu wklejenie na końcu zdania daje `a b[image 1]`,
 * czyli napis doklejony do słowa; dokładamy ją WYŁĄCZNIE wtedy, gdy poprzedni znak nie jest
 * odstępem, żeby nie mnożyć spacji przy każdym wklejeniu w już rozstrzelone miejsce.
 */
export function placedAt(text: string, caret: number): Placed {
  const at = Math.max(0, Math.min(caret, text.length));
  const before = markersIn(text).filter((one) => one.start < at).length;
  const lead = at > 0 && !/\s/.test(text.charAt(at - 1)) ? ' ' : '';
  /* Odstęp z tyłu TYLKO wtedy, gdy go tam jeszcze nie ma. Wklejenie w środek zdania, które już
   * ma spację po kursorze, dorzucałoby drugą — a podwójny odstęp w wiadomości jest czymś, czego
   * człowiek nie napisał i nie umie sobie wytłumaczyć. */
  const trail = at < text.length && /\s/.test(text.charAt(at)) ? '' : ' ';
  const mark = `${lead}[image ${String(before + 1)}]${trail}`;
  return {
    text: renumbered(text.slice(0, at) + mark + text.slice(at)),
    index: before,
    caret: at + mark.length,
  };
}

/** Ten sam tekst bez znacznika o podanym miejscu w liście, z resztą przenumerowaną. */
export function withoutMarker(text: string, index: number): string {
  const marks = markersIn(text);
  const gone = marks[index];
  if (gone === undefined) return text;
  /* Spacja, którą [`placedAt`] dokłada za znacznikiem, znika razem z nim. Zostawiona
   * produkowałaby podwójne odstępy przy każdym usunięciu i nikt by nie wiedział, skąd są. */
  const after = text.slice(gone.end).startsWith(' ') ? gone.end + 1 : gone.end;
  return renumbered(text.slice(0, gone.start) + text.slice(after));
}
