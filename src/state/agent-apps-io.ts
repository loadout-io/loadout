/* Jedyna krawędź ulotnej migawki lokalnych aplikacji agentów.
 *
 * DLACZEGO OSOBNY PLIK, A NIE `invoke()` w magazynie — ten sam kształt i ten sam powód, co
 * w `./settings-io.ts` i `./workspaces-io.ts`: nazwa komendy stoi w jednym miejscu, a magazyn
 * obok zajmuje się wyłącznie tym, co z odpowiedzią zrobić.
 *
 * ZERO POLITYKI TUTAJ. Ani jednego `try`, ani jednej wartości domyślnej, ani jednego zdania dla
 * człowieka: odmowa jedzie odrzuconą obietnicą do magazynu, bo to on wie, czego właśnie próbował.
 */
import { invoke } from '@tauri-apps/api/core';

/**
 * Pyta obie lokalne aplikacje agentów naraz i oddaje to, co powiedziały.
 *
 * `unknown`, nie typ wiersza, i to nie jest lenistwo: kształt sprawdza magazyn, slot po slocie
 * (`./agent-apps.ts`, `fromWire`), więc rzut TUTAJ byłby obietnicą, której nikt nie egzekwuje.
 */
export function checkAgentApps(): Promise<unknown> {
  return invoke('check_agent_apps');
}
