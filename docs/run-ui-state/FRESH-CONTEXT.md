# Nowy kontekst bez wyniku poprzedniego biegu

Zgłoszenie właściciela 2026-09-10: po otwarciu aplikacji nowa rozmowa pokazywała
czerwone kafelki poprzedniego biegu. Dopiero uruchomienie następnego workflow je usuwało.

Przyczyna: efekt startowy w `src/sections/run/index.tsx` odczytywał najnowszy zakończony
bieg i automatycznie wywoływał `finishedRun`. Dodatkowo zakończony obraz należy do
folderu, więc pojawiał się również nad nową kartą rozmowy w tym samym folderze.

## Zakres

- `src/sections/run/index.tsx`: przy otwarciu odnajduj wyłącznie trwający bieg.
  Zakończone biegi pozostają dostępne przez istniejącą historię.
- `src/sections/run/visible-run.ts`: zakończony obraz pokazuj w jego karcie;
  osobna rozmowa ma podgląd przyszłego workflow. To wybór widoku, bez kasowania
  wyniku i bez zmiany stanu lub procesu biegu.
- `e2e/tests/a-new-conversation-does-not-inherit-a-failed-run.spec.ts`: sprawdź
  zachowanie przez prawdziwy renderer, kliknięcia i istniejącą atrapę granicy Tauri.

## Test

Nowe kryteria muszą paść na asercjach przed poprawką:

1. Start okna z zakończonym nieudanym biegiem w historii pokazuje Ready to run
   i oczekujące kroki. Jawne otwarcie historii nadal pokazuje pierwotny błąd.
2. Po porażce biegu kliknięcie New terminal pokazuje Ready to run. Powrót do karty
   biegu zachowuje jego wynik; żadna z tych czynności nie uruchamia ani nie zatrzymuje procesu.
3. Trwający bieg odnaleziony przy otwarciu nadal ma widoczny Stop.

Zawężone polecenie: `npx --no-install vitest run e2e/tests/a-new-conversation-does-not-inherit-a-failed-run.spec.ts`.
Istniejące kryteria zakończenia, historii i odzyskania żywego biegu pozostają weryfikacją regresji.
