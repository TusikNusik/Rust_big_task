# Rust Big Task (Stock notification app)- Stanisław Kalewski, Piotr Korcz


## Serwer
Serwer asynchronicznie wysyła requesty na stronę yahoo-finance i pobiera z niej aktualne ceny akcji. Serwer automatycznie słucha na `localhost:1234` więc przy uruchamianiu go nie trzeba nic wpisywać. Skróty akcji serwer czerpie z pliku `stocks_small.txt` lub `stocks.txt`, załączyłem `stocks_small.txt`, aby zademonstrować, gdyż przetwarzanie pliku `stocks.txt` zajmuje około 15 minut (aczykolwiek działa).

## Spełnienie planu
Moim zdaniem spełniliśmy pierwszą iterację.
Serwer oraz klient wymieniają komunikaty:
- add `stock_symbol` ABOVE `users_price`
- add `stock_symbol` BELOW `users_price`
- del `stock_symbol` ALOW/BELOW `users_price`

Na razie można mieć **tylko jeden rodzaj alertu na każdą akcję**, czyli albo ABOVE albo BELOW, aczykolwiek zmienimy to w przyszłości.

Serwer oraz klient korzystają z featerów biblioteki Tokio, oraz porozumiewają się za pomocą protokołów z pliku `protocol.rs`

## Klient
Klient łączy się z serwerem po TCP i umożliwia użytkownikowi dodawanie i usuwanie alertów cenowych z poziomu terminala. Po uruchomieniu próbuje połączyć się z serwerem, a następnie czeka na komendy wpisywane przez użytkownika. 

Działa w pełni asynchronicznie dzięki bibliotece Tokio - jednocześnie obsługuje wejście użytkownika oraz wiadomości przychodzące z serwera. Po odebraniu komunikatu `TRIGGER ...` wypisuje informację o przekroczeniu progu cenowego.

## Protocol
Cały sposób komunikacji między klientem a serwerem znajduje się w pliku `protocol.rs`. Plik ten zawiera:

- definicje wiadomości przesyłanych między stronami (`AddAlert`, `RemoveAlert`, `AlertTriggered`, `Error`),
- funkcje parsujące przychodzące linie tekstu i funkcje zamieniające struktury Rust na komendy tekstowe.

Komunikacja opiera się na prostych komendach tekstowych, z których każda kończy się znakiem nowej linii.
