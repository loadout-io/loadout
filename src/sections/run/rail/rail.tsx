/* Rzeczy, które człowiek uruchomił komendą — i wyjście każdej z nich.
 *
 * 2026-08-31 — CZEGO W TYM PLIKU JUŻ NIE MA: KOLUMNY AGENTÓW. Stała tu prawa kolumna ekranu
 * pracy z kafelkiem na agenta, i była TRZECIĄ kopią jednego zdania: strumień mówił „Check —
 * Ran the checks, they did not work", blok TERAZ pod nim mówił to samo, kafelek obok mówił to
 * po raz trzeci. Limit żywych regionów na fakt wynosi 1 (niezmiennik 13). Agenci stoją teraz
 * na kafelkach planu (`../graph/`), czyli w tym samym obrazie, który człowiek sam ułożył —
 * a `roster.ts`, `card.ts`, `say.ts`, `colour.ts`, `again.ts` i `processes.ts` zostały co do
 * linii i to one dalej liczą, kto jest w biegu i co powiedział. Zniknął widok, nie polityka.
 *
 * DLACZEGO TE DWIE RZECZY ZOSTAŁY WŁAŚNIE TUTAJ. Rzecz uruchomiona komendą NIE JEST agentem
 * (`./processes.ts`) — nie ma kroku w planie, nie ma czego narysować na płótnie i nie pisze do
 * strumienia ani jednej linii. Kafelek jest całym jej śladem, więc znika razem z kolumną tylko
 * wtedy, gdy ktoś nie zauważy, że to dwie różne listy. Stąd osobna grupa, obok obrazu planu.
 *
 * TA GRUPA NIE PODEJMUJE ANI JEDNEJ DECYZJI O TREŚCI. Nie wybiera zdania, nie liczy stanu, nie
 * przypisuje koloru — bierze gotowe `RailCard` z `./processes.ts` i zamienia je na markup.
 * Gdyby wybierała, polityka „kto to powiedział" istniałaby w dwóch miejscach (niezmiennik 23).
 *
 * KOLOR KWADRATU TO TOŻSAMOŚĆ, NIGDY STAN [DESIGN §3, „Tożsamość ≠ stan"]. Ta reguła powstała,
 * bo referencyjny poprzedni prototyp dawał agentowi Forge dokładnie ten sam hex, co „wymaga uwagi",
 * i na jednym ekranie dwie różne rzeczy znaczyły to samo.
 */
import type { ReactElement } from 'react';
import { useEffect, useSyncExternalStore } from 'react';
import { saidOf } from '../entry/echo';
import { runFeed } from '../feed/live';
import type { RailCard } from './card';
import { statusToken } from './colour';
import type { Held } from './processes';
import {
  closeStarted,
  openStarted,
  openedStarted,
  railGroups,
  refreshStarted,
  startedThings,
  stopStarted,
  subscribeToStarted,
} from './processes';

/**
 * Jak często ta grupa pyta rejestr, co jeszcze biegnie — w milisekundach.
 *
 * Sekunda, i to jest wybór z zapisaną ceną. W górę: kafelek nad rzeczą, która zeszła, stoi tyle,
 * ile wynosi ten odstęp, a „Running" nad komendą zeszłą pięć sekund temu jest tym samym
 * kłamstwem, co nad zeszłą dwie minuty temu, tylko krócej. W dół: pytanie jest jednym przejściem
 * granicy i jednym wzięciem zamka, więc dziesięć razy częstsze nie kupuje nic, czego człowiek by
 * zobaczył — oko nie odróżnia 100 ms od 1 s przy kafelku, który stoi godzinami.
 *
 * Odstęp, nie zdarzenie z drutu, i powód stoi w nagłówku `./processes.ts`: kanał do okna wiezie
 * WIERSZE STRUMIENIA, a rzecz uruchomiona komendą nie jest agentem i nie ma w strumieniu czego
 * pisać (niezmiennik 17).
 */
const ASK_AGAIN = 1_000;

/* `.btn-quiet` z `theme.css` — prymityw, nie kopia DESIGN §6. Ta sama nazwa stoi w strumieniu,
 * na ekranie agenta i w historii, wiec zmiana wysokosci cichego przycisku dociera dzis do
 * wszystkich trzech naraz; do 2026-08-31 byla to jedna z siedmiu recznych kopii. */
const QUIET = 'btn-quiet';

/**
 * Zdanie, którym Rust odpowiada na powtórzenie kroku → DO STRUMIENIA, nie do slotu obok.
 *
 * Odpowiedź przychodzi wtedy i tylko wtedy, gdy dzisiejszy plik workflow różni się od tego,
 * który wtedy biegł: „to samo jeszcze raz" i „to samo z twoją poprawką" nie mogą wyglądać
 * identycznie. Zdanie bez miejsca do wylądowania jest ciszą, a cisza po naciśnięciu wygląda
 * dokładnie jak przycisk, który nic nie robi (niezmiennik 16).
 *
 * DO STRUMIENIA, bo rozmowa z Loadoutem jest JEDNĄ historią — tą samą drogą idzie odmowa startu
 * (`../index.tsx`) i echo wiersza wejścia (`../entry/entry.tsx`). Wiersz w `useState` komponentu
 * ginąłby przy pierwszym wyjściu do innej sekcji, a bieg trwa dłużej niż ekran.
 *
 * FUNKCJA MODUŁOWA, nie domknięcie w komponencie, i to jest ten sam powód, co przy `../session
 * /open.ts`: to repo nie ma jsdom, więc handler zamknięty w komponencie byłby kodem, którego
 * żadne kryterium nie umie dotknąć.
 */
export function sayAfterRunningAgain(said: string): void {
  runFeed.appendLines([saidOf(said)]);
}

/**
 * Jedna linia tekstu kafelka.
 *
 * `data-card-line` niosą wszystkie cztery i tylko one — po tym atrybucie liczy się sufit
 * z ARCHITECTURE §7. Linia bez wartości nie istnieje: pusty slot dalej zajmuje wysokość
 * i dalej wygląda jak fakt, którego nie znamy, zamiast jak fakt, którego nie ma.
 */
function CardLine({ text, className }: { text: string; className: string }): ReactElement | null {
  if (text === '') return null;
  return (
    <span data-card-line className={className}>
      {text}
    </span>
  );
}

/**
 * Grupa „Started" plus wyjście tej jednej rzeczy, w którą człowiek wszedł.
 *
 * ZERO KAFELKÓW TO ZERO NAGŁÓWKA (niezmiennik 17, DESIGN §6): nagłówek nad pustką jest gorszy
 * niż jego brak, bo obiecuje listę, na którą nic nigdy nie wejdzie. Kiedy nic nie biegnie, ten
 * komponent nie rysuje ani jednego piksela.
 */
export function StartedThings(): ReactElement {
  /* MAGAZYNY CZYTAMY PRZEZ `useSyncExternalStore` Z BIEŻĄCYM STANEM JAKO MIGAWKĄ SERWEROWĄ.
   * `renderToStaticMarkup` jest rendererem serwerowym, a ta aplikacja nigdy nie hydratuje
   * serwerowego HTML-a — więc powód, dla którego React chce tam stanu początkowego, tutaj nie
   * istnieje. Ten sam zapis stoi w `../session/mount.tsx` i w `../index.tsx`. */
  const started = useSyncExternalStore(subscribeToStarted, startedThings, startedThings);
  const opened = useSyncExternalStore(subscribeToStarted, openedStarted, openedStarted);

  /* ODŚWIEŻANIE JEST JEDYNĄ DROGĄ, KTÓRĄ TA GRUPA DOWIADUJE SIĘ O ŚMIERCI. Rzecz uruchomiona
   * komendą nie jest agentem i nie ma w strumieniu czego pisać (niezmiennik 17), więc kanał
   * biegu jej nie wiezie — a kafelek ma istnieć dokładnie tak długo, jak ona.
   *
   * Pierwsze pytanie idzie od razu, nie po sekundzie: po przeładowaniu okna magazyn jest pusty,
   * a rejestr po tamtej stronie granicy żyje dalej i wie o wszystkim, co jeszcze biegnie.
   *
   * `renderToStaticMarkup` nie odpala efektów, więc cudze kryteria montujące ten ekran nie
   * wołają tędy ani jednego `invoke` — grupa sądzona jest wtedy za to, co jej podano. */
  useEffect(() => {
    void refreshStarted();
    const asking = setInterval(() => {
      void refreshStarted();
    }, ASK_AGAIN);
    return () => {
      clearInterval(asking);
    };
  }, []);

  /* PUSTA LISTA AGENTÓW NIE JEST TU BŁĘDEM: ta funkcja rozdziela dwie listy, a ten komponent
   * rysuje wyłącznie tę drugą. Agenci mają dziś własne miejsce — kafelki planu — i przepisanie
   * ich tutaj byłoby dokładnie tą trzecią kopią, którą to zadanie zdjęło z ekranu. */
  const groups = railGroups({ agents: [], started });
  const inside = started.find((one) => one.id === opened) ?? null;

  return (
    <>
      {groups.started.length === 0 ? null : (
        /* RZECZY URUCHOMIONE KOMENDĄ MAJĄ WŁASNĄ GRUPĘ, i to jest cała różnica, którą ta lista
           ma pokazać: jedno Loadout prowadzi, drugie człowiek kazał uruchomić, a „stop" pod
           każdym znaczy co innego. Kolor kwadratu ich NIE rozróżnia — kolor jest tożsamością,
           nigdy stanem [DESIGN §3]; rozróżnia je miejsce. */
        <div
          data-started-list
          /* WŁASNY SUFIT WYSOKOŚCI, i to jest naprawa zgłoszona ze zrzutu właściciela
             („jak dużo step to nam wyjebuje UI", 2026-08-22): grupa stała na `auto`, więc
             rosła z każdą uruchomioną rzeczą i spychała to, co pod nią, poza okno. */
          className="grid max-h-[50%] shrink-0 content-start gap-[6px] overflow-auto border-b border-line px-[10px] pt-3 pb-3"
        >
          <h2 className="px-[4px] font-mono text-eyebrow text-muted">Started</h2>
          {groups.started.map((card) => (
            <StartedTile
              key={card.id}
              card={card}
              held={started.find((one) => one.id === card.id) ?? null}
            />
          ))}
        </div>
      )}

      {/* Wyjście jednej rzeczy uruchomionej komendą. Zakrywa całe okno, a nie kolumnę — a rzecz
          pod nim biegnie dalej. */}
      {inside === null ? null : <StartedOutput held={inside} />}
    </>
  );
}

/**
 * Kafelek rzeczy uruchomionej komendą: cała powierzchnia otwiera jej wyjście, a Stop stoi obok.
 *
 * DWA PRZYCISKI, NIE JEDEN, bo odpowiadają na dwa różne pytania: „pokaż mi, co ona mówi" i „skończ
 * ją". Zagnieżdżony byłby markupem, którego przeglądarka nie przyjmuje, a jeden przycisk na oba
 * znaczyłby, że wejście w kafelek zabija to, w co się weszło.
 *
 * STOP RYSUJE SIĘ WYŁĄCZNIE PRZY ZNANEJ GRUPIE. Dopóki Rust nie odpowiedział, którą to grupa, nie
 * ma czego ubić — a przycisk, który nie ma czego zrobić, jest kontrolką bez handlera
 * (niezmiennik 16). Trwa to jedno wywołanie, więc człowiek go w praktyce nie zobaczy zgaszonego.
 */
function StartedTile({ card, held }: { card: RailCard; held: Held | null }): ReactElement {
  const pgid = held?.pgid ?? null;
  return (
    <span
      data-started={card.id}
      className="grid grid-cols-[minmax(0,1fr)_auto] items-stretch gap-[6px]"
    >
      <button
        type="button"
        onClick={() => {
          openStarted(card.id);
        }}
        /* Ten sam prymityw, co u kafelka planu — jedna decyzja o wygladzie kafelka, nie dwie. */
        data-interactive=""
        className="card grid grid-cols-[22px_minmax(0,1fr)] gap-[9px] rounded-sm bg-transparent px-[10px] py-[9px] text-left"
      >
        {/* Kwadrat tożsamości, z tej samej przygaszonej palety, co u agentów: różnicę niesie
            miejsce na liście, nigdy odcień [DESIGN §3]. Inicjał jest pierwszym znakiem komendy,
            bo komenda JEST nazwą tej rzeczy. */}
        <span
          aria-hidden
          className="grid size-[22px] place-items-center font-mono text-mono-strong text-ink"
          style={{ background: `var(${card.square})` }}
        >
          {card.name.slice(0, 1)}
        </span>

        <span className="grid min-w-0">
          <CardLine text={card.name} className="truncate font-mono text-mono-strong text-ink" />
          <CardLine text={card.role} className="truncate font-mono text-label text-muted" />
          <CardLine text={card.say.text} className="mt-[3px] truncate text-body" />
          {/* Stan jest SŁOWEM w kolorze nasyconym — nigdy kolorem kwadratu [DESIGN §3]. */}
          <span
            data-card-line
            data-status
            className="mt-[5px] font-mono text-label"
            style={{ color: `var(${statusToken(card.status)})` }}
          >
            {card.status}
          </span>
        </span>
      </button>

      {pgid === null ? null : (
        <button
          type="button"
          aria-label={'Stop ' + card.name}
          onClick={() => {
            void stopStarted(card.id);
          }}
          className={QUIET}
        >
          Stop
        </button>
      )}
    </span>
  );
}

/**
 * Wyjście tej jednej rzeczy — to, co otwiera kliknięcie w kafelek.
 *
 * Zamówienie właściciela brzmiało „po kliku mogę tam wejść", więc panel niesie dwie rzeczy
 * i tylko dwie: KTÓRĄ komendę pokazuje i co ona powiedziała. Kafelek, w który da się wejść i nie
 * ma tam nic, jest kontrolką bez skutku z dodatkowym krokiem (niezmiennik 16).
 *
 * KOMENDA W NAGŁÓWKU, bo bez niej ten panel wygląda identycznie dla każdej rzeczy — a przy dwóch
 * biegnących człowiek czyta jedną i patrzy na drugą.
 */
function StartedOutput({ held }: { held: Held }): ReactElement {
  return (
    /* `.enter`: ten panel POJAWIA sie po kliknieciu w kafelek. Sprezyna wylacznie na wejsciu
       (DESIGN §7) i wylacznie na tej jednej powierzchni — jeden region na jedno zdarzenie. */
    <div className="enter fixed inset-0 z-10 grid grid-rows-[auto_minmax(0,1fr)] bg-bg">
      {/* `.screen-head` niesie 52 px z ARCHITECTURE §7, a material bierze z `.glass` obok:
          pasek bez klasy materialu przestaje sluchac `prefers-reduced-transparency`. */}
      <div className="screen-head glass">
        <button type="button" aria-label="Back to the run" onClick={closeStarted} className={QUIET}>
          ←
        </button>
        <h1 className="min-w-0 truncate font-mono text-mono-strong text-ink">{held.command}</h1>
        {held.pgid === null ? null : (
          <button
            type="button"
            onClick={() => {
              void stopStarted(held.id);
            }}
            className={QUIET + ' ml-auto'}
          >
            Stop
          </button>
        )}
      </div>

      <div data-started-output className="screen-body">
        {/* Nagłówek panelu niesie komendę raz; tutaj stoi drugi raz i to jest świadome: kryterium
            czyta TEN region i pyta, czy to, co się otworzyło, należy do tej właśnie linii.
            Bez tego panel odpowiadałby na „czy cokolwiek się otworzyło", a to jest słaba wersja
            tego samego zdania. `.value`, bo komenda jest wartoscia maszynowa — do przepisania
            znak w znak, wiec kroj wchodzi razem ze stopniem (DESIGN §4), a nie obok niego. */}
        <p className="value pb-[9px]">{held.command}</p>
        {held.said === '' ? (
          /* PUSTKA MÓWI, ŻE JEST PUSTA, i mówi to zdaniem, nie kreską: rzecz, która jeszcze nic
             nie napisała, wygląda tak samo jak panel, który nie umie nic pokazać. */
          <p data-empty className="lead">
            Nothing has come back from it yet.
          </p>
        ) : (
          /* `pre`, bo to jest tekst, który ktoś inny sformatował: sklejone spacje i zgubione
             złamania linii zamieniają tabelkę z `npm` w jeden akapit. Kuracja tego strumienia
             nie istnieje z rozmysłu (decyzja D4: kurowany strumień jest dla AGENTÓW, a to nie
             jest agent) — i nie udaje terminala, bo nie ma tu ani kolorów, ani kursora. */
          <pre className="whitespace-pre-wrap break-words font-mono text-mono text-body">
            {held.said}
          </pre>
        )}
      </div>
    </div>
  );
}
