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
import { defaultLead, subscribeToDefaultLead } from '../../state/settings';

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
let chosen = '';
const listeners = new Set<() => void>();

/**
 * Identyfikator wskazanego agenta, albo `''`, dopóki nikt nie wybierał ani tu, ani w Settings.
 *
 * DWA ŹRÓDŁA, JEDEN FAKT I USTALONE PIERWSZEŃSTWO: wskazanie z paska bije domyślne, bo jest
 * młodsze i dotyczy tego jednego okna. Odwrotna kolejność znaczyłaby, że wybór z paska nic nie
 * robi u kogoś, kto raz coś ustawił w Settings — czyli kontrolka, która kłamie (niezmiennik 16).
 */
export function lead(): string {
  return chosen === '' ? defaultLead() : chosen;
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
  if (id === chosen) return;
  chosen = id;
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
