/* Strefa HISTORII — jedyna, która przyrasta [DESIGN §1].
 *
 * PRZYPIĘCIE DO DOŁU ROBI UKŁAD, NIE SKRYPT. Kontener jedzie w `flex-col-reverse`, więc treść
 * sama siedzi przy dolnej krawędzi, a najnowszy wiersz stoi pod `scrollTop === 0`. To jest cała
 * implementacja „widok nie wyrywa zdania spod oczu": nie ma `useEffect`, który przewija po
 * paczce, więc nie ma czego wyłączać ani czym warunkować. Jedyne wywołanie portu w tym drzewie
 * wychodzi z przycisku `Jump to newest` — i to jest jedyna droga imperatywna, jaką model ma.
 *
 * `Jump to newest` jest widoczny zawsze, kiedy jest dokąd skakać, i to jest decyzja, nie
 * przeoczenie: warunek „pokaż, gdy użytkownik odjechał od dołu" wymaga ODCZYTU pozycji, a
 * odczyt jest dotknięciem portu — czyli dokładnie tym, czego kryterium 1 zabrania. Kontrolka,
 * która zawsze coś robi, jest tańsza niż kontrolka, która wie, kiedy się pokazać.
 *
 * PYTANIE MA TU JEDNO ŻYWE MIEJSCE. Wiersz `asked` zostaje w historii jako zapis tego, co się
 * wydarzyło, ale przyciski odpowiedzi są WYŁĄCZNIE w bloku przyklejonym — dwa komplety
 * przycisków na to samo pytanie to dwa miejsca, w których bieg da się odblokować, i pierwszy
 * rozjazd między nimi jest cichy (niezmiennik 13).
 */
import type { FormEvent, ReactElement, ReactNode } from 'react';
import { useState } from 'react';
import { Line } from './line';
import type { FeedView, Question } from './model';

export interface FeedProps {
  view: FeedView;
  /** Element, po którym jeździ port przewijania. Podpina go ekran. */
  portRef: (element: HTMLDivElement | null) => void;
  onToggle: (rowId: number) => void;
  onAnswer: (questionId: number, option: string) => void;
  onJumpToNewest: () => void;
  /**
   * Czy pytanie stoi już PRZY KROKU, który je zadał — wtedy tutaj go nie ma.
   *
   * 2026-08-31 — TO NIE JEST DRUGI WARUNEK NA „CZY BIEG ŻYJE", i to jest cała różnica wobec
   * akapitu w `./model.ts` przy `runEnded`. Tamten zakazuje pytać drugi raz o LIVENESS: karta
   * wisi na samym `pinned`, a odpowiedź „czy cokolwiek żyje" ma jedno miejsce. Ten props
   * odpowiada na inne pytanie — GDZIE ta jedna karta stoi — i rozstrzyga je ten, kto widzi
   * oba miejsca naraz (`../index.tsx`), z tego samego planu, z którego rysuje się obraz.
   *
   * Domyślnie `false`, bo dwa cudze kryteria stawiają ten komponent bez obrazu obok
   * (`./answer-card-dies-with-the-run.test.tsx`, `./suggestion-has-a-button.test.tsx`), a wtedy
   * dół strumienia jest jedynym miejscem, w którym pytanie ma gdzie stanąć. Kiedy kroku nie da
   * się wskazać — pyta lider albo pod-agent rozpuszczony w biegu, których na obrazie nie ma —
   * karta zostaje TUTAJ, zamiast zniknąć z ekranu razem z biegiem, który stanie na niej na
   * zawsze (niezmiennik 17: brak kroku znaczy „nie wiemy", nigdy „nie ma pytania").
   */
  askedAtItsStep?: boolean;
  /**
   * Co postawić ZAMIAST zdania o braku danych, kiedy historia jest pusta.
   *
   * 2026-08-31 — PO CO TEN SZEW ISTNIEJE. Ten komponent umie odpowiedzieć na jedno pytanie:
   * czy w strumieniu są wiersze. Na pustym strumieniu przed pierwszym biegiem prawdziwa
   * odpowiedź brzmi inaczej — „nie ma jeszcze folderu, agenta ani workflow" — a tego ten plik
   * nie wie i wiedzieć nie ma prawa: trzy listy z dysku należą do ekranu, nie do strumienia
   * (niezmiennik 23). Ekran podaje więc gotowy blok, a ten wybiera MIEJSCE, w którym on stanie.
   *
   * Domyślnie nieobecny, i to nie jest wygoda: sześć cudzych kryteriów stawia ten komponent
   * samodzielnie, a strumień bez podanego bloku ma się dalej czytać dokładnie tak, jak dotąd.
   */
  guide?: ReactNode;
}

/* CZEGO TU JUŻ NIE MA: dwóch stałych `SECONDARY` i `QUIET` z listą klas przycisku spisaną
 * ręcznie z DESIGN §6. Były kopią decyzji, a kopia nie ma jak zostać zgodna z oryginałem —
 * zmierzone 2026-08-31: przycisk drugoplanowy miał w repo 8 wystąpień pod 3 nazwami, cichy
 * 14 pod 4, i różniły się już geometrią. Od tego dnia rola ma nazwę: `.btn` i `.btn-quiet`
 * w `@layer components`, i to one niosą także cztery stany, których żaden przycisk tego pliku
 * nie miał ani jednego (najechanie, wciśnięcie, skupienie, wyłączenie). */

export interface AskedProps {
  question: Question;
  onAnswer: (questionId: number, option: string) => void;
}

/** Zachęta pola odpowiedzi. Zdanie, nie opis stanu (DESIGN §6). */
export const ANSWER_PROMPT = 'Type your answer and press Enter';

/**
 * Pytanie do człowieka, przyklejone [T2 §7.2 wiersz 10].
 *
 * Kolor `--attend` odpowiada na jedno pytanie: co czeka na MOJĄ decyzję (DESIGN §3). Opcje
 * przychodzą z linii, nigdy stąd: pytanie z opcjami dopisanymi w widoku odpowiada agentowi coś,
 * czego nie pytał.
 *
 * POLE TEKSTOWE JEST ZAWSZE, I TO JEST NAPRAWA, NIE OZDOBA. Zmierzone 2026-08-18: Rust wysyła
 * `options: Vec::new()` w KAŻDYM punkcie kontrolnym (`commands::run::ask`), a ten blok rysował
 * wyłącznie przyciski z tej listy — czyli kartę „Needs your answer" z ZEREM kontrolek. Każdy
 * workflow z kafelkiem punktu kontrolnego był przez to nieukończalny: pytanie stało na ekranie
 * i nie było czym na nie odpowiedzieć. Przyciski zostają tam, gdzie opcje naprawdę są: wybór
 * z trzech jest szybszy niż przepisywanie jednej z nich ręcznie.
 *
 * GDZIE TA TREŚĆ JEDZIE, i to jest druga połowa naprawy. `answer()` stawia ją w `view.toCarry`,
 * czyli w kolejce wysyłkowej o pojemności jednego zdania, a zabiera ją stąd kontrolka „dalej":
 * `continue_run` bierze po tamtej stronie `answer: Option<String>` (`src-tauri/src/ipc.rs`)
 * i podaje je agentowi razem z podbiciem licznika zgód. Ten blok nie woła komendy sam i nie ma
 * prawa: bieg puszcza JEDNA kontrolka w całej aplikacji, a druga byłaby drugim miejscem, z
 * którego da się odblokować bieg — pierwszy rozjazd między nimi jest cichy (niezmiennik 13).
 */
export function Asked({ question, onAnswer }: AskedProps): ReactElement {
  const [typed, setTyped] = useState('');

  function send(event: FormEvent<HTMLFormElement>): void {
    /* Bez tego przeglądarka przeładowuje stronę i bieg znika razem z nią — okno Tauri nie ma
     * dokąd nawigować, a magazyny żyją na poziomie modułu. */
    event.preventDefault();
    /* Puste Enter nie jest odpowiedzią. Wysłane, zdjęłoby pytanie z ekranu i zostawiło bieg
     * stojący na czymś, o czym okno już nie mówi. */
    if (typed.trim() === '') return;
    onAnswer(question.id, typed.trim());
    setTyped('');
  }

  return (
    /* WCHODZI SPRĘŻYNĄ, i to jest jedyne miejsce w tym pliku, które ma do tego prawo.
       DESIGN §7 wymienia kartę pytania wprost jako powierzchnię, która POJAWIA SIĘ nad tym,
       co już jest na ekranie: karta wskakująca skokiem czyta się jak przeskok widoku — oko nie
       wie, czy patrzy na to samo miejsce. `.enter` niesie `--duration` i krzywą osobno, więc nie
       wpada w pułapkę skrótu `animation`, w której drugi czas staje się opóźnieniem.

       Ton idzie atrybutem, nie klasą-bliźniakiem: `[data-tone]` bije samą klasę niezależnie od
       kolejności reguł, a `.card-attend` obok `.card` byłoby drugim napisem do ręcznego
       utrzymania. Lewa krawędź i wypełnienie zostają klasami narzędziowymi — te wygrywają
       z prymitywem, bo warstwa `utilities` stoi nad `components`. */
    <div
      /* ZNACZNIK JEST TU OD 2026-08-31, odkąd karta ma DWA legalne miejsca: pod krokiem, który
         zapytał, i — kiedy takiego kroku nie da się wskazać — na dole strumienia. Kryterium
         musi umieć policzyć, ile ich stoi, a nie da się tego zrobić po tekście pytania: to samo
         zdanie żyje na tym ekranie drugi raz, jako wiersz historii, który zostaje na zawsze. */
      data-asked={question.id}
      data-tone="attend"
      className="card enter shrink-0 border-l-2 border-l-attend bg-attend-soft"
    >
      <p className="label text-attend">Needs your answer</p>
      {/* `text-body` obok `text-ink` było DWIEMA barwami na jeden napis, nie stopniem i barwą:
          zmierzone 2026-08-31 kompilacją arkusza — przy zdefiniowanym `--color-body` Tailwind
          rozstrzyga `text-body` jako barwę i stopnia nie wypisuje wcale, a w gotowym arkuszu
          `.text-ink` stoi za `.text-body`, więc wygrywał już wcześniej. Napis znika bez zmiany
          na ekranie; stopień prozy jest odziedziczony z `body`. */}
      <p className="mt-1 text-ink">{question.text}</p>

      {question.options.length === 0 ? null : (
        <div className="mt-2 flex flex-wrap gap-2">
          {question.options.map((option) => (
            <button
              key={option}
              type="button"
              onClick={() => onAnswer(question.id, option)}
              className="btn"
            >
              {option}
            </button>
          ))}
        </div>
      )}

      <form onSubmit={send} className="mt-2 flex items-center gap-2">
        <input
          aria-label="Your answer"
          placeholder={ANSWER_PROMPT}
          spellCheck={false}
          value={typed}
          onChange={(event) => {
            setTyped(event.target.value);
          }}
          className="field flex-1"
        />
        <button type="submit" className="btn">
          Send
        </button>
      </form>
    </div>
  );
}

export function Feed({
  view,
  portRef,
  onToggle,
  onAnswer,
  onJumpToNewest,
  askedAtItsStep = false,
  guide,
}: FeedProps): ReactElement {
  /* Nic w historii i nikogo w strefie TERAZ znaczy: biegu jeszcze nie było. Sam pusty strumień
   * przy pracujących agentach to co innego — wtedy zaproszenie kłamałoby o stanie maszyny. */
  const nothingYet = view.history.length === 0 && view.now.rows.length === 0;

  return (
    <section data-feed className="flex min-h-0 flex-1 flex-col gap-2">
      {nothingYet ? (
        /* PUSTY EKRAN TO ZAPROSZENIE, NIE KOMUNIKAT O BRAKU DANYCH (DESIGN §6) — a od
         * 2026-08-31 zaproszenie ma czym być. Kiedy ekran poda `guide`, stoi ono tutaj: to jest
         * to samo miejsce, ta sama chwila i ten sam brak, tylko odpowiedź jest o dwa piętra
         * konkretniejsza („zacznij od folderu" zamiast „nic tu nie ma").
         *
         * ZDANIE NIŻEJ ZOSTAJE NA SWOIM MIEJSCU i nie jest długiem: bez `guide` ten komponent
         * nie wie o świecie nic poza tym, że wierszy nie ma, i wtedy zdanie o braku wierszy jest
         * dokładnie tym, co umie powiedzieć uczciwie. Przycisku i „Type /plan to start" nie ma
         * tu dalej z tego samego powodu, co przedtem: zaproszenie wskazujące na kontrolkę,
         * której ten komponent nie ma, jest gorsze niż zdanie mniej (niezmiennik 16). */
        (guide ?? (
          <div className="flex flex-1 flex-col items-center justify-center gap-3">
            {/* Znak pustego ekranu jest ROLĄ, nie napisem: `.mark` niesie 40 px, ramkę kreskowaną
              i promień pojemnika treści. Ten znak był jedną z dziewięciu ręcznych kopii tej samej
              rzeczy, rysowaną w 32 px; DESIGN §6 rozstrzyga tę rozbieżność na 40 — tyle ma
              prymityw, który już istniał (`src/ui/primitives/empty-state.tsx`). */}
            <span className="mark">◇</span>
            <p data-empty className="text-ink">
              Nothing here yet: the work shows up line by line.
            </p>
          </div>
        ))
      ) : (
        <div ref={portRef} className="flex min-h-0 flex-1 flex-col-reverse overflow-y-auto">
          {/* Jedno dziecko kontenera odwróconego: wiersze zostają w swojej kolejności, a to,
              co się odwraca, to kierunek wypełniania — czyli przypięcie do dołu. */}
          <div>
            {/* KOMENDA JEDZIE Z WIERSZA, i to jest cała droga propozycji do przycisku: model
                przepisuje ją z linii, ten plik podaje ją komponentowi, a `line.tsx` rysuje
                kontrolkę wyłącznie wtedy, gdy ją dostanie. Bez tej jednej właściwości przycisk
                startu istnieje tylko w teście — czyli jest kontrolką, której nikt nie zobaczy
                (niezmiennik 16). O tym, CZY on w ogóle jest, rozstrzyga rodzaj wiersza, czyli
                decyzja podjęta w Ruście; ta linia niczego nie rozpoznaje. */}
            {view.history.map((row) => (
              <Line key={row.id} row={row} onToggle={onToggle} command={row.command} />
            ))}
          </div>
        </div>
      )}

      {/* Karta stoi tu wtedy i tylko wtedy, gdy nie stoi PRZY SWOIM KROKU — powód w całości
          przy `FeedProps.askedAtItsStep`. Warunek jest o MIEJSCU, nigdy o tym, czy bieg żyje:
          tamto gasi model i tylko model (`./model.ts`, `runEnded`). */}
      {view.pinned === null || askedAtItsStep ? null : (
        <Asked question={view.pinned} onAnswer={onAnswer} />
      )}

      {view.history.length === 0 ? null : (
        <div className="flex shrink-0 justify-end">
          <button type="button" onClick={onJumpToNewest} className="btn-quiet">
            Jump to newest
          </button>
        </div>
      )}
    </section>
  );
}
