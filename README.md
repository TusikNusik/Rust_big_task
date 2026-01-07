# Rust Big Task (Stock notification app)- Stanisław Kalewski, Piotr Korcz


## Serwer
Serwer asynchronicznie wysyła requesty na stronę yahoo-finance i pobiera z niej aktualne ceny akcji. Serwer automatycznie słucha na `localhost:1234` więc przy uruchamianiu go nie trzeba nic wpisywać. Skróty akcji serwer czerpie z pliku `stocks_small.txt` lub `stocks.txt`, załączyłem `stocks_small.txt`, aby zademonstrować, gdyż przetwarzanie pliku `stocks.txt` zajmuje około 15 minut (aczykolwiek działa).

Serwer korzysta z bazy danych `SQLite`. Do bazy ma dostęp tylko serwer, udostępnia/obsługuje żadania klientów. Serwer w przypadku alertów
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
* **iteratory

## Klient
Klient łączy się z serwerem po TCP i umożliwia użytkownikowi dodawanie i usuwanie alertów cenowych z poziomu terminala. Po uruchomieniu próbuje połączyć się z serwerem, a następnie czeka na komendy wpisywane przez użytkownika. 

## GUI 

## Protocol
Cały sposób komunikacji między klientem a serwerem znajduje się w pliku `protocol.rs`. Plik ten zawiera:

- definicje wiadomości przesyłanych między stronami (`AddAlert`, `RemoveAlert`, `AlertTriggered`, `CheckPrice`, `Error`, ...),
- funkcje parsujące przychodzące linie tekstu i funkcje zamieniające struktury Rust na komendy tekstowe.

Komunikacja opiera się na prostych komendach tekstowych, z których każda kończy się znakiem nowej linii.
