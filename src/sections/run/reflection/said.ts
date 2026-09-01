/* Co refleksja ZROBIŁA z tym biegiem — jedno zdanie, cztery rozłączne stany.
 *
 * PO CO TO ISTNIEJE (2026-08-29, T-165). Po każdym biegu Loadout brał prywatną turę, zapisywał
 * jej rachunek do `run.json` (`commands::run::ReflectionReceipt`) i dowód do
 * `<bieg>/logs/reflection.jsonl` — a ŻADEN ekran tego nie czytał. Człowiek zostawiał ptaszek
 * włączony, płacił za turę i nie miał ani jednego miejsca, w którym mógłby zobaczyć, czy z niej
 * cokolwiek wyszło. Cisza jest przy tym nieodróżnialna od awarii: bieg, po którym nie powstała
 * żadna notatka, wygląda dokładnie tak samo, jak bieg, w którym tura padła.
 *
 * CZTERY STANY I CZTERY ZDANIA, bo to są cztery różne rzeczy do zrobienia:
 *
 *   1. opis biegu nic o tym nie mówi   — bieg zapisany przed tym polem; nie wiemy i nie zgadujemy,
 *   2. tura nie poszła                 — nikt nie pytał, więc nie ma czego szukać w Knowledge,
 *   3. tura poszła i nic nie zostawiła — pytaliśmy i odpowiedź brzmi „nic", i to jest odpowiedź,
 *   4. tura poszła i zostawiła notatki — jest ich N i czekają w Knowledge.
 *
 * Zlanie 1 z 2 jest tą wadą, dla której to zadanie powstało: „nie wiadomo" przedstawione jako
 * „nie robiliśmy tego" jest zmyśleniem (niezmiennik 17).
 *
 * CZYSTA FUNKCJA, bez `invoke` i bez stanu — dokładnie jak `../history-command.ts`. Zdanie
 * ma się dać osądzić bez okna, a to, że stoi w markupie, sądzi `./reflection-explains-itself`.
 */
import type { PastReflection } from '../io';

/**
 * Co stoi przy biegu, którego `run.json` powstał, zanim to pole istniało.
 *
 * NAZYWA NASZĄ NIEWIEDZĘ, a nie stan biegu. Zdanie „Loadout did not look back at this run"
 * byłoby tu twierdzeniem o czymś, czego z tego pliku nie da się odczytać — a wygląda ono
 * identycznie jak prawda o biegu, którego naprawdę nie pytano.
 */
export const NOT_IN_THE_RECORD =
  "This run's record does not say whether Loadout looked back at it.";

/** Bieg, po którym prywatna tura nie poszła — bo jej nie chciano, albo nie było czego czytać. */
export const DID_NOT_LOOK_BACK = 'Loadout did not look back at this run.';

/**
 * Tura poszła i nie zostawiła nic.
 *
 * MÓWI TO WPROST i to jest cała przyczyna, dla której ten plik istnieje. Puste miejsce w tym
 * wierszu czyta się jak ekran, który się nie dorysował — a to jest bieg, za którego turę ktoś
 * zapłacił.
 */
export const KEPT_NOTHING = 'Loadout looked back at this run and found nothing worth keeping.';

/** Ile notatek — zdanie, nie liczba obok słowa, żeby jedna nie czytała się jak „1 notes". */
function notesText(notes: number): string {
  return notes === 1 ? '1 note' : String(notes) + ' notes';
}

/**
 * Co jeszcze z tej tury wypadło, kiedy nie została ani jedna notatka — albo pusty napis.
 *
 * DWA POWODY SĄ ROZŁĄCZNE i oba są odpowiedzią na inne pytanie człowieka. „Już to odrzuciłeś"
 * mówi, że Loadout wraca do tego samego i że decyzja człowieka trzyma; „bez uzasadnienia" mówi,
 * że reguła przyszła bez „dlaczego", a takiej nie zapisujemy, bo instrukcji bez uzasadnienia nie
 * da się potem wycofać [T6 §5.1]. Bez tej klauzuli oba te biegi czytają się jak bieg, w którym
 * model nie miał nic do powiedzenia.
 */
function alsoThrewOut(reflection: PastReflection): string {
  const parts: string[] = [];
  if (reflection.discardedAgain > 0) {
    parts.push(notesText(reflection.discardedAgain) + ' you had already turned down');
  }
  if (reflection.droppedWithoutReason > 0) {
    parts.push(notesText(reflection.droppedWithoutReason) + ' that came with no reason under it');
  }
  if (parts.length === 0) return '';
  return ' It threw out ' + parts.join(', and ') + '.';
}

/**
 * Jedno zdanie o tym, co refleksja zrobiła z tym biegiem. Nigdy pusty napis.
 *
 * @param reflection rachunek z `run.json`, albo `null` — kiedy opis biegu tego pola nie niesie.
 */
export function reflectionText(reflection: PastReflection | null): string {
  if (reflection === null) return NOT_IN_THE_RECORD;
  if (!reflection.ran) return DID_NOT_LOOK_BACK;
  if (reflection.kept === 0) return KEPT_NOTHING + alsoThrewOut(reflection);
  return (
    'Loadout looked back at this run and kept ' +
    notesText(reflection.kept) +
    ' for you to approve in Knowledge.'
  );
}
