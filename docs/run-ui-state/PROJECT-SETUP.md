# Biblioteka należy do projektu

Decyzja właściciela 2026-09-10: nowy projekt zaczyna bez agentów, workflowów,
Knowledge i Context. Poprzedni bieg nie staje się stanem nowej rozmowy. Przenoszenie
konfiguracji jest osobną, jawną czynnością „Import setup from project”.

## Granice

- Zapisane elementy mieszkają pod `<projekt>/.loadout/`. Katalog aplikacji przechowuje
  listę projektów, instalacje/logowanie aplikacji agentowych oraz ustawienia samego
  Loadouta. Nie jest domyślną biblioteką żadnego projektu.
- Ten sam wybór obowiązuje listy, edytory, walidację, Start, /ask, lidera i jego
  narzędzia. Przełączenie projektu nie zatrzymuje procesów ani nie przepisuje plików.
- Asynchroniczne operacje pozostają przypisane do projektu, w którym je rozpoczęto.
- Dotychczasowa biblioteka globalna zostaje na dysku. Jest dostępnym źródłem jawnego
  importu, bez automatycznego kopiowania do nowych projektów.

## Import

Wybór projektu źródłowego → karty kategorii → wybór pojedynczych elementów z podglądem
→ import do bieżącego projektu. Kategorie: Agents, Workflows, Knowledge (notatki i
umiejętności), Context oraz Connections. Workflow pokazuje wymaganych agentów,
umiejętności, połączenia i materiały. Zależności są widoczne przed zatwierdzeniem.
Identyfikatory zostają w osobnych przestrzeniach projektów, więc odwołania workflow nadal działają. Kopie są niezależne; istniejących plików nie nadpisujemy. Identyczna wcześniej zaimportowana zależność jest używana bez ponownego kopiowania. Źródło jest tylko czytane.
Sekrety połączeń nie są częścią kopii; import pokazuje konieczność ich ustawienia.
Zapis każdego pliku jest atomowy. Błąd podczas kopiowania wycofuje niezmienione nowe pliki; nie obiecujemy jednej transakcji odpornej na utratę zasilania dla całego importu.
Historia, żywe procesy, automatyczne wyzwalacze i wyniki pracy nie są uruchamiane
ani kopiowane przez import konfiguracji.

## Pliki

Zakres obejmuje `commands/` (w tym nowy rdzeń project_setup), `ipc.rs`, rejestrację
komend, `bridge/library`, `skills`, katalogi oraz adaptery sekcji Agents, Workflows,
Knowledge, Context, Connections i Import, a także wybór projektu w powłoce.
Testy w `src-tauri/tests/it/`, `src/` i `e2e/tests/`. Bez zmian harnessu i jego bramek.

## Test

1. Regresja przed poprawką: pusty projekt nie widzi workflow ani wiedzy z globalnej
   biblioteki lub innego projektu. Start i lider nie rozwiążą obcego agenta.
2. Przeglądarka: przełączenie A → B usuwa dane A ze wszystkich odpowiednich ekranów;
   opóźniony zapis/odczyt A nie zmienia B. Nowy projekt jest pusty.
3. Prawdziwe pliki dwóch projektów: wybrany workflow wraz z zależnościami jest
   kopiowany, identyfikatory nadal się rozwiązują, edycja kopii nie zmienia źródła.
   Brak zaznaczenia, konflikty, nieaktualny podgląd i niepoprawne ścieżki nie
   nadpisują istniejącej pracy.
4. Przeglądarka: wybór projektu, kategorii i elementów, podgląd, informacja o
   zależnościach, zatwierdzenie i widoczny wynik. Klawiatura, Escape, fokus oraz
   wąskie okno. Oglądamy rzeczywisty zrzut ekranu.
5. Testy zawężone w pętli. Pełne CI i niezależna weryfikacja funkcjonalna przed
   lądowaniem i wydaniem. Nie dotykamy biegu właściciela w meetnotes.

Powtarzane wykonania tego samego kafelka biegu mają wspólną kartę, licznik faktycznych uruchomień i rozwijaną listę ich stanów. Relacje i licznik kroków dotyczą kafelków grafu; nazwa nie jest tożsamością.
