# Context — prosty przebieg przygotowania

Zmiana zlecona przez właściciela 2026-09-09: przygotowanie zestawu ma zajmować kilka kliknięć.

## Zachowanie

- Nowy zestaw: nazwa, materiał (tekst, Cmd+V obrazu albo Add files), Build context.
- Jeden ekran. Przycisk budowania najpierw zapisuje aktualne pola, przygotowuje potrzebne
  dokumenty, a dopiero potem uruchamia istniejący proces budowania. Odmowa zapisu albo
  przygotowania zatrzymuje dalszą pracę i zostawia materiał do poprawienia.
- Cel, instrukcja opracowania, wymagania i wybór agenta/modelu są dostępne pod Options.
  Dotychczasowe wartości zostają zachowane; pusty model nadal oznacza domyślny model CLI.
- Save draft jest dodatkową, spokojną czynnością; nie stanowi etapu wymaganego przed Build.
- Lista pokazuje każdy dodany materiał raz. Osobne wyniki importu pozostają dla odmów;
  uwagi przyjętego pliku stoją przy źródle. Ready oznacza gotowy zestaw, nie import pliku.
- Postęp domyślnie mówi, co się dzieje, zwykłym językiem. Szczegóły poszczególnych porcji
  i źródeł są rozwijane. Stop zostaje dostępny podczas budowania i przygotowania PDF.
- Wynik powstaje na tym samym ekranie. Zmienione materiały nie udają gotowej nowej wersji.

## Zakres

`src/sections/context/` (edytor, przyciski, lista, okablowanie), `src/state/context.ts`
(anulowanie lokalnego przygotowania), powiązane testy w `src/` i `e2e/tests/context-*.spec.ts`,
ten dokument. Bez zmian silnika, formatu biblioteki, uprawnień vendorów i wyroczni repo.

## Test

Najpierw czerwone kryterium przeglądarkowe na starej implementacji: po wpisaniu materiału
Build context jest dostępne od razu; jedno kliknięcie wysyła zapis aktualnych bajtów, czeka
na jego wynik i dopiero potem zleca build. Dwa szybkie kliknięcia nie zlecają dwóch buildów.
Odmowa zapisu pozostaje widoczna i nie dociera żądanie budowania. Options jest opcjonalne.
Pozostałe kryteria: jedna pozycja na import, zachowanie załączników i ustawień, widoczny
postęp/Stop, automatyczne przygotowanie PDF i wynik bez zmiany zakładki. Testy przeglądarkowe
dowodzą UI→IPC; natywna próba dotyczy rzeczywistego zapisu i agenta. Zawężone testy i checki
w pętli, pełne CI przy integracji.
