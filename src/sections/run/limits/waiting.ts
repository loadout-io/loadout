/* Ile kroków stoi w kolejce po wolne miejsce — i gdzie stoją.
 *
 * PO CO TEN PLIK ISTNIEJE. Pasek kart ma jedno zdanie o czekaniu („N of M slots in use — an
 * agent in X is waiting for a free one") i do 2026-08-18 nie mogło się pokazać NIGDY: ekran
 * podawał `waitingIn={null}` na sztywno, bo „ile kroków czeka" nie miało nośnika na drucie.
 * Nośnik jest: stan kroku `ready` znaczy dokładnie „krok jest gotowy do biegu i nie ma jeszcze
 * permitu" [ARCHITECTURE §5]. `src/state/run.ts` przyjmuje go z wiersza `stepState` i przepisuje
 * do planu, więc kolejka jest już w oknie — brakowało wyłącznie jej policzenia.
 *
 * MILCZĄCE CZEKANIE JEST NIEODRÓŻNIALNE OD ZAWIESZENIA i to jest cały powód, dla którego to
 * zdanie stoi na ekranie: kiedy pula jest pełna, krok czekający na miejsce wygląda dokładnie
 * jak zepsuty, a człowiek ubija bieg, który był zdrowy.
 *
 * FUNKCJE CZYSTE, POZA KOMPONENTEM, bo to repo nie ma jsdom: liczba w propsie da się osądzić
 * tylko wtedy, gdy powstaje w funkcji, którą test może zawołać wprost (niezmiennik 15 w duchu).
 *
 * ZERO ZNACZY „NIKT NIE CZEKA", NIE „NIE WIEM". Jedno i drugie musi wyglądać inaczej, więc
 * nazwa folderu wraca jako `null`, kiedy nikt nie czeka albo kiedy nie wiadomo, jak nazwać
 * miejsce: zdanie o kolejce, której nie ma, jest gorsze niż brak zdania (niezmiennik 17).
 */
import type { Step } from '../../../state/run';
import { folderName } from '../folders';

/**
 * Ile kroków tego biegu stoi w kolejce po miejsce.
 *
 * Wyłącznie `ready`. `pending` znaczy „jeszcze nie może pójść, bo czeka na poprzednika" i nie
 * jest kolejką po miejsce — liczenie ich razem dawałoby przy starcie zdanie o dziesięciu krokach
 * czekających na wolne miejsce w chwili, w której żaden nie jest jeszcze gotowy.
 */
export function waitingFor(steps: readonly Step[]): number {
  return steps.filter((step) => step.state === 'ready').length;
}

/**
 * Nazwa folderu, w którym coś stoi w kolejce — albo `null`, kiedy nie stoi nic.
 *
 * Nazwa, nie `boolean`: „ktoś gdzieś czeka" nie mówi człowiekowi, gdzie zajrzeć, a to jest
 * jedyny powód, dla którego to zdanie w ogóle jest na ekranie. `null` także wtedy, gdy kroki
 * czekają, a folderu nie znamy (bieg puszczony bez wskazanego zakresu): wolę brak zdania niż
 * zdanie z pustą nazwą miejsca.
 */
export function waitingWhere(steps: readonly Step[], folder: string | null): string | null {
  if (waitingFor(steps) === 0) return null;
  if (folder === null || folder === '') return null;
  return folderName(folder);
}
