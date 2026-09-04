/* Co refleksja ZROBIŁA z tym biegiem — jedno zdanie, pięć rozłącznych stanów.
 *
 * PO CO TO ISTNIEJE (2026-08-29, T-165). Po każdym biegu Loadout brał prywatną turę, zapisywał
 * jej rachunek do `run.json` (`commands::run::ReflectionReceipt`) i dowód do
 * `<bieg>/logs/reflection.jsonl` — a ŻADEN ekran tego nie czytał. Człowiek zostawiał ptaszek
 * włączony, płacił za turę i nie miał ani jednego miejsca, w którym mógłby zobaczyć, czy z niej
 * cokolwiek wyszło. Cisza jest przy tym nieodróżnialna od awarii: bieg, po którym nie powstała
 * żadna notatka, wygląda dokładnie tak samo, jak bieg, w którym tura padła.
 *
 * PIĘĆ STANÓW I PIĘĆ ZDAŃ, bo to jest pięć różnych rzeczy do zrobienia:
 *
 *   1. opis biegu nic o tym nie mówi   — bieg zapisany przed tym polem; nie wiemy i nie zgadujemy,
 *   2. tura nie poszła                 — nikt nie pytał, więc nie ma czego szukać w Knowledge,
 *   3. tura poszła i nie zdążyła       — skończyły się jej pieniądze albo czas (2026-09, Z-38),
 *   4. tura poszła i nic nie zostawiła — pytaliśmy i odpowiedź brzmi „nic", i to jest odpowiedź,
 *   5. tura poszła i zostawiła notatki — jest ich N i czekają w Knowledge.
 *
 * Zlanie 1 z 2 jest tą wadą, dla której to zadanie powstało: „nie wiadomo" przedstawione jako
 * „nie robiliśmy tego" jest zmyśleniem (niezmiennik 17).
 *
 * CZYSTA FUNKCJA, bez `invoke` i bez stanu — dokładnie jak `../history-command.ts`. Zdanie
 * ma się dać osądzić bez okna, a to, że stoi w markupie, sądzi `./reflection-explains-itself`.
 */
import type { PastReflection } from '../io';
import { REFLECTION_LABEL } from './toggle';

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
 * 2026-09 (Z-18): kod z pliku → słowa dla człowieka. Nieznanego kodu celowo tu nie ma:
 * surowy enum z drutu nigdy nie trafia na ekran (niezmiennik 14), a gołe zdanie zachowuje
 * czytelność (5).
 */
const WHY_NOT: Readonly<Record<string, string>> = {
  stopped: 'you stopped it',
  'turned-off': 'learning from runs was turned off',
  'no-agent-worked': 'no agent finished any work',
  'nothing-was-left': 'no agent left anything to learn from',
  'nothing-came-back': 'nothing came back from the learning turn',
  /* 2026-09 (Z-38): jedyny z tych powodów, który mówi o TEJ MASZYNIE, a nie o biegu — nic
     w katalogu biegu tego nie naprawi, więc czyta się inaczej niż pozostałe. */
  'no-agent-app': 'the agent app it needed did not start',
};

function didNotLookBack(reflection: PastReflection): string {
  const because = reflection.why === null ? undefined : WHY_NOT[reflection.why ?? ''];
  if (because === undefined) return DID_NOT_LOOK_BACK;
  return DID_NOT_LOOK_BACK.slice(0, -1) + ' because ' + because + '.';
}

/**
 * Początek zdania o turze, która poszła i nie zdążyła odpowiedzieć.
 *
 * Z `REFLECTION_LABEL`, a nie z przepisanego napisu: człowiek ma poznać rzecz, którą sam
 * zaznaczył, a nazwa tej kontrolki żyje w jednym miejscu (niezmiennik 13). Te same słowa składa
 * po drugiej stronie granicy `commands::run::REFLECTION_DID_NOT_FINISH` dla wiersza, który
 * schodzący bieg stawia w strumieniu.
 */
const DID_NOT_FINISH = REFLECTION_LABEL + " didn't finish: the note-taker ";

/**
 * Zdanie o turze, która zeszła na swoim suficie — albo `undefined`, kiedy zeszła inaczej.
 *
 * # Dlaczego to NIE jest kolejny wiersz `WHY_NOT` (2026-09, Z-38)
 *
 * Bo tamte kończą zdanie „Loadout did not look back at this run", czyli mówią, że tury NIE
 * BYŁO. Tu tura była, poszła i została opłacona — a odpowiedź nie zdążyła wrócić. Zmierzone na
 * biegu z 2026-09-04: dwie godziny pracy, tura zabita po 22 sekundach na ośmiu centach i wiersz
 * historii mówiący „did not look back", czyli dokładnie to, co się nie stało.
 *
 * KWOTA, KIEDY JEST. `budgetUsd` przyjeżdża z tego samego rachunku, co powód, więc brakuje jej
 * wyłącznie w pliku sprzed tej zmiany — a taki plik nie ma też tego powodu. Zdanie bez liczby
 * jest tu jednak prawdziwe, i to jest niezmiennik 5 na granicy: brak jednego klucza nie ma
 * prawa zabrać człowiekowi całego faktu o jego biegu.
 *
 * EKSPORTOWANE, BO TO ZDANIE STOI NA TRZECH POWIERZCHNIACH (2026-09, Z-38): w strumieniu biegu
 * (składa je Rust, `commands::run::ran_out_sentence`), w panelu historii przez `reflectionText`
 * niżej i w sekcji Knowledge, która tłumaczy nim pustą kolejkę decyzji. Trzy żywe regiony na
 * jeden fakt są WYJĄTKIEM od niezmiennika 13, zgłoszonym właścicielowi i przez niego
 * podtrzymanym — a skoro tak, to niech przynajmniej zdanie ma jedno źródło po tej stronie
 * granicy: druga jego kopia rozjechałaby się przy pierwszej poprawce słowa.
 */
export function ranOutOfSomething(reflection: PastReflection): string | undefined {
  if (reflection.why === 'ran-out-of-budget') {
    const had = reflection.budgetUsd;
    const spent =
      had === undefined || had === null
        ? 'used up what it was given'
        : 'used its $' + had.toFixed(2);
    return DID_NOT_FINISH + spent + ' before answering.';
  }
  if (reflection.why === 'ran-out-of-time') {
    return DID_NOT_FINISH + 'ran out of time before answering.';
  }
  return undefined;
}

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
  // PRZED `didNotLookBack`, bo to jest tura, która BYŁA: zdanie o tym, że Loadout nie oglądał
  // się za siebie, opisywałoby wtedy bieg, za którego turę człowiek zapłacił (2026-09, Z-38).
  const ranOut = ranOutOfSomething(reflection);
  if (ranOut !== undefined) return ranOut;
  if (!reflection.ran) return didNotLookBack(reflection);
  if (reflection.kept === 0) return KEPT_NOTHING + alsoThrewOut(reflection);
  return (
    'Loadout looked back at this run and kept ' +
    notesText(reflection.kept) +
    ' for you to approve in Knowledge.'
  );
}
