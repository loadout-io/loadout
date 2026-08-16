/* Jedyne miejsce w sekcji Umiejętności, które zna nazwy komend po stronie Rusta
 * (niezmiennik 23: polityka w jednym rdzeniu, adapter po pięć linii).
 *
 * DLACZEGO OSOBNY PLIK, A NIE `invoke()` rozsiany po magazynie i po karcie. Kryterium stanu
 * mierzy LICZBĘ wywołań: „zero razy, dopóki blokujące znalezisko nie zostało przeczytane".
 * Zdanie o liczbie wywołań ma sens tylko wtedy, kiedy jest jedna krawędź, przez którą da się
 * wywołać cokolwiek — dwie drogi do Rusta znaczą, że licznik pilnuje jednej z nich, a instalacja
 * jedzie drugą i nikt tego nie zauważy.
 *
 * 2026-08-16 — ciała wypełnia T-27, dwiema nazwami z `src-tauri/commands.golden.txt`. Adapter
 * i nic poza adapterem: cała polityka adresu, limity, skan i zapis mieszkają po stronie Rusta
 * (`skills::ingest`, `skills::place`), więc tu nie ma czego przepisać (niezmiennik 23).
 */
import { invoke } from '@tauri-apps/api/core';

import type { Import } from '../../state/skills';

/**
 * Adres → pobrana i przejrzana umiejętność.
 *
 * Cała droga bajtów (polityka adresu, limity, normalizacja, skan) mieszka po stronie Rusta
 * w `skills::ingest`. Frontend dostaje wynik, nigdy surowe bajty: treść, którą agent wykona,
 * nie ma po co przechodzić przez warstwę, która ją renderuje.
 */
export function readLink(url: string): Promise<Import> {
  return invoke<Import>('review_skill', { url });
}

/**
 * Zapisz przejrzaną umiejętność w katalogach vendorów.
 *
 * Jedzie CAŁY przegląd, nie samo ciało: na dysk ma trafić dokładnie ten tekst, który został
 * przeskanowany, a nie tekst złożony jeszcze raz po drodze.
 */
export function install(item: Import): Promise<void> {
  return invoke<void>('install_skill', { item });
}
