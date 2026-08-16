/* Jedyne miejsce w sekcji Pamięć, które zna nazwy komend po stronie Rusta
 * (niezmiennik 23: polityka w jednym rdzeniu, adapter po pięć linii).
 *
 * DLACZEGO OSOBNY PLIK, A NIE `invoke()` rozsiany po magazynie i po wierszu. Kryterium stanu
 * mierzy LICZBĘ wywołań: „ani jednego więcej, dopóki człowiek nie odpowie na wymuszony wybór".
 * Zdanie o liczbie wywołań ma sens tylko wtedy, kiedy jest jedna krawędź, przez którą da się
 * wywołać cokolwiek — dwie drogi do Rusta znaczą, że licznik pilnuje jednej z nich, a promocja
 * jedzie drugą i nikt tego nie zauważy.
 *
 * Kiedy wyląduje `src/ipc/**` (T-07), te dwa ciała stają się jednolinijkowymi przelotkami
 * przez tamten moduł. Krawędź zostaje tutaj: sekcja ma wiedzieć, CO woła, a nie jak to jedzie.
 *
 * Ciała są jeszcze puste. Szkielet ma się WCZYTAĆ i paść w czasie wykonania — moduł, którego
 * nie ma, daje „Cannot find module", czyli czerwień, której bramka nie liczy (AGENTS.md §2a).
 */
import type { Note } from '../../state/memory';

/**
 * „Use this": od tej chwili notatka wchodzi do promptu.
 *
 * Wraca **cała notatka odczytana z pliku po zapisie**, nie `void`: magazyn ma przestawić stan
 * na to, co naprawdę leży na dysku, a nie na to, czego się spodziewał. Odmowa („zakres jest
 * pełny", „nie ma uzasadnienia") przyjeżdża jako odrzucenie obietnicy.
 */
export function putToUse(args: { id: string }): Promise<Note> {
  throw new Error('not implemented: put ' + args.id + ' to use');
}

/** „Stop using": notatka zostaje na liście i przestaje wchodzić do promptu. */
export function stopUsing(args: { id: string }): Promise<Note> {
  throw new Error('not implemented: stop using ' + args.id);
}
