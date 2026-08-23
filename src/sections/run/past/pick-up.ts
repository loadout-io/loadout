/* „Kontynuuj stąd" — wznowienie biegu z historii od wskazanego kroku.
 *
 * 2026-08-23, pytanie właściciela nad ekranem historii: „a z history możemy kontynuować?".
 * Do tego dnia historia była wyłącznie do czytania, a jedyne wznowienie, jakie produkt umiał
 * (`../rail/again.ts`), powtarzało DOKŁADNIE JEDEN kafelek OSTATNIEGO biegu tego workflow.
 * Bieg, który padł na siódmym kroku z dziesięciu, nie da się tak dokończyć: sześć skończonych
 * kroków nikt nie chce powtarzać, a trzech nieruszonych tamta droga nie widzi wcale.
 *
 * OSOBNY PLIK, nie ciało handlera w `panel.tsx` — ten sam powód, co przy `../rail/again.ts`:
 * tamten plik jest MONTAŻEM, a to repo nie ma jsdom, więc `onClick` nie odpala się w żadnym
 * kryterium. Decyzja w czystej funkcji jest jedyną, o którą kryterium umie zapytać.
 *
 * ZAKRES JEDZIE Z MAGAZYNU PANELU, nie z `activeWorkspace()`: lista przyszła z konkretnego
 * folderu, więc bieg z niej wznawia się w tym samym folderze — także wtedy, gdy człowiek
 * przełączył boczne menu, zanim kliknął. Ta sama reguła i ten sam powód, co przy `openOneRun`.
 */
import { saidOf } from '../entry/echo';
import { feedFor } from '../feed/live';
import { atOnce as atOnceNow } from '../limits/chosen';
import { resumeRun } from '../io';
import { closeHistory, pastNow } from './store';

/** Napis na przycisku. Kontrakt kryterium — nie „resume", nie „continue": jedno i drugie znaczy
 * w tym produkcie co innego (punkt kontrolny odpowiada słowem „continue"). */
export const PICK_UP_HERE = 'Pick up here';

/**
 * Wznawia otwarty bieg od kroku o tym kluczu: on i wszystko, co graf stawia po nim.
 *
 * Panel zamyka się od razu, bo to, co się teraz dzieje, dzieje się w strumieniu pod nim —
 * historia stojąca nad ruszonym biegiem zasłaniałaby jedyne miejsce, w którym widać, że
 * cokolwiek ruszyło.
 */
export function pickUpFrom(step: string): void {
  const past = pastNow();
  if (past.opened === null) {
    /* CICHY POWRÓT, i to jest jedyne miejsce w tym pliku, w którym on jest uczciwy: z ekranu nie
     * da się tu dojść, bo przycisk stoi WEWNĄTRZ otwartego biegu. Zdania tu nie ma, bo nie ma
     * gdzie stanąć — panel, w którym człowiek nacisnął, w tym stanie nie istnieje, a wiersz
     * w strumieniu odpowiadałby na pytanie, którego nikt nie zadał. Warunek zostaje, bo bez
     * niego wznowienie poszłoby z pustą nazwą katalogu i tamta strona brałaby przekazania
     * z czegokolwiek. */
    return;
  }
  const { folder, opened } = past;
  closeHistory();
  /* NAZWA I PLIK JADĄ RAZEM Z ŻĄDANIEM, bo bez nich okno nie umie powiedzieć, że coś biegnie —
   * a to jest defekt ze zrzutu właściciela: `/stop` nad pracującym agentem odpowiedziało
   * „Nothing is running." Powód w całości stoi przy `asARun` w `../io.ts`. */
  void resumeRun(
    opened.folder,
    step,
    atOnceNow(),
    folder,
    opened.title === '' ? opened.when : opened.title,
    opened.workflowFile,
  )
    .then((said) => {
      /* Zdanie przychodzi TYLKO wtedy, gdy dzisiejszy plik różni się od tego, który wtedy biegł.
       * Panel jest już zamknięty, więc idzie tam, gdzie idą odpowiedzi ekranu pracy. */
      if (said !== null) sayOnTheRun(folder, said);
    })
    .catch((error: unknown) => {
      sayOnTheRun(folder, error instanceof Error ? error.message : String(error));
    });
}

/** Odpowiedź ekranu pracy — DO STRUMIENIA, tą samą drogą, którą idzie każda inna.
 *
 * Nie do slotu pod paskiem i nie do panelu: panel jest już zamknięty, a slot żyje w `useState`
 * ekranu, więc wyjście do Agentów gubiłoby zdanie razem z komponentem. Strumień żyje na poziomie
 * modułu, bo bieg trwa dłużej niż ekran — powód stoi w całości przy `sayWhatDidNotStart`
 * w `../index.tsx`. Zakres jest ten sam, z którego wznawialiśmy: wiersz ma stanąć w strumieniu
 * TEGO workspace'u, a nie tego, na który człowiek przełączył się w międzyczasie.
 */
function sayOnTheRun(folder: string | null, said: string): void {
  feedFor(folder ?? '').appendLines([saidOf(said)]);
}
