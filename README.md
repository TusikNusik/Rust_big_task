# Rust Big Task (Stock notification app)- Stanisław Kalewski, Piotr Korcz


## Serwer
Serwer asynchronicznie wysyła requesty na stronę yahoo-finance i pobiera z niej aktualne ceny akcji. Serwer automatycznie słucha na `localhost:1234` więc przy uruchamianiu go nie trzeba nic wpisywać. Skróty akcji serwer czerpie z pliku `stocks_small.txt` lub `stocks.txt`, załączyłem `stocks_small.txt`, aby zademonstrować, gdyż przetwarzanie pliku `stocks.txt` zajmuje około 15 minut (aczykolwiek działa).

Serwer korzysta z bazy danych `SQLite`. Do bazy ma dostęp tylko serwer, udostępnia/obsługuje żadania klientów.
## Baza danych
Baza danych `SQLite`. Przechowuje informacje o danych, nawet po rozłączeniu serwera.

Baza zawiera trzy tabele, `users`, `alerts`, `positions`, przechowujące odpowiednio informacje o :
* klientach (dane logowania), 
* wszystkich alertach (przynależność, rodzaj akcji, granica) 
* wszystkich pozycjach w portfelach (przynależność, ilość, suma wydana na akcje)

## Spełnienie planu
Moim zdaniem spełniliśmy nasz plan, wykonaliśmy wszystkie założone punkty:
* serwer został rozszerzony o dodatkowe funkcje sprawdzania ceny, kupowania oraz sprzedawania akcji. 
* interfejs graficzny pozwala na przystępne korzystanie z programu, notyfikacje dźwiękowe oraz wizualizacja posiadanych akcji, wybranych alertów.
* baza danych przechowuje informacje o użytkownikach. 

Wykorzystana funkcjonalność języka Rust:
* **asynchroniczność**: Serwer korzystaja z featerów biblioteki Tokio, cały projekt jest oparty na tej idei. Nagmienne korzystanie z `future`, aby nie oczekiwać biernie na wynik, wymkorzystanie `select!` do rozpoznania nadchodzącego zdarzenia, `spawn` do tworzenia nowych wątków z puli.
* **współbieżność**: Na poziomie zapisu do bazy danych oraz dostępu przez czytelników do mapy z cenami akcji.
* **borrowed types**: Nagminne użycie na poziomie całego projektu.
* **iteratory**: Użycie `map`, `filter`, `collect` oraz iteracji po kolekcjach przy przetwarzaniu list akcji, alertów i danych z bazy.

## Klient
Klient konsolowy łączy się z serwerem po TCP i pozwala użytkownikowi zarządzać alertami oraz portfelem. Umożliwia logowanie i rejestrację, dodawanie/usuwanie alertów, sprawdzanie ceny akcji, kupno i sprzedaż, a także pobieranie danych o alertach i posiadanych pozycjach. Jednocześnie wyświetla komunikaty zwrotne z serwera, w tym powiadomienia o spełnionych alertach.
## GUI 
Aplikacja desktopowa zbudowana w `eframe/egui`. Pozwala na łączenie z serwerem, logowanie/rejestrację, zarządzanie alertami, podgląd portfela oraz wysyłanie poleceń BUY/SELL/PRICE. Dla alertów wyświetla okno popup i emituje dźwięk. Wyświetlany jest tylko ostatni popup aby w przypadku wielu na raz użytkownik nie musiał wszystkich usuwać, a informacje o wszystkich innych alertach jest w logu.
## Protocol
Prosty protokół tekstowy, oparty o pojedyncze linie zakończone `\n`. Klient wysyła komendy: `ADD`, `DEL`, `LOGIN`, `REGISTER`, `PRICE`, `BUY`, `SELL`, `DATA`. Serwer wysyła komendy: `TRIGGER`, `ALERTADDED`, `ALERTDELETED`, `PRICE`, `BOUGHT`, `SOLD`, `DATA`, `LOGIN`, `REGISTER`, `ERR`.
## Test
Przy uruchamianiu testów e2e wymagany jest działający serwer.
