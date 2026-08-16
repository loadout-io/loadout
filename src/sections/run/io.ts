/* Jedyne miejsce w sekcji Bieg, które zna nazwy komend po stronie Rusta
 * (niezmiennik 23: polityka w jednym rdzeniu, krawędź po pięć linii).
 *
 * DLACZEGO OSOBNY PLIK, A NIE `invoke()` pod przyciskiem. Start jest pierwszą krawędzią tej
 * sekcji, ale nie ostatnią: za nim stoi Stop, a za Stopem Continue. Trzy `invoke` rozsiane po
 * trzech komponentach to trzy miejsca, w których mieszka nazwa komendy — a wtedy zapadka
 * „drugie kliknięcie nie startuje drugiego biegu" musi istnieć w każdym z nich osobno i
 * w jednym zawsze jej zabraknie.
 *
 * DLACZEGO IDENTYFIKATOR I LIMIT SĄ ARGUMENTAMI, A NIE ODCZYTEM ZE STANU. Magazyn otwartego
 * dokumentu (`src/state/workflows.ts`) jest FABRYKĄ, nie singletonem, a liczba „ile naraz" jest
 * stanem całej aplikacji, nie tej sekcji (`src/state/workspaces.ts`, akapit „czego tu nie ma").
 * Krawędź, która sięgałaby po nie sama, byłaby drugim miejscem, w którym mieszka odpowiedź na
 * pytanie „co jest otwarte" — i pierwszym, które by się rozjechało.
 *
 * 2026-08-17 — CIAŁO JEST SZKIELETEM (faza kontraktu). Ma się WCZYTAĆ i paść w czasie
 * wykonania: moduł, którego nie ma, daje „Cannot find module", czyli czerwień, której bramka
 * nie liczy (AGENTS.md §2a). Wypełnia je faza implementacji — razem z zapadką opisaną niżej.
 */

/**
 * Start: uruchamia otwarty workflow.
 *
 * Rozwiązuje się dopiero wtedy, kiedy bieg się skończy — komenda po stronie Rusta trwa tyle,
 * co bieg — i to jest zarazem cała definicja słowa „w trakcie" dla zapadki: **drugie kliknięcie,
 * zanim pierwsze wróci, nie ma prawa zawołać komendy drugi raz**. Dwa biegi tego samego
 * workflow to dwa zestawy agentów piszących po tych samych plikach, czyli dokładnie to, czego
 * walidator odmawia przy zapisie (niezmiennik 12) — tylko że tutaj nikt nie odmawia, bo z
 * punktu widzenia Rusta to są dwa poprawne żądania.
 *
 * @param workflow identyfikator otwartego workflow — to samo, czym front nazywa jego plik.
 *   Katalog rozwiązuje Rust [T3 §8.3]; front, który dokleiłby ścieżkę sam, byłby drugim
 *   miejscem, w którym mieszka odpowiedź na pytanie „gdzie to leży".
 * @param howManyAtOnce ile kroków ma NAPRAWDĘ biec naraz. Liczba jedzie w żądaniu, nigdy ze
 *   stałej po tamtej stronie (niezmiennik 11): cicha wersja złamania wygląda jak pole, które
 *   jest wczytywane, logowane i nigdzie nie podawane, a semafor dostaje `1`.
 */
export function start(workflow: string, howManyAtOnce: number): Promise<void> {
  throw new Error(
    'not implemented: run ' + workflow + ' with ' + String(howManyAtOnce) + ' steps at once',
  );
}
