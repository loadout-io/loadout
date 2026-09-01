/* Który fragment wpisanej linii jest nazwą workflow, którą Loadout naprawdę zna.
 *
 * PO CO TO ISTNIEJE. Zamówienie właściciela 2026-08-30: „jak to wpisuje w terminal to ma sie
 * podswietlac jakos zajebiscie". Chodzi o to, żeby wpisywana nazwa **wyróżniła się na żywo**,
 * a nie dopiero po naciśnięciu Enter — czyli żeby literówkę było widać, zanim się za nią zapłaci
 * odmową.
 *
 * DLACZEGO CZYSTA FUNKCJA. Bo to jest polityka („co jest nazwą, którą znamy"), a to repo nie ma
 * jsdom, więc pisania w polu nie da się odpalić w kryterium. Reguła zamknięta w komponencie
 * byłaby kodem, którego nie dotknie żadne kryterium — ta sama rodzina, z której wzięło się
 * siedemnaście kłamiących kontrolek w repo źródłowym.
 *
 * DLACZEGO TA SAMA REGUŁA, CO ENTER. Rozpoznanie liczy `typable` i porównuje z listą — dokładnie
 * tak, jak robi to `readRunLine` przy naciśnięciu (`../run-command.ts`). Drugie, luźniejsze
 * dopasowanie tutaj byłoby kolorem obiecującym coś, czego Enter odmówi, i to jest gorsze niż brak
 * koloru: człowiek uczy się ufać podświetleniu w pierwszej minucie.
 *
 * CZEGO TU NIE MA: sprawdzania, czy workflow ma kroki. To rozstrzyga polityka startu i ma jedną
 * odpowiedź (niezmiennik 23). Podświetlenie mówi „znam tę nazwę", nie „to na pewno ruszy".
 */
import { typable } from '../run-command';

/** Kawałek linii razem z odpowiedzią, czy Loadout zna tę nazwę. */
export interface Piece {
  readonly text: string;
  /** `true` wyłącznie dla fragmentu, który JEST nazwą z listy. */
  readonly known: boolean;
}

/** Komenda, po której drugie słowo jest nazwą workflow. */
const RUN = '/run';

/**
 * Rozbiór linii na kawałki, z których dokładnie jeden może być podświetlony.
 *
 * Zwraca CAŁĄ linię pociętą na kawałki, nie same trafienia: warstwa rysująca skleja je z powrotem
 * znak w znak, a fragment zgubiony po drodze przesunąłby podświetlenie o tyle znaków, ile go
 * brakuje — czyli pokolorowałby sąsiednie słowo.
 *
 * Pusta linia oddaje pustą listę, a nie jeden pusty kawałek: warstwa nie ma wtedy prawa dołożyć
 * ani jednego węzła tekstowego, bo zapadka gęstości `textElements` może tylko maleć
 * (`docs/ARCHITECTURE.md` §7).
 */
export function segments(line: string, known: readonly string[]): readonly Piece[] {
  /* WYŁĄCZNIE PO `/run `. Nazwa workflow jest DRUGIM słowem tej jednej komendy; szukanie jej
   * w dowolnym miejscu linii podświetlałoby słowo w zdaniu do lidera — a zdanie do lidera jest
   * tym, co człowiek pisze najczęściej. */
  if (!line.startsWith(`${RUN} `)) return line === '' ? [] : [{ text: line, known: false }];

  const head = line.slice(0, RUN.length + 1);
  const rest = line.slice(RUN.length + 1);
  /* Nazwa kończy się na pierwszej spacji: wszystko za nią jest zadaniem dla agentów, a nie
   * częścią nazwy. Wiodących spacji nie ma po `head`, bo `head` je zabrał. */
  const cut = rest.indexOf(' ');
  const name = cut === -1 ? rest : rest.slice(0, cut);
  const tail = cut === -1 ? '' : rest.slice(cut);

  if (name === '') return [{ text: head, known: false }];

  const matches = known.some((one) => typable(one) === typable(name));
  const pieces: Piece[] = [
    { text: head, known: false },
    { text: name, known: matches },
  ];
  if (tail !== '') pieces.push({ text: tail, known: false });
  return pieces;
}
