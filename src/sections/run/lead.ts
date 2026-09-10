/* „Kim jest lider" po stronie okna — jeden fakt, jeden dom (niezmiennik 13).
 *
 * SZKIELET T-60. Ciała rzucają, i to jest wymóg fazy kontraktu, nie niedbalstwo: `vitest`
 * przewraca się już na ZBIERANIU brakującego importu („Cannot find module"), a to jest podpis
 * z `NOT_A_REAL_RED` — kryterium, które go dostanie, nie uruchomiło ani jednej asercji. Moduł
 * musi więc istnieć, importy muszą się rozwiązać, a padnięcie ma nastąpić na zachowaniu.
 *
 * CO TU MIESZKA, A CO NIE. Mieszka tu WYBÓR — identyfikator zapisanego agenta — i słowo, którym
 * pasek nazywa kontrolkę. Nie mieszka tu ani vendor, ani model, ani dial bezpieczeństwa: „kim
 * jest lider" ma dokładnie jedno źródło, zapisaną definicję agenta, a kopia któregokolwiek z tych
 * pól trzymana obok w stanie okna jest pierwszą rzeczą, która się rozjedzie (niezmiennik 13).
 * Okno trzyma wskazanie; kto to jest, odpowiada Rust, czytając plik.
 *
 * DLACZEGO MODUŁ, A NIE `useState` W KONTROLCE STARTU. Wybór człowieka przeżywa odmontowanie
 * ekranu: powłoka montuje dokładnie jedną sekcję (`src/App.tsx`), więc wyjście do Agentów
 * i powrót niszczyłoby stan kontrolki. Ten sam ruch i ten sam zmierzony powód, co przy
 * `./limits/chosen.ts` — i ten sam kształt, którego chce `useSyncExternalStore`.
 *
 * Stał tu jeszcze `./chosen-workflow.ts` jako drugi przykład i przestał, bo tamten modułu już
 * nie ma: był wyniesiony na poziom modułu dla zachowania, które właściciel skasował 2026-08-19
 * („nie powinno być tak, że jak piszę bez komendy... to się na nowo całe workflow odpala"),
 * a jego jedynym konsumentem była lista wyboru, której miejsce zajęła ta kontrolka.
 *
 * 2026-08-29 — WYBÓR Z SETTINGS JEST TU POKAZYWANY, A NIE KOPIOWANY (niezmiennik 13). Do tego
 * dnia to wskazanie zaczynało się puste przy KAŻDYM uruchomieniu i człowiek wybierał tę samą
 * osobę przed każdą pracą. Domyślny lider mieszka teraz w jednym miejscu — `src/state/settings.ts`,
 * a trwale w pliku (`~/.loadout/settings.json`, niezmiennik 4) — a ten moduł trzyma wyłącznie
 * NADPISANIE na to jedno okno. Druga kopia domyślnego wyboru trzymana tutaj rozjechałaby się
 * z plikiem przy pierwszym zapisie z Settings i nikt by tego nie zobaczył.
 */
import { activeWorkspace } from '../../state/workspaces';
import { defaultLead, subscribeToDefaultLead } from '../../state/settings';
import { whatTheLeadCanDo as askRustWhatItCanDo } from './io';
import type { WhatTheLeadCanDo } from './io';

/**
 * Etykieta dostępnościowa kontrolki lidera w pasku loadoutu.
 *
 * Stała, a nie napis wpisany w komponencie, z jednego powodu: kryterium ma ją CZYTAĆ, nie
 * przepisywać. Wpisana z palca po obu stronach byłaby zielona także wtedy, gdyby kontrolka
 * i test mówiły o dwóch różnych rzeczach — a wtedy „na pasku stoi lider" jest zdaniem o teście.
 *
 * Słowo jest z tabeli DESIGN §8: `orchestrator` jest na liście żargonu, a `lead agent` jest jego
 * zamiennikiem (niezmiennik 14). Wybór bez nazwy jest zagadką, więc kontrolka musi się nazywać.
 */
export const LEAD_LABEL = 'Lead agent';

/** Nadpisanie na TO okno: co człowiek wskazał w pasku, zamiast tego, co stoi w Settings. */
const chosen = new Map<string, string>();
function projectKey(): string {
  return activeWorkspace()?.folder ?? '';
}
const listeners = new Set<() => void>();

/**
 * Identyfikator wskazanego agenta, albo `''`, dopóki nikt nie wybierał ani tu, ani w Settings.
 *
 * DWA ŹRÓDŁA, JEDEN FAKT I USTALONE PIERWSZEŃSTWO: wskazanie z paska bije domyślne, bo jest
 * młodsze i dotyczy tego jednego okna. Odwrotna kolejność znaczyłaby, że wybór z paska nic nie
 * robi u kogoś, kto raz coś ustawił w Settings — czyli kontrolka, która kłamie (niezmiennik 16).
 */
export function lead(): string {
  return chosen.get(projectKey()) || defaultLead();
}

/**
 * Zapisuje wskazanie. Identyfikatorem, nie nazwą: nazwa agenta się zmienia, `id` przeżywa
 * zmianę nazwy (T4 §5.1) i to nim posługuje się Rust, szukając definicji w bibliotece.
 *
 * 2026-08-20 — TO WSKAZANIE NIE MA JESZCZE DRUTU DO RUSTA I JEST TO ZGŁOSZENIE, NIE PRZEOCZENIE.
 * `say_to_orchestrator` musiałoby dostać klucz `lead` obok `folder`, a `src/sections/run/io.ts`
 * należy do niewyładowanego T-41 i mandat T-60 na tamten plik pozwala dopisać WYŁĄCZNIE klucz
 * `folder` przy `open_chat`. Nowej komendy nie da się dodać obok: `ipc_commands_registered.rs`
 * porównuje listę handlera z `src-tauri/commands.golden.txt` co do sztuki. Dopóki człowiek tego
 * nie rozstrzygnie, wybór żyje w oknie i czeka na odbiorcę — a Rust dalej rozmawia zaszytym
 * Claude'em (`ipc::AppState::chat_driver`). Cała reszta drogi jest gotowa:
 * `commands::chat::Lead::pointed_at` bierze dokładnie ten napis.
 */
export function setLead(id: string): void {
  if (id === chosen.get(projectKey())) return;
  chosen.set(projectKey(), id);
  for (const listener of listeners) listener();
}

/**
 * Prenumerata w kształcie, którego chce `useSyncExternalStore`.
 *
 * Słucha OBU magazynów, bo [`lead`] składa odpowiedź z obu. Prenumerata pilnująca wyłącznie
 * nadpisania z paska pokazywałaby stary wybór po zapisie w Settings aż do następnego renderu
 * z innego powodu — czyli kontrolkę, która czasem się odświeża, a czasem nie.
 */
export function subscribeToLead(listener: () => void): () => void {
  listeners.add(listener);
  const stopWatchingTheDefault = subscribeToDefaultLead(listener);
  return () => {
    listeners.delete(listener);
    stopWatchingTheDefault();
  };
}

/* ── CO TEN LIDER MOŻE ─────────────────────────────────────────────────────────────────────
 *
 * 2026-09 (Z-50) — ODPOWIEDŹ RUSTA, NIE DRUGA DEFINICJA. Okno nie liczy tu niczego z dialu ani
 * z listy narzędzi: obie te wartości stają się `--tools` po tamtej stronie granicy, więc kopia
 * reguły trzymana tutaj rozjechałaby się w dniu, w którym zmieni się sufit polityki — a rozjazd
 * wyglądałby jak zdanie, które po prostu jest nieaktualne, i nikt by go nie zauważył
 * (niezmiennik 13).
 *
 * MODUŁ, A NIE `useState` W EKRANIE, z tego samego powodu, co wskazanie wyżej: kształt, którego
 * chce `useSyncExternalStore`, i stan, który przeżywa odmontowanie sekcji. */

/** Ostatnia odpowiedź Rusta. `null` znaczy „jeszcze nie przeczytano", nie „nic nie może". */
let powers: WhatTheLeadCanDo | null = null;
let powersFolder = '';
const watchingThePowers = new Set<() => void>();

/**
 * Co lider może, o ile ktoś już o to zapytał.
 *
 * `null` jest stanem, nie brakiem: zanim odpowiedź przyjdzie, wiersz wejścia mówi to samo, co
 * mówił zawsze. Zdanie zgadnięte na czas odczytu byłoby zdaniem, które zmienia się pod ręką.
 */
export function whatTheLeadCanDo(): WhatTheLeadCanDo | null {
  return powersFolder === projectKey() ? powers : null;
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToLeadPowers(listener: () => void): () => void {
  watchingThePowers.add(listener);
  return () => {
    watchingThePowers.delete(listener);
  };
}

/**
 * Zapamiętuje odpowiedź granicy.
 *
 * Osobno od [`readWhatTheLeadCanDo`], bo to jest jedyne miejsce, w którym ta wartość się zmienia
 * — a kryterium, które chce osądzić ZDANIE, musi mieć jak postawić odpowiedź bez żywego Tauri.
 */
export function rememberWhatTheLeadCanDo(can: WhatTheLeadCanDo | null): void {
  powers = can;
  powersFolder = projectKey();
  for (const listener of watchingThePowers) listener();
}

/**
 * Pyta Rusta o moce lidera wskazanego w tej chwili i zapamiętuje odpowiedź.
 *
 * ODMOWA ZOSTAWIA `null`, a nie zdanie na ekranie: „nie wskazałeś lidera" mówi już wiersz
 * wejścia w chwili Entera i mówi to głośniej, bo dotyczy tekstu, który człowiek właśnie napisał.
 * Druga kopia tej odmowy pod polem byłaby czerwienią przy każdym wejściu w sekcję — także
 * u kogoś, kto jeszcze niczego nie wybrał, czyli w chwili, w której nic złego się nie stało.
 */
export async function readWhatTheLeadCanDo(): Promise<void> {
  /* O KOGO PYTAMY, ZAPAMIĘTANE PRZED ODCZYTEM. Dwa szybkie przełączenia w pasku to dwa odczyty,
   * a odpowiedzi wracają w dowolnej kolejności: bez tego porównania wolniejsza odpowiedź o
   * POPRZEDNIM liderze nadpisałaby świeższą i zdanie pod polem opisywałoby kogoś, kogo już nie
   * ma na pasku — czyli dokładnie tę nieprawdę, którą to zadanie zdejmuje. */
  const asked = lead();
  const folder = projectKey();
  try {
    const can = await askRustWhatItCanDo(asked, folder || null);
    if (asked !== lead() || folder !== projectKey()) return;
    /* `null` z granicy znaczy „nie ma odpowiedzi" i nie ma prawa udawać zera mocy: taka jest
     * atrapa w testach przeglądarkowych (`e2e/harness.ts`), a zdanie o liderze, który nic nie
     * może, jest tam równie nieprawdziwe jak w produkcie. */
    rememberWhatTheLeadCanDo((can as WhatTheLeadCanDo | null) ?? null);
  } catch {
    if (asked === lead() && folder === projectKey()) rememberWhatTheLeadCanDo(null);
  }
}
