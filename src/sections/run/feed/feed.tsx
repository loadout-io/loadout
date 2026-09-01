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
import { Fragment, useEffect, useState } from 'react';
import { answerForKey, choiceOf, keyMayAnswer } from './choice';
import { Answered, Message } from './message';
import type { FeedView, Question } from './model';
import { EVERYONE, onlyFrom, speakersIn } from './speakers';
import { StreamHead } from './stream-head';

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

  /* NUMER NA PRZYCISKU MA BYĆ PRAWDZIWYM KLAWISZEM.
   *
   * Kwadracik z `1` narysowany nad martwym nasłuchem jest gorszy niż jego brak: obiecuje skrót,
   * po którym nic się nie dzieje (niezmiennik 16), a bieg stoi i kosztuje pieniądze, dopóki
   * człowiek nie odpowie. Nasłuch wisi na dokumencie, bo karta nie ma ogniska — a karta, która
   * ognisko ZABIERA, wyrywałaby kursor z wiersza wejścia w chwili, w której agent zapyta.
   *
   * PISANIE NIE JEST ODPOWIADANIEM — i „pisanie" znaczy tu POLE Z TREŚCIĄ, nie samo ognisko.
   * Powód w całości stoi przy `keyMayAnswer` w `./choice.ts`: wiersz wejścia tego ekranu łapie
   * kursor sam, więc warunek na samym ognisku zabiłby ten skrót w działającej aplikacji.
   *
   * NASŁUCH ŻYJE RAZEM Z KARTĄ. Zdejmuje go `useEffect`, więc kiedy bieg zejdzie i karta zniknie
   * (`./model.ts`, `runEnded` czyści kolejkę pytań), klawisz przestaje odpowiadać agentowi,
   * który już nie pracuje. */
  useEffect(() => {
    function pressed(event: KeyboardEvent): void {
      const on = event.target;
      const inAField =
        on instanceof HTMLInputElement || on instanceof HTMLTextAreaElement
          ? on.value !== ''
          : on instanceof HTMLElement && on.isContentEditable && on.textContent !== '';
      if (
        !keyMayAnswer({
          modified: event.metaKey || event.ctrlKey || event.altKey,
          typing: inAField,
        })
      ) {
        return;
      }
      const chosen = answerForKey(event.key, question.options);
      if (chosen === null) return;
      event.preventDefault();
      onAnswer(question.id, chosen);
    }
    document.addEventListener('keydown', pressed);
    return () => {
      document.removeEventListener('keydown', pressed);
    };
  }, [question.id, question.options, onAnswer]);

  return (
    /* WCHODZI SPRĘŻYNĄ, i to jest jedyne miejsce w tym pliku, które ma do tego prawo.
       DESIGN §7 wymienia kartę pytania wprost jako powierzchnię, która POJAWIA SIĘ nad tym,
       co już jest na ekranie: karta wskakująca skokiem czyta się jak przeskok widoku — oko nie
       wie, czy patrzy na to samo miejsce.

       TON `live`, NIE `attend`, i to jest zmiana z 2026-08-31. Makieta `polecenie.html` (`.ask`)
       daje tej karcie pomarańczową ramkę z poświatą i świecącą krawędź po lewej — a `--color-live`
       jest tą samą pomarańczą, którą bije kropka nad strumieniem. Jedna barwa na „TO stoi i czeka
       na ciebie" zamiast dwóch, które trzeba rozróżniać. */
    <div
      /* ZNACZNIK JEST TU OD 2026-08-31, odkąd karta ma DWA legalne miejsca: pod krokiem, który
         zapytał, i — kiedy takiego kroku nie da się wskazać — na dole strumienia. Kryterium
         musi umieć policzyć, ile ich stoi, a nie da się tego zrobić po tekście pytania: to samo
         zdanie żyje na tym ekranie drugi raz, jako wiersz historii, który zostaje na zawsze. */
      data-asked={question.id}
      data-tone="live"
      className="card enter shrink-0 border-l-2 border-l-live bg-live-soft"
    >
      {/* KTO CZEKA, NIE „ktoś czeka". Zmierzone: bieg tego produktu prowadzi kilku agentów naraz,
          więc zdanie „needs your answer" bez nazwy zostawia człowieka szukającego, KTÓRY z nich
          stanął — a stoją wtedy wszyscy za nim. Nazwa przyjeżdża z pytania (`Question.agent`),
          czyli z podpisu, pod którym Rust je przysłał. */}
      {/* BARWA `--live`, NIE AKCENT. Stopień nadoczka niesie wersaliki i akcent sam
          (`src/styles/theme.css`, `.text-eyebrow`) — wypisanie ich tutaj byłoby drugą kopią
          jednego faktu. Barwę przestawiamy, bo to nadoczko mówi „TO stoi i czeka na ciebie",
          czyli dokładnie to samo, co bijąca kropka nad strumieniem; warstwa `utilities` stoi
          nad `components`, więc `text-live` wygrywa. */}
      <p data-waiting-for={question.agent} className="text-eyebrow text-live">
        {question.agent} is waiting for you
      </p>

      {/* PYTANIE W STOPNIU PYTANIA. `--text-question` istnieje w drabince dokładnie po to
          (17 px, waga 600) i do 2026-08-31 nie miał ani jednego wołającego: karta pisała pytanie
          stopniem prozy, czyli tym samym, którym pisany jest każdy wiersz strumienia obok.
          Rzecz, na której stoi cały bieg, nie ma prawa wyglądać jak wiersz historii. */}
      <p className="mt-2 text-question text-ink">{question.text}</p>

      {question.options.length === 0 ? null : (
        <div className="mt-3 flex flex-wrap gap-2">
          {question.options.map((option, at) => {
            const { title, consequence } = choiceOf(option);
            return (
              <button
                key={option}
                type="button"
                data-choice={at + 1}
                onClick={() => {
                  onAnswer(question.id, option);
                }}
                /* SZEROKI PRZYCISK, nie pastylka: opcja niesie dwa wiersze — czynność i to, co
                   z niej wyniknie — a pastylka mieści jeden. `flex-1` z `basis`, żeby dwie opcje
                   stanęły obok siebie, a cztery zawinęły się po dwie zamiast ścisnąć się w kreski. */
                className="btn h-auto min-w-[220px] flex-1 basis-[240px] items-start gap-3 px-3 py-[11px] text-left"
              >
                {/* NUMER W KWADRACIE. To jest jedyna rzecz na ekranie, która mówi, KTÓRY klawisz
                    odpowiada tą opcją — a nasłuch wyżej jest tym, co czyni ją prawdą. */}
                <kbd
                  data-tone="accent"
                  className="chip h-[22px] w-[22px] rounded-sm px-0 font-mono text-mono-strong"
                >
                  {at + 1}
                </kbd>
                <span className="min-w-0 flex-1">
                  <span className="block text-ui text-ink">{title}</span>
                  {/* ZDANIE KONSEKWENCJI TYLKO WTEDY, GDY AGENT JE NAPISAŁ. Pusty wiersz pod
                      tytułem byłby miejscem, w którym oko szuka treści i jej nie znajduje;
                      wymyślone zdanie byłoby obietnicą, której nikt nie złożył (niezmiennik 17). */}
                  {consequence === '' ? null : (
                    <span className="lead mt-[2px] block whitespace-normal">{consequence}</span>
                  )}
                </span>
              </button>
            );
          })}
        </div>
      )}

      <form onSubmit={send} className="mt-3 flex items-center gap-2">
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
        {/* WYPEŁNIONY AKCENTEM, bo to jest w tej karcie JEDYNA rzecz do naciśnięcia, kiedy
            człowiek napisał własne zdanie — a makieta rysuje wysyłkę jako okrągły, wypełniony
            przycisk po prawej krawędzi pola. Nazwa zostaje słowem, nie strzałką: czytnik ekranu
            i oko dostają to samo zdanie. */}
        <button type="submit" className="btn-primary rounded-pill px-4">
          Send
        </button>
      </form>

      {/* WIERSZ SKRÓTÓW — `.gest` z makiety, ale WYŁĄCZNIE te, które naprawdę coś robią.
          Makieta wymienia cztery; czwarty (`⌘⏎ answer and continue`) nie ma w tej aplikacji
          nasłuchu i wypisany tutaj byłby skrótem, po którym nic się nie dzieje (niezmiennik 16).
          Trzy pozostałe mają: numery odpowiadają nasłuchem wyżej, `/` otwiera listę komend
          w wierszu wejścia (`../entry/entry.tsx`), a `⌘K` paletę (`src/ui/palette/keys.ts`).

          STOI POD KARTĄ, nie pod strumieniem, i gaśnie razem z nią: zdanie „1 or 2 answer" nad
          biegiem, który zszedł, opisuje klawisz, który już nikomu nie odpowiada. */}
      <p data-answer-keys className="value mt-2 flex flex-wrap gap-4 text-meta">
        {question.options.length < 2 ? null : <span>1 or 2 answer</span>}
        <span>/ commands</span>
        <span>⌘K anywhere</span>
      </p>
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

  /**
   * Który podpis jest w mocy. Stan WIDOKU, nie modelu, i to jest wybór.
   *
   * Zawężenie jest tym, na co patrzy JEDNA para oczu przy JEDNYM oknie — nie faktem o biegu.
   * Trzymane w modelu, przestawiałoby się razem z sesją folderu i wracało po przełączeniu
   * zakresu, a strumień, który sam się zawęża, wygląda jak strumień, w którym zniknęli agenci.
   * Wraca do `EVERYONE` przy odmontowaniu kolumny i tak ma być.
   */
  const [showing, setShowing] = useState(EVERYONE);
  const speakers = speakersIn(view.history);
  /* Chip, który zniknął z rzędu — bo zawężenie przeżyło bieg, w którym ten agent mówił — nie ma
   * prawa zostać w mocy: strumień byłby wtedy pusty, a nic na ekranie nie mówiłoby dlaczego
   * (niezmiennik 17). */
  const inForce = showing === EVERYONE || speakers.includes(showing) ? showing : EVERYONE;
  const rows = onlyFrom(view.history, inForce);

  return (
    <section data-feed className="flex min-h-0 flex-1 flex-col">
      {nothingYet ? null : (
        <StreamHead
          speakers={speakers}
          showing={inForce}
          onShow={setShowing}
          /* ŻYWE ZNACZY „KTOŚ TERAZ PRACUJE", i model jest jedynym miejscem, które to wie:
             strefa TERAZ trzyma wiersz na agenta, który IDZIE, a `runEnded` opróżnia ją całą.
             Drugi warunek policzony tutaj byłby drugą odpowiedzią na to samo pytanie
             (niezmiennik 13). */
          live={view.now.rows.length > 0 || view.now.thinking !== null}
          /* Przypięcie do dołu robi układ (`flex-col-reverse` niżej), więc dopóki w strumieniu
             są wiersze, najnowszy stoi pod okiem. */
          following={view.history.length > 0}
        />
      )}

      {nothingYet ? (
        /* PUSTY EKRAN TO ZAPROSZENIE, NIE KOMUNIKAT O BRAKU DANYCH (DESIGN §6) — a od
         * 2026-08-31 zaproszenie ma czym być. Kiedy ekran poda `guide`, stoi ono tutaj: to jest
         * to samo miejsce, ta sama chwila i ten sam brak, tylko odpowiedź jest o dwa piętra
         * konkretniejsza („zacznij od folderu" zamiast „nic tu nie ma").
         *
         * ZDANIE NIŻEJ ZOSTAJE NA SWOIM MIEJSCU i nie jest długiem: bez `guide` ten komponent
         * nie wie o świecie nic poza tym, że wierszy nie ma, i wtedy zdanie o braku wierszy jest
         * dokładnie tym, co umie powiedzieć uczciwie. */
        (guide ?? (
          <div className="flex flex-1 flex-col items-center justify-center gap-3">
            <span className="mark">◇</span>
            <p data-empty className="text-ink">
              Nothing here yet: the work shows up line by line.
            </p>
          </div>
        ))
      ) : (
        <div ref={portRef} className="flex min-h-0 flex-1 flex-col-reverse overflow-y-auto py-2">
          {/* Jedno dziecko kontenera odwróconego: wiersze zostają w swojej kolejności, a to,
              co się odwraca, to kierunek wypełniania — czyli przypięcie do dołu. */}
          <div>
            {/* WYPOWIEDŹ, NIE WIERSZ TRANSKRYPTU. Powód, dla którego ta kolumna ma własny
                kształt wiersza, stoi w całości w nagłówku `./message.tsx`: to jest pierwsza
                powierzchnia biegu, a nie przegląd tysiąca linii.

                KOMENDA JEDZIE Z WIERSZA, i to jest cała droga propozycji do przycisku: model
                przepisuje ją z linii, ten plik podaje ją komponentowi, a `message.tsx` rysuje
                kontrolkę wyłącznie wtedy, gdy ją dostanie (niezmiennik 16). */}
            {rows.map((row) => (
              <Fragment key={row.id}>
                <Message row={row} onToggle={onToggle} command={row.command} />
                {/* TWOJA ODPOWIEDŹ STOI POD PYTANIEM, NA KTÓRE PADŁA, i to jest jedyne miejsce,
                    w którym ma sens: strumień jest zapisem tego, co się wydarzyło, a wybrana
                    opcja jest jedynym śladem tego, w którą stronę bieg został skierowany.
                    Bez tego naciśnięcie `1` zdejmowało kartę i nie zostawiało na ekranie ani
                    jednego znaku — czyli wyglądało dokładnie jak klawisz, który nie zadziałał
                    (DESIGN §8).

                    ŁĄCZONE PO IDENTYFIKATORZE, nie po kolejności: `Answer.questionId` jest
                    identyfikatorem LINII, która zapytała, więc to jest ta sama liczba, którą
                    wiersz historii nosi w `id`. Relacja jest w danych, nie dorysowana
                    (niezmiennik 17). */}
                {view.answers
                  .filter((answer) => answer.questionId === row.id)
                  .map((answer) => (
                    <Answered
                      key={String(answer.questionId) + answer.option}
                      agent={row.agent}
                      option={answer.option}
                    />
                  ))}
              </Fragment>
            ))}
            {/* ZAWĘŻENIE, KTÓRE NIC NIE ZOSTAWIŁO, MÓWI O SOBIE. Pusta kolumna po naciśnięciu
                chipa czyta się jak bieg, który zniknął — a zniknął tylko jeden wątek. Zdanie
                nazywa podpis i nazywa drogę powrotną, bo chip `All` stoi wtedy nad nim. */}
            {rows.length > 0 ? null : (
              <p data-narrowed-to-nothing className="lead px-[18px] py-3">
                {'Nothing from ' + inForce + ' in this run yet — press All to see everyone.'}
              </p>
            )}
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
        <div className="flex shrink-0 justify-end px-[18px] pt-1">
          <button type="button" onClick={onJumpToNewest} className="btn-quiet">
            Jump to newest
          </button>
        </div>
      )}
    </section>
  );
}
