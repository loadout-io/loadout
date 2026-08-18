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

import type { Authored, Import, InstalledSkill } from '../../state/skills';

/**
 * Co naprawdę leży w katalogach agentów.
 *
 * 2026-08-18 — do tego dnia ta droga nie istniała, a skutek był zmierzony: `install` pisało na
 * dysk, okno nigdy tego nie odczytywało, więc licznik „N saved" pokazywał wyłącznie to, co
 * dodano w TEJ sesji, a zainstalowana umiejętność znikała po restarcie. Niezmiennik 4 mówi
 * odwrotnie: pliki są prawdą, a ekran ma pokazywać je, a nie swoją pamięć.
 *
 * Bez argumentów: katalogi vendorów wylicza Rust (`skills::place::destinations`) i to jest
 * jedyne miejsce, w którym stoi ścieżka `.claude/skills`.
 */
export function listSkills(): Promise<InstalledSkill[]> {
  return invoke<InstalledSkill[]>('list_skills');
}

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
 * Trzy odpowiedzi z formularza → przejrzana umiejętność, tą samą drogą co adres.
 *
 * 2026-08-19 — druga droga wejścia, obiecana na ekranie od pierwszego dnia sekcji („Paste
 * a link, or write one yourself") i nieistniejąca do dziś: `review_skill(url)` był jedyną
 * komendą, która cokolwiek wciągała.
 *
 * Jedzie to, co człowiek WPISAŁ, i nic ponadto — ani slug, ani złożony `SKILL.md`. Plik składa,
 * zapisuje i skanuje Rust, w jednym miejscu i w jednej kolejności; tekst zbudowany tutaj byłby
 * tekstem, którego nikt nie przeskanował, a nazwa policzona tutaj byłaby drugą odpowiedzią na
 * pytanie „jak nazywa się katalog" (niezmienniki 23 i 13).
 */
export function authorSkill(authored: Authored): Promise<Import> {
  return invoke<Import>('author_skill', { authored });
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

/**
 * Zabierz umiejętność z katalogów agentów.
 *
 * 2026-08-18 — do tego dnia tej drogi NIE BYŁO WCALE: ani komendy, ani kontrolki. Skutek jest
 * cięższy niż zwykły brak funkcji, bo ta sekcja pisze do ŻYWEJ konfiguracji narzędzi człowieka
 * (`src-tauri/src/skills/mod.rs`, `DESTINATION_DIRS`): jedno błędne kliknięcie „Add" wchodziło
 * do każdego następnego uruchomienia Claude Code i Codeksa, bez ostrzeżenia i bez drogi
 * powrotu. Dodawanie bez zabierania nie jest połową mechanizmu — jest pułapką.
 *
 * Argumentem jest NAZWA, nie ścieżka. Katalogi vendorów wylicza Rust
 * (`skills::place::destinations`) i to jest jedyne miejsce w repo, w którym stoi
 * `.claude/skills`. Ścieżka podana z okna byłaby drugą odpowiedzią na pytanie „gdzie to leży"
 * (niezmiennik 13) — i jedyną, którą da się skierować gdziekolwiek.
 */
export function remove(name: string): Promise<void> {
  return invoke<void>('delete_skill', { name });
}
