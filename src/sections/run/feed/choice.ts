/* Opcja odpowiedzi: co się z niej czyta i który klawisz ją wybiera.
 *
 * # Skąd bierze się drugie zdanie
 *
 * Z opcji, którą napisał agent, i wyłącznie stamtąd. Na drucie stoi `options: Vec<String>`
 * (`src/ipc/types.ts`, rodzaj `asked`) i nic poza tym — żadnego pola na konsekwencję, żadnego
 * pola na tytuł. Makieta rysuje pod tytułem zdanie „Second reader gets the change as it stands";
 * jedyną uczciwą drogą, jaką to zdanie ma na ekran, jest napisanie go przez agenta w TEJ SAMEJ
 * opcji. Ten moduł rozcina napis tam, gdzie agent sam go rozdzielił, i **nie dopisuje ani
 * jednego słowa**: opcja bez myślnika ma sam tytuł i żadnego drugiego wiersza.
 *
 * Zdanie konsekwencji zmyślone przez okno byłoby obietnicą, której nikt nie złożył
 * (niezmiennik 17), na kontrolce, która puszcza dalej bieg kosztujący pieniądze.
 *
 * # Dlaczego rozbiór jest TUTAJ, a nie w Ruście
 *
 * Bo nie jest kuracją zdarzenia (niezmiennik 15): nie rozstrzyga, CO wiersz znaczy, tylko
 * łamie jeden napis na dwa wiersze tego samego przycisku — tak samo jak `../rail/colour.ts`
 * rozstrzyga barwę podpisu. Dzień, w którym `Line::Asked` zacznie wysyłać dwa pola, jest dniem,
 * w którym ten plik znika, a nie dniem na trzecie miejsce, w którym mieszka ta sama reguła.
 */

/** Opcja rozłożona na to, co robi, i na to, co z tego wyniknie. */
export interface Choice {
  /** Czynność — pierwszy, pogrubiony wiersz przycisku. */
  readonly title: string;
  /** Jedno zdanie o tym, co się stanie; pusty napis, kiedy agent go nie napisał. */
  readonly consequence: string;
}

/**
 * Myślniki, którymi ludzie i modele rozdzielają zdanie od jego dopowiedzenia.
 *
 * OTOCZONE SPACJAMI, i to jest cała ostrożność tego wyrażenia: `re-run`, `gpt-5` i `ship-a-feature`
 * mają łącznik w środku wyrazu, a opcja rozcięta na nim traci połowę tytułu. Pauza (U+2014)
 * i półpauza (U+2013) stoją obok łącznika, bo modele piszą wszystkimi trzema.
 */
const SPLIT = /\s+[—–-]\s+/u;

/** Rozbiór jednej opcji tak, jak napisał ją agent. */
export function choiceOf(option: string): Choice {
  const whole = option.trim();
  const at = SPLIT.exec(whole);
  if (at === null || at.index === 0) return { title: whole, consequence: '' };
  return {
    title: whole.slice(0, at.index).trim(),
    consequence: whole.slice(at.index + at[0].length).trim(),
  };
}

/**
 * Którą opcję wybiera ten klawisz — albo `null`, kiedy żadnej.
 *
 * NUMER JEST POZYCJĄ, NIE NAZWĄ: `1` znaczy „pierwsza z listy, którą agent podał", więc pytanie
 * o dwóch opcjach nie ma odpowiedzi na `3`. Zawijanie („trzeci wraca na pierwszą") wysłałoby
 * agentowi odpowiedź, której człowiek nie wybrał, i nie zostawiłoby po sobie ani śladu —
 * a odpowiedź na punkt kontrolny puszcza dalej bieg za pieniądze.
 *
 * ODDAJE NAPIS OPCJI ZNAK W ZNAK, nie jej tytuł: agent dostaje z powrotem to, co sam podał, więc
 * po tamtej stronie porównanie z listą działa. Rozbiór na tytuł i konsekwencję jest sprawą
 * ekranu i kończy się na ekranie.
 */
export function answerForKey(key: string, options: readonly string[]): string | null {
  if (!/^[1-9]$/u.test(key)) return null;
  return options[Number(key) - 1] ?? null;
}

/**
 * Czy naciśnięcie w ogóle ma prawo odpowiedzieć — zanim spytamy, KTÓRĄ opcję wybiera.
 *
 * PISANIE NIE JEST ODPOWIADANIEM, i to jest cała treść tej reguły. Wiersz wejścia tego ekranu
 * łapie kursor sam (`../index.tsx`, `caretBackToTheField`), więc kursor stoi w polu przez
 * większość czasu — a skrót, który by tego nie widział, zamieniałby każdą wpisaną jedynkę
 * w odpowiedź dla agenta.
 *
 * WARUNKIEM JEST TREŚĆ POLA, NIE SAMO OGNISKO, i to jest różnica między skrótem, który działa,
 * a skrótem, do którego nie da się dojść. Wersja porzucająca KAŻDE zdarzenie z pola tekstowego
 * jest w tej aplikacji martwa: pole jest ogniskiem od chwili, w której ekran się zamontował,
 * więc numer na przycisku obiecywałby klawisz, którego nie ma jak nacisnąć (niezmiennik 16).
 * Puste pole znaczy „nie piszę"; pierwszy wpisany znak oddaje klawiaturę tekstowi i `1` jest
 * od tej chwili zwykłą cyfrą.
 *
 * MODYFIKATOR ZAWSZE PORZUCA: `⌘1` przełącza sekcję (`src/ui/palette/keys.ts`), a dwa znaczenia
 * jednego naciśnięcia to dwie rzeczy, które dzieją się naraz.
 */
export function keyMayAnswer(pressed: {
  readonly modified: boolean;
  readonly typing: boolean;
}): boolean {
  return !pressed.modified && !pressed.typing;
}

/**
 * Czy WŁASNE SŁOWA w ogóle mogą odpowiedzieć na to pytanie — zanim narysujemy pole na nie.
 *
 * ZMIERZONA WADA, KTÓRĄ TO ZAMYKA. Karta potwierdzenia rysowała pole „Your answer" ZAWSZE, także
 * pod przyciskami pytania związanego z hostem. Człowiek, którego zapytano „Stop this run?",
 * wpisywał „yes, stop it", naciskał Send — i widział, jak pytanie znika z ekranu, a pod jego
 * własnym zdaniem staje wiersz „The person did not approve that operation. Nothing changed.".
 * Aplikacja zaprzeczała człowiekowi, który właśnie się zgodził, i zabierała mu przy tym jedyną
 * kontrolkę, którą mógł zgodzić się skutecznie: żeby spróbować jeszcze raz, musiał poprosić
 * lidera o to samo pytanie od nowa. Bieg tymczasem szedł dalej i kosztował pieniądze.
 *
 * DLACZEGO TO NIE BYŁO DO NAPRAWIENIA NA SAMYM ZDANIU. Zgodę wystawia host i wyłącznie na
 * DOSŁOWNY napis przycisku (`src-tauri/src/bridge/library.rs`, `answer_exact`: `affirmative`
 * porównywane co do bajta). Cokolwiek innego — także „Stop run " ze spacją — nie jest zgodą i
 * nigdy nią nie będzie. Pole, które przyjmuje tekst nieumiejący nic autoryzować, jest kontrolką
 * bez skutku (niezmiennik 16), a wygląda gorzej niż jej brak, bo wygląda na rozmowę.
 *
 * TRZY WARUNKI, KAŻDY Z INNEGO POWODU:
 *
 *   `question` — pytanie WIĄZANE PRZEZ HOSTA. Pytanie agenta (bez adresu) przyjmuje każde zdanie
 *   i tak ma zostać: tam własne słowa są całą odpowiedzią, a nie próbą trafienia w napis.
 *
 *   `continue_run` — jedyna droga hosta, na której własne słowa SĄ odpowiedzią. Punkt kontrolny
 *   biegu jedzie z `affirmative: None` (`bridge/library/control.rs`, ramię `continue_run`), więc
 *   host zachowuje oryginał człowieka co do bajta; ekran wie o tej drodze od dawna, bo sam ją
 *   rozgałęzia (`../index.tsx`, `answerQuestion`). Pozostałe operacje hosta — Stop biegu, start
 *   powtórki, przywrócenie plików, trzy operacje aplikacji — podają `Some(napis przycisku)`.
 *
 *   `options` — NIGDY nie zostawiamy karty bez czego nacisnąć. To jest ta sama wada, którą
 *   naprawiono 2026-08-18 (pytanie z zerem kontrolek zatrzymywało workflow na zawsze); pusta
 *   lista opcji znaczy „odpowiedz własnymi słowami", niezależnie od tego, kto pyta.
 */
export function ownWordsMayAnswer(question: {
  readonly options: readonly string[];
  readonly question?: { readonly operation: string };
}): boolean {
  const bound = question.question;
  return bound === undefined || bound.operation === 'continue_run' || question.options.length === 0;
}
