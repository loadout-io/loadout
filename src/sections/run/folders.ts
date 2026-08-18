/* Wybór folderu przez okno systemu — JEDNA droga na całe repo.
 *
 * DLACZEGO OSOBNY PLIK. To wywołanie ma dziś dwóch wołających i każdy pyta o to samo:
 * przełącznik zakresów w bocznym menu (`src/ui/shell/workspace-switcher.tsx`, przycisk
 * „Choose folder") i `/open` w wierszu wejścia ekranu Run. Dwie kopie tego wywołania to dwa
 * miejsca, w których mieszka odpowiedź na pytanie „skąd bierze się ścieżka" (niezmiennik 23).
 *
 * OKNO WYBORU FOLDERU JEST WTYCZKĄ TAURI, nie komendą Loadouta — `dialog:allow-open` stoi
 * w `src-tauri/capabilities/default.json` od T-01. Import stoi TU, a nie w `io.ts`, bo `io.ts`
 * jest krawędzią KOMEND biegu i mieszanie w nim wtyczki systemowej z `run_workflow` dałoby
 * jeden plik odpowiadający na dwa różne pytania.
 *
 * 2026-08-18 — TA FUNKCJA NIE ZAKŁADA JUŻ KARTY, i to nie jest uproszczenie, tylko naprawa
 * skutku, o który nikt nie prosił. Do dziś `chooseWorkingFolder` przy okazji otwierała kartę
 * folderu, bo karta ZNACZYŁA folder. Właściciel rozstrzygnął to inaczej: folder pracy jest
 * zakresem („workspace") wybieranym w bocznym menu, a karty na ekranie Run pokazują biegi.
 * Nagłówek `workspace-switcher.tsx` zgłasza ten skutek uboczny wprost i prosi o dokładnie to,
 * co jest tu zrobione: czysty wybór ścieżki, bez ani jednego zapisu do magazynu. Kto z tą
 * ścieżką co zrobi — dołoży zakres, zmieni mu nazwę — jest decyzją wołającego, nie tej funkcji.
 */
import { open as chooseFolder } from '@tauri-apps/plugin-dialog';

/** Nazwa folderu, czyli to, co widać, kiedy pełna ścieżka się nie mieści. */
export function folderName(path: string): string {
  return (
    path
      .split('/')
      .filter((part) => part !== '')
      .at(-1) ?? path
  );
}

/**
 * Pyta człowieka o folder. Oddaje kanoniczną ścieżkę albo `null`.
 *
 * `null` znaczy **anulowanie**, czyli wartość, nie błąd (niezmiennik 7): człowiek się rozmyślił
 * i nie ma o czym mówić. Odmowa samego okna wyboru jedzie wyjątkiem, bo to jest awaria i wołający
 * ma o niej powiedzieć zdaniem człowieka — z `why()`, nigdy z `instanceof Error`.
 */
export async function chooseWorkingFolder(): Promise<string | null> {
  const picked = await chooseFolder({
    directory: true,
    multiple: false,
    title: 'Choose a folder to work in',
  });
  /* Wtyczka oddaje `null` przy anulowaniu, a przy `multiple: false` nigdy tablicy — sprawdzamy
   * jednak typ, bo to jest granica z cudzym kodem, a nie nasza obietnica. */
  return typeof picked === 'string' ? picked : null;
}
