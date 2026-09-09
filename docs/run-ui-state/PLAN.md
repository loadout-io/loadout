# Naprawa walidacji i widoku biegu (0.4.1)

## Zakres

- Wspólna walidacja efektywnych uprawnień w `workflow/roster.rs` i `commands/run.rs`: Create/Update wymagają zapisu, Use może pozostać Look only; nadpisanie kroku jest wiążące. Panel problemów pokazuje konkretny krok i jawną naprawę. Bez automatycznej eskalacji.
- Brak opublikowanego planu zatrzymuje zależne kroki przez istniejące pomijanie schedulera, także przy Carry on; niezależne gałęzie mogą pracować. Wynik procesu nie zastępuje kontraktu planu.
- Stan biegu z księgi silnika trafia do UI jako kontrolna migawka przez `engine/line.rs`, `commands/run.rs`, `ipc/types.ts`, `state/run.ts`, `sections/run/io.ts` i renderery tego ekranu. Próby mają odrębne klucze, komunikat błędu wygrywa z Done vendora, wynik końcowy i czasy są faktami silnika.
- Końcowa migawka przechodzi przez niezawodną wysyłkę w `src-tauri/src/ipc.rs`, także gdy kolejka transkryptu jest pełna.
- Historia (`sections/run/history-command.ts`, ewentualne addytywne pola `commands/history.rs`) zachowuje tożsamość próby i wynik. Spóźnione zdarzenia poprzedniego startu nie zmieniają nowego biegu.
- Testy w modułach `src-tauri/tests/it/` oraz plikach frontendowych i e2e. Rejestracja w `it/main.rs`. Sprawdzenia drutu i mapowania nowych linii aktualizowane bez zmiany chronionych checków.
- Wersja w package/package-lock, Cargo/Cargo.lock, tauri.conf; opis wydania i diagnoza. Merge main, pełne CI, tag i wydanie GitHub.

## Test

Przed poprawką odtworzyć na działających punktach wejścia: Create+Look only przechodzi panel/Start; brak kandydata z Carry on uruchamia zależne próby; historia pętli ma powielone klucze; ekran pokazuje Finished po porażce i może brać stare Done dla nowego kroku. Wymagane czerwone asercje, potem zielone liczniki.

Po poprawce: oba vendory, Create/Update/Use, dziedziczone i nadpisane uprawnienia, zapis szkicu; odmowa przed pierwszym procesem; kandydat publikowany i czytany w następnym kroku; brak kandydata blokuje potomków i Serve bez powtórek; sukces/porażka/anulowanie w nagłówku, odrębne próby i stare kanały. Test widocznego zdania oraz izolowane uruchomienie aplikacji. Jeden ciężki Cargo naraz. Niezależny weryfikator Claude: DZIALA/NIE_DZIALA/NIE_WIEM.
