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

import type { Authored, Import, InstalledSkill, Landing } from '../../state/skills';

/**
 * Co naprawdę leży w katalogach agentów, które czyta agent pracujący w TYM folderze.
 *
 * 2026-08-18 — do tego dnia ta droga nie istniała, a skutek był zmierzony: `install` pisało na
 * dysk, okno nigdy tego nie odczytywało, więc licznik „N saved" pokazywał wyłącznie to, co
 * dodano w TEJ sesji, a zainstalowana umiejętność znikała po restarcie. Niezmiennik 4 mówi
 * odwrotnie: pliki są prawdą, a ekran ma pokazywać je, a nie swoją pamięć.
 *
 * 2026-08-19 — JEDZIE FOLDER, NIE ZAKRES. Lista odpowiada na jedno pytanie: „co widzi agent
 * pracujący tutaj". Przy otwartym zakresie odpowiedź obejmuje oba korzenie, bo agent zagląda
 * w oba — więc wybór „ten projekt / wszędzie" nie ma tu czego rozstrzygać. Sam folder wystarcza,
 * a katalogi wylicza dalej Rust (`skills::place::destinations`) i to jest jedyne miejsce,
 * w którym stoi ścieżka `.claude/skills`.
 *
 * `null` znaczy „nie ma otwartego zakresu" i jest wartością, nie brakiem argumentu: klucz musi
 * dojechać, bo Tauri dopasowuje argumenty PO NAZWIE i deserializuje je, zanim wejdzie w ciało
 * komendy.
 *
 * # Dlaczego ten jeden argument ma domyślną wartość, i dlaczego jest nią `null`, a nie `?`
 *
 * Bo ma go dziś DWÓCH wołających i odpowiadają na dwa różne pytania. Sekcja Umiejętności pyta
 * „co widzi agent pracujący tutaj" i podaje folder JAWNIE (`src/state/skills.ts`). Panel kroku
 * w edytorze workflow (`src/sections/workflows/index.tsx`) pyta „z czego ten krok ma wybierać",
 * a **zakres per krok workflow jest świadomie poza zakresem T-44** (TASK.md, „Świadomie poza
 * zakresem": „Krok ma własny wybór umiejętności [T-13, `skills-row.tsx`] i to jest inne
 * pytanie"). Domyślna wartość jest więc zapisem tej granicy: tamten wołacz zostaje przy
 * odpowiedzi, którą miał przed tym zadaniem — samym korzeniem globalnym.
 *
 * `= null`, a nie `folder?: string`, i to nie jest kwestia stylu. `JSON.stringify` **zdejmuje**
 * klucz o wartości `undefined`, a Tauri dopasowuje argumenty po nazwie i deserializuje je przed
 * wejściem w ciało komendy — brakujący klucz nie daje więc mniejszego wywołania, daje ODRZUCONE,
 * z odmową w postaci surowego napisu, którego nikt nie widzi. `null` dojeżdża i po tamtej stronie
 * jest `None`, czyli dokładnie „nie ma otwartego projektu".
 *
 * CENA JEST NAZWANA, bo domyślna wartość jest miękka: wołacz, który o folder zapomni, dostanie
 * po cichu samą listę globalną i kompilator go nie zapyta. Dopóki tak jest, ten akapit jest
 * jedynym miejscem, w którym to widać — a pytanie „czy panel kroku ma widzieć umiejętności
 * projektowe" jest otwarte i należy do T-13, nie do tego pliku.
 */
export function listSkills(folder: string | null = null): Promise<InstalledSkill[]> {
  return invoke<InstalledSkill[]>('list_skills', { folder });
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
 * Jedno zdanie człowieka → trzy pola napisane przez agenta, którego wybrał.
 *
 * 2026-08-19 — trzecie wejście do tej sekcji. Adres i formularz wymagały, żeby człowiek napisał
 * treść sam; ta droga zamienia jedno zdanie w tekst od modelu, który człowiek CZYTA przed
 * zapisem. Nic tu nie zapisuje: trzy pola lądują w formularzu z T-42 i dopiero `authorSkill`
 * składa z nich plik, skanuje go i odkłada kopię kanoniczną (niezmiennik 23).
 *
 * `null` znaczy „człowiek to zatrzymał" i jest **wartością**, nie odmową (niezmiennik 7): po niej
 * gaśnie stan „pisze" i nie ma ani draftu, ani zdania o awarii.
 *
 * Jedzie `id` wybranego agenta, nie jego nazwa i nie nazwa vendora: model, prompt systemowy
 * i dial bezpieczeństwa liczy Rust z zapisanej definicji, a nazwy vendorów nie ma prawa być
 * w tej sekcji ani na ekranie, ani w kodzie (`mounted.test.tsx` zamraża ich brak w markupie).
 */
export function askAnAgent(want: string, agent: string): Promise<Authored | null> {
  return invoke<Authored | null>('draft_skill', { want, agent });
}

/**
 * „Stop" dla draftu: zatrzymaj agenta, który pisze umiejętność.
 *
 * Bez argumentów, bo draft jest jeden naraz — a uchwyt do tego, który pisze teraz, mieszka po
 * tamtej stronie granicy. Osobna komenda od `stop` w sekcji Praca: jedna na oba znaczyłaby, że
 * Stop tutaj ubija bieg w sąsiedniej karcie.
 */
export function stopWriting(): Promise<void> {
  return invoke<void>('stop_draft');
}

/**
 * Zapisz przejrzaną umiejętność w katalogach vendorów wybranego zakresu.
 *
 * Jedzie CAŁY przegląd, nie samo ciało: na dysk ma trafić dokładnie ten tekst, który został
 * przeskanowany, a nie tekst złożony jeszcze raz po drodze.
 *
 * 2026-08-19 — JEDZIE TEŻ WYBÓR I FOLDER. Zakres bez korzenia projektu nie odmawia po tamtej
 * stronie po cichu — `place::destinations` oddaje wtedy ścieżki WZGLĘDNE — więc obie wartości
 * jadą razem, a odpowiedź „który to projekt" liczy Rust jedną funkcją, tą samą, którą pyta
 * o nią Start biegu.
 */
export function install(item: Import, landing: Landing, folder: string | null): Promise<void> {
  return invoke<void>('install_skill', { item, landing, folder });
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
 *
 * 2026-08-19 — ZAKRES I FOLDER, bo ta sama nazwa w dwóch zakresach to dwie rzeczy. „Zabierz
 * z tego projektu" ma zostawić kopię, która leży u człowieka i jest używana przez każdy inny
 * projekt; zabranie obu naraz jest inną czynnością, o którą nikt nie prosił.
 */
export function remove(name: string, landing: Landing, folder: string | null): Promise<void> {
  return invoke<void>('delete_skill', { name, landing, folder });
}
