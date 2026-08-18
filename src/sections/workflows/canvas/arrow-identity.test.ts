/* Tożsamością strzałki jest para `from->to` — po obu stronach mappera.
 *
 * INCYDENT, PO KTÓRYM TO POWSTAŁO. 2026-08-19, plik właściciela `~/.loadout/workflows/
 * new-workflow.json`: `links` niosło `s_2->s_3` dwa razy i `s_4->s_5` trzy razy. W konsoli
 * żywego okna leciało kilkanaście razy „Encountered two children with the same key, `s_2->s_3`",
 * bo `toCanvas` nadaje krawędzi identyfikator z tej właśnie pary — więc powtórzona pozycja daje
 * dwie krawędzie o JEDNYM identyfikatorze. React ostrzega przy tym wprost, że dzieci mogą
 * zostać zdublowane albo pominięte, czyli że strzałka może się nie narysować. W edytorze grafu
 * to nie jest usterka kosmetyczna: strzałka JEST znaczeniem dokumentu.
 *
 * Komentarz nad `toCanvas` obiecywał to zwężenie od początku („dwa razy narysowana ta sama
 * strzałka to jedna krawędź, a nie dwie różne o tym samym znaczeniu"), a kod go nie robił.
 * Ten plik jest po to, żeby proza i kod nie mogły się już rozejść po cichu.
 *
 * SŁABĄ WERSJĄ jest sprawdzenie samej długości tablicy. Przechodzi dla implementacji, która
 * zostawia OSTATNIE wystąpienie zamiast pierwszego — czyli przestawia kolejność strzałek w pliku
 * przy każdym zapisie i robi z autosave'u generator różnic w gicie [T3 §8.2 reguła 2]. Dlatego
 * niżej stoi równość całej tablicy z konkretną kolejnością.
 *
 * Sprawdzane są OBIE strony, bo leczą dwie różne rzeczy: `toCanvas` ratuje plik, który już leży
 * na dysku (ten właściciela leży tam teraz), a `toFile` pilnuje, żeby płótno nigdy więcej
 * takiego pliku nie zapisało. Jedna strona wystarczyłaby do zieleni i zostawiła drugą połowę.
 */
import { describe, expect, it } from 'vitest';
import type { AgentStep, WorkflowFile } from '../../../state/workflows';
import { toCanvas, toFile } from './map';

function step(id: string): AgentStep {
  return {
    kind: 'agent',
    id,
    name: id,
    agent: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    overrides: {},
    copies: 1,
    instructions: 'Do the work.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y: 24 },
  };
}

/** Kształt z dysku właściciela, skrócony do tego, co mierzy to kryterium. */
function file(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_new_workflow',
    name: 'New workflow',
    steps: [step('s_2'), step('s_3'), step('s_4'), step('s_5')],
    links: [
      { from: 's_2', to: 's_3' },
      { from: 's_2', to: 's_3' },
      { from: 's_3', to: 's_4' },
      { from: 's_4', to: 's_5' },
      { from: 's_4', to: 's_5' },
      { from: 's_4', to: 's_5' },
    ],
  };
}

describe('an arrow is the pair from->to, once', () => {
  it('draws a repeated arrow as one edge, so the ids stay unique', () => {
    const { edges } = toCanvas(file());

    expect(
      new Set(edges.map((edge) => edge.id)).size,
      'React keys the edges by this id and warns that duplicates may be dropped or doubled — ' +
        'an arrow that sometimes fails to draw is the document lying about itself.',
    ).toBe(edges.length);
    expect(edges.map((edge) => edge.id)).toEqual(['s_2->s_3', 's_3->s_4', 's_4->s_5']);
  });

  it('keeps the first occurrence, so the file does not get reshuffled on every save', () => {
    const before = file();
    const { nodes, edges } = toCanvas(before);

    const after = toFile(before, nodes, edges);

    expect(
      after.links,
      'keeping the LAST occurrence would pass a length check and still rewrite the order of ' +
        'the arrows on every autosave, so every save shows up in git as a change nobody made.',
    ).toEqual([
      { from: 's_2', to: 's_3' },
      { from: 's_3', to: 's_4' },
      { from: 's_4', to: 's_5' },
    ]);
  });

  it('never writes the same arrow twice, and writes them in the order it got them', () => {
    const before = file();
    const { nodes } = toCanvas(before);
    /* Krawędzie podane WPROST, nie przez `toCanvas`: podróż tam i z powrotem przepuszcza je
     * przez to samo zwężenie DWA razy, więc implementacja zostawiająca ostatnie wystąpienie
     * i odwracająca kolejność wychodzi z niej nietknięta — dwa odwrócenia znoszą się nawzajem
     * i kryterium świeci na zielono nad wadą, którą miało łapać. `toFile` jest lejkiem KAŻDEGO
     * zapisu z płótna, więc sądzimy go osobno i wprost.
     *
     * TRZY RÓŻNE STRZAŁKI, nie jedna: przy jednej kolejność nie ma jak się pomylić. */
    const handed = [
      { id: 's_2->s_3', source: 's_2', target: 's_3' },
      { id: 's_4->s_5', source: 's_4', target: 's_5' },
      { id: 's_2->s_3', source: 's_2', target: 's_3' },
      { id: 's_3->s_4', source: 's_3', target: 's_4' },
      { id: 's_4->s_5', source: 's_4', target: 's_5' },
    ];

    const after = toFile(before, nodes, handed);

    expect(after.links).toEqual([
      { from: 's_2', to: 's_3' },
      { from: 's_4', to: 's_5' },
      { from: 's_3', to: 's_4' },
    ]);
  });
});
