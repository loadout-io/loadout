/* Kafelki rzeczy, które Loadout uruchomił dla człowieka — obok kafelków agentów, nigdy w nich.
 *
 * ZGŁOSZENIE, Z KTÓREGO TO POWSTAŁO (właściciel, 2026-08-20): „jak napiszę aby coś odpalił jakąś
 * apkę to chcę mieć też po prawej gdzie są agenci info o procesach odpalonych itp, i po kliku
 * mogę tam wejść".
 *
 * CICHA PORAŻKA, PRZED KTÓRĄ STOI TEN PLIK: kafelek, który zostaje po rzeczy, która zeszła.
 * „Running" nad komendą zeszłą dwie minuty temu jest tym samym kłamstwem, co widmowy agent
 * z T-66 i wiersz okna udający agenta z T-67 — a ta fala pokazała, że ta klasa wady wraca
 * powierzchnia po powierzchni, bo za każdym razem wchodzi przez inną powierzchnię. Kafelek
 * istnieje dokładnie tak długo, jak rzecz za nim (niezmiennik 17), i rozstrzyga to TA funkcja,
 * a nie arkusz stylów i nie komponent.
 *
 * DWIE GRUPY, NIE DWA KOLORY. Rzecz uruchomioną komendą odróżnia od agenta MIEJSCE na liście,
 * a nie odcień kwadratu: kolor jest tożsamością, nigdy stanem [DESIGN §3 „Tożsamość ≠ stan"] —
 * to ta sama reguła, przez którą w referencyjnym poprzedni prototyp agent Forge miał dokładnie ten hex,
 * który obok znaczył „czeka na twoją decyzję". Kwadrat idzie więc z tej samej przygaszonej palety
 * (`./colour.ts`), a różnicę niesie struktura odpowiedzi.
 *
 * CZEGO TEN PLIK NIE ROBI: nie pyta systemu o nic. Dostaje to, co wie rejestr po stronie Rusta
 * (`src-tauri/src/commands/processes.rs`), i zamienia to na kafelki. Funkcja, która sama czytałaby
 * stan świata, nie dałaby się osądzić bez maszyny — a to jest jedyna rzecz w tej ścieżce, którą
 * da się osądzić czystym wejściem i czystym wyjściem.
 */
import type { RailCard } from './card';
import { railCard } from './card';

/**
 * Jedna rzecz uruchomiona komendą, tak jak widzi ją okno.
 *
 * Trzy pola, bo trzy fakty: co to jest, jak to zaadresować i czy jeszcze biegnie. Kształt jest
 * ŚWIADOMIE własny, a nie przepisany z `StartedProcess` po stronie Rusta: tam kluczem jest `pgid`,
 * czyli liczba, którą okno poznaje dopiero z odpowiedzi, a kafelek ma stanąć w chwili, w której
 * człowiek naciśnie Enter. Zlanie tych dwóch kształtów w jeden kazałoby oknu czekać z rysowaniem
 * na coś, co przyjdzie później.
 */
export interface StartedProcess {
  /** Klucz kafelka. Nigdy napis na ekranie — dokładnie jak `RailCard.id`. */
  readonly id: string;
  /**
   * Wiersz powłoki, co do znaku. To ON jest nazwą kafelka.
   *
   * Wymyślona etykieta („Dev server") byłaby relacją, której w danych nie ma (niezmiennik 17),
   * a człowiek szuka na liście tego, co sam wpisał.
   */
  readonly command: string;
  /** Czy to jeszcze biegnie. `false` znaczy „nie ma kafelka", nie „kafelek na szaro". */
  readonly alive: boolean;
}

/** Co lista dostaje: gotowe kafelki agentów i to, co wie okno o rzeczach uruchomionych. */
export interface GroupsInput {
  /** Kafelki agentów, już policzone przez `roster()`. Ten plik ich nie przelicza. */
  readonly agents: readonly RailCard[];
  /** Wszystko, o czym okno wie — także to, co już zeszło. Odsiew jest odpowiedzią tej funkcji. */
  readonly started: readonly StartedProcess[];
}

/** Dwie grupy jednej kolumny. Pusta lista znaczy „nie ma czego pokazać", nigdy „nie wiem". */
export interface RailGroups {
  readonly agents: readonly RailCard[];
  readonly started: readonly RailCard[];
}

/**
 * Kafelki obu grup — agenci tam, gdzie byli, a rzeczy uruchomione komendą obok nich.
 *
 * Rzecz, która zeszła, nie dostaje kafelka wcale. To jest cała treść tej funkcji i cały powód,
 * dla którego ona istnieje osobno od komponentu.
 */
export function railGroups(input: GroupsInput): RailGroups {
  return {
    /* KAFELKI AGENTÓW JADĄ DALEJ CO DO WARTOŚCI, nie przez `map`. Przeliczenie ich tutaj
     * postawiłoby odpowiedź na pytanie „co ten agent ostatnio powiedział" w dwóch miejscach,
     * a jedno z dwóch jest zawsze tym nieaktualnym (niezmiennik 13). Ten plik wolno DOŁOŻYĆ
     * grupę obok; przepisać tamtej nie wolno. */
    agents: input.agents,
    started: input.started.filter((one) => one.alive).map(tileFor),
  };
}

/**
 * Kafelek jednej rzeczy, która jeszcze biegnie.
 *
 * TĄ SAMĄ FUNKCJĄ, którą kafelek dostaje agent (`railCard`), i to jest wymóg, nie oszczędność:
 * kwadrat tożsamości przydziela `colour.ts` i ma go przydzielać RAZ dla całej listy. Ręczny
 * literał obok byłby drugim miejscem, w którym powstaje kafelek — a wtedy rzecz uruchomiona
 * komendą mogłaby dostać odcień z palety STANU, czyli ten sam błąd, przez który cała reguła
 * „tożsamość ≠ stan" powstała [DESIGN §3].
 */
function tileFor(one: StartedProcess): RailCard {
  return railCard({
    id: one.id,
    /* WIERSZ POWŁOKI JEST NAZWĄ, co do znaku. Etykieta wymyślona z komendy („Dev server")
     * byłaby relacją, której w danych nie ma (niezmiennik 17), a człowiek szuka na liście tego,
     * co sam wpisał. */
    name: one.command,
    /* PUSTA ROLA, bo tego faktu nie ma. „Po co ten agent jest" jest zdaniem z definicji agenta,
     * a rzecz uruchomiona komendą żadnej definicji nie ma — pusty slot kafelek po prostu
     * pomija (`rail.tsx`, `CardLine`), a zdanie zmyślone zajęłoby jego miejsce i czytałoby się
     * jak fakt. */
    role: '',
    /* ZIELONE „working", bo to znaczy „dzieje się TERAZ" [DESIGN §3] — a kafelek dostaje
     * wyłącznie rzecz, która biegnie. Rzecz, która zeszła, nie ma kafelka wcale, więc żaden
     * inny stan nie ma tu jak wystąpić. */
    status: 'working',
    /* JEDNA WYPOWIEDŹ Z PUSTYM ZDANIEM, nie pusta lista, i to jest wybór o nazwanym powodzie:
     * `sayFor([])` oddaje „Thinking…", czyli zdanie o kimś, kto MYŚLI. Nad wierszem powłoki jest
     * to relacja, której w danych nie ma (niezmiennik 17) — komenda nie myśli, komenda biegnie.
     * Puste zdanie kafelek pomija tak samo jak pustą rolę, więc zostają dwie linie, które są
     * prawdziwe: co to jest i że to się dzieje.
     *
     * Dzień, w którym ta linia zacznie nieść ostatni wiersz wyjścia, jest dniem, w którym
     * `StartedProcess` dostanie czwarte pole — a dziś nie ma, bo wyjście jedzie na drut raz
     * i tylko dla tej rzeczy, w którą człowiek wszedł (`commands::processes::Processes::said`). */
    lines: [{ kind: 'run', text: '' }],
  });
}
