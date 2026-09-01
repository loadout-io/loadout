/* Co znaczy „dodaj kolumnę" — polityka, nie ciało `onClick`.
 *
 * To repo nie ma jsdom, więc kliknięcia nie da się odpalić w teście. Reguła zamknięta
 * w komponencie byłaby regułą, której żadne kryterium nie umie dotknąć; ten sam powód i ten
 * sam kształt stoi przy `./evaluate` i przy `run/launch.ts`.
 */
import type { EvalVariant } from './io';

/**
 * Nowa kolumna, odbita od tej, która już stoi.
 *
 * # ODBITA, nie pusta, i to jest cała treść tej funkcji
 *
 * Kolumna to agent plus patch nad jego definicją. Pusta zaczynałaby od pytania „który agent",
 * na które człowiek odpowiedział już przy zakładaniu zestawu — a druga odpowiedź na to samo
 * pytanie jest tą, która kiedyś rozjedzie się z pierwszą. Kopia różni się **jedną rzeczą**,
 * i o to w tabeli chodzi: dwie kolumny różniące się dwiema zmianami nie mówią, która z nich
 * odpowiada za różnicę.
 *
 * Identyfikator jest wyliczany z zajętych, nie z licznika w stanie: licznik po skasowaniu
 * kolumny wraca do wartości, która już raz była, a wtedy nowa kolumna przejmuje wyniki starej.
 */
export function nextColumn(
  existing: readonly EvalVariant[],
  fallbackAgent: string,
): EvalVariant | null {
  const from = existing[existing.length - 1];
  const agent = from?.agent ?? fallbackAgent;
  if (agent === '') return null;
  const taken = new Set(existing.map((one) => one.id));
  const names = new Set(existing.map((one) => one.name.trim().toLowerCase()));
  let at = existing.length + 1;
  while (taken.has('column-' + String(at)) || names.has('column ' + String(at))) at += 1;
  return {
    id: 'column-' + String(at),
    name: 'Column ' + String(at),
    agent,
    /* Patch jedzie PUSTY, czyli „ten agent, jaki jest". Skopiowanie patcha sąsiada dałoby dwie
     * identyczne kolumny, a dwie identyczne kolumny to dwa razy ten sam rachunek za tę samą
     * odpowiedź. Co ma się różnić, mówi człowiek w polu obok. */
    overrides: {},
  };
}

/**
 * Ta sama kolumna z innym modelem — jedyna zmiana, którą da się zrobić z tabeli.
 *
 * Model, a nie dowolne pole definicji: reszta (dial, narzędzia, limit czasu) mieszka
 * w formularzu agenta i ma tam zostać. Drugi formularz agenta wewnątrz tabeli byłby drugim
 * miejscem, w którym mieszka odpowiedź „czym ten agent jest" (niezmiennik 13) — a tabela ma
 * odpowiadać na pytanie „która wersja jest lepsza", nie „z czego składa się agent".
 *
 * Pusty napis **zdejmuje** nadpisanie, zamiast wpisywać pustkę: model ustawiony na `""`
 * u vendora znaczy odmowę startu, a nie „domyślny".
 */
export function withModel(variant: EvalVariant, model: string): EvalVariant {
  const overrides = { ...variant.overrides };
  if (model.trim() === '') delete overrides.model;
  else overrides.model = model.trim();
  return { ...variant, overrides };
}

/** Nazwa, którą człowiek wpisał. Pusta zostawia poprzednią — kolumna bez podpisu nie istnieje. */
export function withName(variant: EvalVariant, name: string): EvalVariant {
  return name.trim() === '' ? variant : { ...variant, name: name.trim() };
}

/** Model tej kolumny, do pokazania w polu. Pusty znaczy „ten, którego ma agent". */
export function modelOf(variant: EvalVariant): string {
  const value = variant.overrides.model;
  return typeof value === 'string' ? value : '';
}
