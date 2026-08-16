/* Jedyne miejsce w sekcji Pamięć, które zna nazwy komend po stronie Rusta
 * (niezmiennik 23: polityka w jednym rdzeniu, adapter po pięć linii).
 *
 * DLACZEGO OSOBNY PLIK, A NIE `invoke()` rozsiany po magazynie i po wierszu. Kryterium stanu
 * mierzy LICZBĘ wywołań: „ani jednego więcej, dopóki człowiek nie odpowie na wymuszony wybór".
 * Zdanie o liczbie wywołań ma sens tylko wtedy, kiedy jest jedna krawędź, przez którą da się
 * wywołać cokolwiek — dwie drogi do Rusta znaczą, że licznik pilnuje jednej z nich, a promocja
 * jedzie drugą i nikt tego nie zauważy.
 *
 * 2026-08-16 — ciała wypełnia T-27, dwiema nazwami z `src-tauri/commands.golden.txt`. Krawędź
 * zostaje tutaj: sekcja ma wiedzieć, CO woła, a nie jak to jedzie.
 */
import { invoke } from '@tauri-apps/api/core';

import type { Note } from '../../state/memory';

/**
 * „Use this": od tej chwili notatka wchodzi do promptu.
 *
 * Wraca **cała notatka odczytana z pliku po zapisie**, nie `void`: magazyn ma przestawić stan
 * na to, co naprawdę leży na dysku, a nie na to, czego się spodziewał. Odmowa („zakres jest
 * pełny", „nie ma uzasadnienia") przyjeżdża jako odrzucenie obietnicy.
 */
export function putToUse(args: { id: string }): Promise<Note> {
  return invoke<Note>('put_note_to_use', args);
}

/** „Stop using": notatka zostaje na liście i przestaje wchodzić do promptu. */
export function stopUsing(args: { id: string }): Promise<Note> {
  return invoke<Note>('stop_using_note', args);
}
