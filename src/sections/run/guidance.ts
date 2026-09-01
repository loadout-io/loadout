/* CZY CZŁOWIEK CHCE JESZCZE BYĆ PROWADZONY — jeden fakt, jedno miejsce.
 *
 * PO CO TO ISTNIEJE. Ekran pierwszego otwarcia zajmuje całą strefę pracy: droga, powitanie,
 * cztery gotowe agenty. Dla kogoś, kto instaluje Loadout drugi raz, to jest ściana stojąca
 * dokładnie tam, gdzie chce zacząć pracować — a przewodnik bez wyjścia przestaje być pomocą
 * i staje się przeszkodą. Makieta nazywa to wyjście dosłownie: „I know my way".
 *
 * DLACZEGO NA POZIOMIE MODUŁU, A NIE W `useState` KOMPONENTU. Ten sam powód, co przy
 * `./whats-ready.ts` i `./limits/chosen.ts`: `src/App.tsx` trzyma w drzewie DOKŁADNIE jedną
 * sekcję, więc wyjście do Agentów i powrót niszczy stan komponentu — a przewodnik odłożony na
 * bok wracałby wtedy przy każdym powrocie, czyli kontrolka „schowaj to" nie schowałaby niczego
 * na dłużej niż jedno spojrzenie. Drugi powód jest dowodowy: to repo nie ma jsdom, więc stan
 * zamknięty w komponencie jest wartością, do której żadne kryterium nie ma jak dojść
 * (niezmiennik 29) — czyli wyjście byłoby mechanizmem bez ani jednej wyroczni.
 *
 * DLACZEGO NIE NA DYSKU. Bo to nie jest ustawienie, tylko odpowiedź na JEDNO pytanie zadane
 * w JEDNYM otwarciu okna: „czy pokazać ci drogę teraz". Zapisane na dysku byłoby dziewiątą
 * pozycją w Settings, której nikt nie szuka, i pierwszą rzeczą, która zabrania nowemu
 * człowiekowi zobaczyć przewodnik po tym, jak raz go zamknął na cudzej maszynie.
 *
 * ZNIKA TAKŻE SAMO Z SIEBIE. Kiedy droga jest przejechana, `./index.tsx` nie rysuje przewodnika
 * w ogóle — i to jest DRUGA, niezależna droga do tej samej ciszy. Ta tutaj jest dla człowieka,
 * tamta dla świata; żadna nie zna połowy drugiej.
 */

let asked = false;

const listening = new Set<() => void>();

function tell(next: boolean): void {
  asked = next;
  for (const one of listening) one();
}

/** Czy przewodnik ma się w ogóle rysować. Ta sama migawka dla okna i dla renderu statycznego. */
export function guidanceIsWanted(): boolean {
  return !asked;
}

export function subscribeToGuidance(onChange: () => void): () => void {
  listening.add(onChange);
  return () => {
    listening.delete(onChange);
  };
}

/**
 * „I know my way" — człowiek odkłada przewodnik na bok.
 *
 * FUNKCJA W MODULE, a nie ciało handlera: to repo nie ma jsdom, więc kliknięcia nie da się
 * odpalić w kryterium. Przycisk podaje `onClick` DOKŁADNIE tę funkcję, którą woła wyrocznia —
 * napis nad handlerem, którego nikt nie umie dotknąć, jest martwą kontrolką (niezmiennik 16).
 */
export function stepAside(): void {
  tell(true);
}

/** Wyłącznie dla kryteriów: przywraca stan sprzed odłożenia przewodnika. */
export function wantGuidanceAgain(): void {
  tell(false);
}
