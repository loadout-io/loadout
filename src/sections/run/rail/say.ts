/* Jedno zdanie na kafelku — i kto je powiedział.
 *
 * Cicha porażka, przed którą stoi ten plik: „latest note from this agent" karmione
 * czymkolwiek, co przyszło ostatnie. Agent pisze prozą, potem padają sprawdzenia, kafelek
 * pokazuje „3 of 40 tests failed" — i podaje to jako CYTAT AGENTA. Sprawdzenia to Loadout,
 * nie agent [FOUNDATIONS §2.2]: to jest ten sam błąd co blok „co wyprodukował" karmiony
 * ostatnią wiadomością agenta, tylko mniejszą czcionką i dlatego trudniejszy do zauważenia.
 *
 * Stąd `who` obok tekstu, zawsze, a nie „gdy się przyda". Zdanie bez podpisu autorytetu
 * czyta się jak fakt niezależnie od tego, czym jest.
 */
import type { Who } from '../../../state/run';
import type { Kind } from '../feed/kinds';
import type { Say } from './card';

/**
 * Trzy autorytety w całej aplikacji, nie osiem [FOUNDATIONS §2.2].
 *
 * `Record<Who, true>`, nie tablica literałów, i to jest cała obrona: czwarty autorytet
 * dopisany kiedyś do `Who` przestaje TU się kompilować, zamiast po cichu wjechać na ekran
 * jako czwarte słowo, którego nikt nie zdefiniował. Ta sama sztuczka, co rejestr rodzajów
 * linii w `src/sections/run/feed/kinds.ts`.
 */
const AUTHORITY: Readonly<Record<Who, true>> = { agent: true, loadout: true, you: true };

/** Zamknięty zbiór autorytetów jako wartość — typ nie istnieje w czasie wykonania. */
export const AUTHORITIES: readonly Who[] = Object.keys(AUTHORITY) as Who[];

/**
 * Kto napisał zdanie, które niesie wiersz tego rodzaju.
 *
 * Rozstrzyga RODZAJ, nie kolejność i nie treść. Implementacja biorąca „to, co przyszło
 * ostatnie" i podpisująca to agentem myli się dokładnie tam, gdzie to boli: `3 of 40 failed`
 * policzyły sprawdzenia, a nie agent, więc podane jako cytat agenta jest `agent said`
 * w rubryce `happened` [FOUNDATIONS §2.2].
 *
 * Dwa rodzaje należą do agenta i oba są jego własnymi słowami: proza (`note`) i pytanie,
 * które zadał (`asked`). Całą resztę — od `Read 6 files` po `Finished in 4m 12s` — pisze
 * mapper po stronie Rusta, czyli Loadout, choćby opisywała cudzą pracę.
 *
 * `Record<Kind, Who>`, nie `switch` z gałęzią domyślną: piętnasty rodzaj dopisany po stronie
 * Rusta przestaje TU się kompilować, zamiast po cichu dostać cudzy podpis.
 */
const AUTHOR: Readonly<Record<Kind, Who>> = {
  run: 'loadout',
  step: 'loadout',
  agent: 'loadout',
  note: 'agent',
  /* CIEBIE — i to jest pierwszy użytkownik trzeciego autorytetu, który stał w `Who` od początku
   * i do 2026-08-19 nie miał ani jednego rodzaju. Zdanie, które napisał człowiek, podpisane
   * `agent` byłoby cytatem przypisanym komuś, kto go nie wypowiedział; podpisane `loadout`
   * udawałoby komunikat systemu. */
  told: 'you',
  /* Trzeci rodzaj należący do agenta: propozycja biegu jest jego własnymi słowami — to lider
   * patrzy na projekt i mówi, co warto uruchomić. Podpisana `loadout` czytałaby się jak
   * komunikat aplikacji, czyli ten sam błąd, co kafelek cytujący „3 of 40 checks failed" jako
   * zdanie agenta [FOUNDATIONS §2.2]; podpisana `you` wkładałaby zdanie lidera w Twoje usta. */
  suggested: 'agent',
  asked: 'agent',
  handoff: 'loadout',
  problem: 'loadout',
  done: 'loadout',
  read: 'loadout',
  search: 'loadout',
  edit: 'loadout',
  ran: 'loadout',
  memory: 'loadout',
  thinking: 'loadout',
  /* Loadout, nie agent: stan kroku ogłasza planista, a nie ten, kto ten krok wykonuje. */
  stepState: 'loadout',
};

/** Kto powiedział to, co niesie wiersz tego rodzaju. */
export function authorityOf(kind: Kind): Who {
  return AUTHOR[kind];
}

/**
 * Jedna rzecz, którą agent nadał: rodzaj i zdanie, jakie ta rzecz niesie.
 *
 * Celowo NIE jest to ani `FeedLine`, ani `HistoryRow`. Zdanie kafelka powstaje w dwóch
 * miejscach — lista kafelków ma pod ręką wiersze historii (`roster.ts`), a scena testowa
 * linie z drutu — i gdyby polityka „kto mówi" umiała czytać tylko jeden z tych dwóch
 * kształtów, drugie miejsce musiałoby ją przepisać u siebie. Tak właśnie po cichu umarło
 * skanowanie sekretów w meetnotes: polityka przepisana w adapterze (niezmiennik 23).
 *
 * `text` jest opcjonalne, bo dokładnie jeden rodzaj go nie ma: `thinking` nie niesie zdania
 * [T2 §7.2 wiersz 4].
 */
export interface Utterance {
  readonly kind: Kind;
  readonly text?: string;
  /** Tylko na linii `done`: jak agent skończył. Kafelek czyta stąd swój stan, nigdy ze zdania. */
  readonly ended?: 'well' | 'badly' | 'stopped';
}

/**
 * Co robi agent, który o coś zapytał I DALEJ CZEKA — zdanie Loadouta, nie treść pytania.
 *
 * Pytanie ma wtedy JEDNO żywe miejsce: kartę z przyciskami przy kroku. Powtórzone na kafelku
 * daje dwa regiony na jeden fakt, przy limicie 1 (niezmiennik 13), a kafelek i tak nie ma
 * miejsca na trzy warianty odpowiedzi. To samo zdanie stoi w strefie TERAZ [feed/model.ts,
 * `WAITING_ON_YOU`] i z tego samego powodu.
 *
 * 2026-08-31 — WARUNKOWE, I TO JEST NAPRAWA ZMIERZONEJ WADY. Wiersz `asked` zostaje w historii
 * NA ZAWSZE — „że agent zapytał" naprawdę się wydarzyło — więc bezwarunkowe odwzorowanie
 * zostawiało to zdanie na kafelku długo po tym, jak bieg zszedł ze Stopem, z odmową albo po
 * prostu się skończył. Kolejkę pytań gasi wtedy model (`../feed/model.ts`, `runEnded`), więc
 * karta z przyciskami znika, a zdanie o czekaniu zostaje samo: człowiek czyta, że coś na niego
 * czeka, i szuka czego nacisnąć.
 */
const WAITING_ON_YOU = 'Waiting for your answer';

/**
 * Co robi agent, po którym nie ma jeszcze ani jednego zdania.
 *
 * Jedyny stan, w którym linie są, a zdania nie ma: same `thinking`. Pusty kafelek czyta się
 * jak zepsuty agent, a wymyślone zdanie jest gorsze od pustego, więc zostaje to jedno słowo,
 * którym Loadout nazywa ten stan wszędzie indziej [T2 §7.3 reguła 5].
 */
const THINKING = 'Thinking…';

/**
 * Jedna linia, zawsze.
 *
 * Ciągi białych znaków — łącznie ze znakiem nowej linii w środku zdania — schodzą do jednej
 * spacji, wynik jest przycięty z obu stron. Kafelek ma sufit czterech linii [ARCHITECTURE §7]
 * i notatka złamana przez agenta w połowie zdania przewraca go, nie dokładając ani jednego
 * pola, na które patrzy kryterium.
 *
 * Czego tu NIE MA: obcięcia do stałej liczby znaków. Skracaniem zajmuje się arkusz stylów
 * (`text-overflow: ellipsis`, makieta linia 185), bo wtedy pełne zdanie wraca, kiedy okno
 * się poszerzy. Obcięte w kodzie nie wraca nigdy, a kafelek jest jedynym miejscem, w którym
 * to zdanie widać.
 */
function oneLine(text: string): string {
  return text.replace(/\s+/g, ' ').trim();
}

/**
 * Zdanie kafelka dla agenta, który nadał te rzeczy — i podpis pod nim.
 *
 * Wygrywa NAJNOWSZE zdanie, nie najnowsza notatka. Agent pisze prozą, potem padają
 * sprawdzenia — i to sprawdzenia są tym, co stało się ostatnie, więc kafelek mówi o nich,
 * podpisany Loadoutem. Notatka dalej należy do agenta; przestała tylko być najświeższa.
 *
 * `waitsOnYou` PYTA O TERAŹNIEJSZOŚĆ, a `said` o przeszłość — i dokładnie dlatego są dwoma
 * argumentami. Historia mówi, że agent zapytał, i mówić tak będzie zawsze; czy KTOKOLWIEK
 * jeszcze na tę odpowiedź czeka, wie tylko model strumienia (`../feed/model.ts`, `attention`),
 * a wołający czyta stamtąd tę jedną odpowiedź (`./card.ts`). Druga tabela wyliczająca to
 * z samych linii byłaby drugim domem jednego faktu (niezmiennik 13) i myliłaby się w obie
 * strony: pytanie odpowiedziane wygląda w historii identycznie jak porzucone.
 */
export function sayFor(said: readonly Utterance[], waitsOnYou: boolean): Say {
  /* Przód, nie tył: pętla od końca wymaga indeksowania, a `noUncheckedIndexedAccess` robi
   * z każdego takiego odczytu gałąź „a jeśli nie ma", której nie da się wykonać. Rodzaj
   * i zdanie osobno, żeby nie trzeba było tej samej gałęzi dopisywać po pętli. */
  let kind: Kind | null = null;
  let text = '';
  for (const utterance of said) {
    if (utterance.text === undefined) continue;
    kind = utterance.kind;
    text = utterance.text;
  }

  if (kind === null) return { text: THINKING, who: 'loadout' };
  /* Podpis idzie za ZDANIEM, nie za linią: skoro kafelek nie cytuje pytania, tylko mówi,
   * na co ono czeka, to zdanie jest Loadouta i tak ma być podpisane.
   *
   * KIEDY NIKT JUŻ NIE CZEKA, WYJĄTKU NIE MA — i to nie jest trzeci wariant tekstu, tylko jego
   * brak. Karty z przyciskami wtedy na ekranie nie ma, więc nie ma czego powtarzać, a ostatnią
   * rzeczą, jaką ten agent naprawdę powiedział, jest właśnie to pytanie. Zdanie dopisane na tę
   * chwilę byłoby trzecim brzmieniem jednego faktu, a kafelek i tak nie umie powiedzieć, czy
   * pytanie dostało odpowiedź, czy zeszło razem z biegiem. */
  if (kind === 'asked' && waitsOnYou) return { text: WAITING_ON_YOU, who: 'loadout' };
  return { text: oneLine(text), who: authorityOf(kind) };
}
