# FrigatePiP — instrukcja obsługi

**Languages:** [English](../en/user-guide.md) · [Polski](../pl/user-guide.md) · [Deutsch](../de/user-guide.md) · [Español](../es/user-guide.md) · [Português](../pt/user-guide.md) · [Français](../fr/user-guide.md)

FrigatePiP to mała aplikacja dla systemu Windows, działająca w zasobniku systemowym, przeznaczona dla [Frigate NVR](https://frigate.video). Gdy Frigate wykryje osobę, samochód lub dowolny inny obiekt, FrigatePiP natychmiast pokazuje w rogu ekranu małe okno typu obraz w obrazie (picture-in-picture) z wideo na żywo z tej kamery. Gdy nic się już nie rusza, okno zamyka się samo.

Istnieje też wersja na konsolę Linuksa, bez pulpitu — zob. [Wersja konsolowa dla Linuksa](#12-wersja-konsolowa-dla-linuksa).

## Spis treści

1. [Wymagania](#1-wymagania)
2. [Instalacja](#2-instalacja)
3. [Pierwsze uruchomienie](#3-pierwsze-uruchomienie)
4. [Ikona w zasobniku](#4-ikona-w-zasobniku)
5. [Okna podglądu](#5-okna-podglądu)
6. [Okno ustawień](#6-okno-ustawień)
7. [Plik ustawień](#7-plik-ustawień)
8. [Wiersz poleceń](#8-wiersz-poleceń)
9. [Język](#9-język)
10. [Pliki, prywatność i zasoby](#10-pliki-prywatność-i-zasoby)
11. [Rozwiązywanie problemów](#11-rozwiązywanie-problemów)
12. [Wersja konsolowa dla Linuksa](#12-wersja-konsolowa-dla-linuksa)

---

## 1. Wymagania

- **Windows 10 (wersja 1809 lub nowsza) lub Windows 11**, wersja 64-bitowa.
- **Frigate** dostępny z Twojego komputera przez sieć. Zalecany jest Frigate 0.14 lub nowszy; tryby *Review* wymagają wersji 0.14 lub nowszej.
- **Port API 5000** Frigate musi być dostępny z komputera. FrigatePiP się nie loguje, więc nie można użyć portu 8971 wymagającego uwierzytelnienia. W Dockerze opublikuj port, np. `- "5000:5000"`.
- **Dodatek Frigate do Home Assistant:** port 5000 jest domyślnie wyłączony — włącz go w ustawieniach *Sieć* dodatku. Adresy HTTPS nie są obsługiwane: użyj zwykłego adresu `http://` w sieci lokalnej.
- Do płynnego wideo na żywo: wbudowany we Frigate **serwer RTSP go2rtc, port 8554** (`- "8554:8554"`). Bez niego nadal możesz korzystać z MJPEG lub klatek (zob. [Obraz na żywo](#obraz-na-żywo)).
- Opcjonalnie: **broker MQTT** używany przez Frigate, jeśli wolisz MQTT od WebSocketu Frigate.
- W przypadku kamer H.265 (HEVC) system Windows potrzebuje dekodera HEVC. Większość komputerów z aktualnym sterownikiem karty graficznej go ma; w przeciwnym razie zainstaluj *Rozszerzenia wideo HEVC* (*HEVC Video Extensions*) ze sklepu Microsoft Store albo użyj sub-strumienia H.264. Jeśli dekodera brak, FrigatePiP automatycznie pokazuje zamiast tego MJPEG z Frigate.
- **Wydania Windows N** (np. Windows 11 Pro N) nie mają Media Foundation, dopóki nie dodasz funkcji opcjonalnej Windows *Media Feature Pack*. FrigatePiP działa także bez niej, ale obraz na żywo pochodzi wtedy z MJPEG z Frigate (ok. 5 klatek na sekundę), a dźwięk z kamery w formacie AAC jest niedostępny.

## 2. Instalacja

### Za pomocą instalatora (zalecane)

1. Pobierz `FrigatePiP-<version>-installer.exe` ze strony [Releases](https://github.com/maciejgornicz/frigate-pip/releases).
2. Uruchom go. Instalator nie wymaga uprawnień administratora; instaluje program tylko dla bieżącego użytkownika, w folderze `%LOCALAPPDATA%\Programs\FrigatePiP`.
3. Wybierz, czy zaznaczyć **Uruchamiaj FrigatePiP razem z Windows** (domyślnie włączone) i czy utworzyć skrót na pulpicie.
4. Na koniec pozostaw zaznaczone **Uruchom aplikację FrigatePiP**. Pojawi się ikona w zasobniku, a przy pierwszym uruchomieniu otworzy się okno ustawień.

Instalator i aplikacja nie są podpisane certyfikatem podpisywania kodu. Windows SmartScreen może wyświetlić komunikat *„System Windows ochronił Twój komputer”*; kliknij **Więcej informacji → Uruchom mimo to**. Możesz sprawdzić sumę kontrolną SHA-256 publikowaną przy każdym wydaniu.

### Wersja przenośna (bez instalacji)

Pobierz `FrigatePiP-<version>-portable.exe` ze strony Releases i uruchom go z dowolnego folderu. Ustawienia są przechowywane w tym samym miejscu co w wersji z instalatorem (zob. [Pliki](#10-pliki-prywatność-i-zasoby)). Aby program uruchamiał się automatycznie, włącz w ustawieniach opcję **Uruchamiaj FrigatePiP razem z Windows**.

### Aktualizacja

FrigatePiP raz dziennie sprawdza, czy jest nowa wersja (opcja **Sprawdzaj aktualizacje** w ustawieniach). Gdy jest dostępna, pojawia się powiadomienie, a menu ikony i okno ustawień proponują **Zaktualizuj do …**: FrigatePiP pobiera instalator z GitHuba, sprawdza jego sumę kontrolną SHA-256 i go uruchamia — aktualizacja trwa kilka sekund i zachowuje Twoje ustawienia. **Sprawdź teraz** w ustawieniach od razu szuka nowej wersji. Możesz też samodzielnie uruchomić instalator nowej wersji. Wersja przenośna tylko otwiera stronę wydania: pobierz nowy plik exe i podmień stary.

### Odinstalowanie

**Ustawienia → Aplikacje → Zainstalowane aplikacje → FrigatePiP → Odinstaluj** (lub *Dodaj lub usuń programy*). Deinstalator zamyka FrigatePiP i usuwa program, skróty oraz wpis autostartu. Twoje ustawienia i log w `%APPDATA%\FrigatePiP` zostają zachowane; aby je również usunąć, usuń ten folder.

## 3. Pierwsze uruchomienie

1. Przy pierwszym uruchomieniu okno ustawień otwiera się na karcie **Połączenie**.
2. Wpisz **Adres Frigate**, np. `http://192.168.1.10:5000`.
3. Kliknij **Testuj połączenie**. FrigatePiP sprawdza, niczego nie zapisując:
   - API Frigate i jego wersję,
   - listę kamer,
   - źródło zdarzeń (WebSocket lub MQTT),
   - wideo RTSP z go2rtc, w tym kodek i to, czy kamera ma dźwięk.

   Każdy wiersz zaczyna się od ✔ (działa) lub ✖ (problem, ze wskazówką).
4. Kliknij **OK**. Od teraz FrigatePiP nasłuchuje wykryć w tle.
5. Aby zobaczyć, jak wygląda okno, kliknij prawym przyciskiem myszy ikonę w zasobniku i wybierz **Pokaż kamerę → (kamera)** albo zaznacz **Pokaż na żywo** na dole okna ustawień.

## 4. Ikona w zasobniku

Ikona w obszarze powiadomień pokazuje stan:

| Kolor | Znaczenie |
|---|---|
| Niebieska | Połączono z Frigate, oczekiwanie na wykrycia |
| Szara | Brak połączenia (FrigatePiP automatycznie ponawia próby) |
| Pomarańczowa | Powiadomienia są wstrzymane |

Najedź kursorem na ikonę, aby zobaczyć stan (na przykład *Połączono: http://192.168.1.10:5000*).

- **Kliknięcie lewym przyciskiem** otwiera okno ustawień.
- **Kliknięcie prawym przyciskiem** otwiera menu:

| Element menu | Działanie |
|---|---|
| *(wiersz stanu)* | Stan połączenia lub pozostały czas wstrzymania (tylko informacja). |
| **Zaktualizuj do …** | Pojawia się, gdy jest dostępna nowa wersja: pobiera ją i instaluje (zob. [Aktualizacja](#aktualizacja)). |
| **Ustawienia…** | Otwiera okno ustawień. |
| **Pokaż kamerę** | Od razu otwiera okno podglądu wybranej kamery, nawet gdy powiadomienia są wstrzymane. Przydaje się do sprawdzenia obrazu albo po prostu do podejrzenia, co się dzieje. |
| **Wstrzymaj powiadomienia** | **Na 15 minut**, **Na 1 godzinę** lub **Do wznowienia**. W czasie wstrzymania wykrycia nie otwierają okien. Gdy wstrzymanie jest aktywne, ten element menu ma nazwę **Wstrzymano**, a **Wznów teraz** kończy wstrzymanie. |
| **Zamknij okna podglądu** | Zamyka wszystkie otwarte okna podglądu, także przypięte. |
| **Otwórz Frigate** | Otwiera interfejs webowy Frigate w przeglądarce. |
| **Pomoc** | Otwiera tę instrukcję. |
| **Pokaż log** | Otwiera plik logu w Notatniku (przydatne, gdy coś nie działa). |
| **Plik ustawień (zaawansowane)…** | Otwiera `config.ini` w Notatniku. Zapisane zmiany działają od razu. |
| **Zakończ** | Zamyka FrigatePiP. |

## 5. Okna podglądu

Gdy wykrycie pasuje do Twoich ustawień, w wybranym rogu ekranu pojawia się okno podglądu:

1. Okno pojawia się od razu, w ciągu kilku milisekund od wiadomości Frigate.
2. Najpierw pokazuje najnowszą klatkę z kamery zapisaną przez Frigate, a potem przełącza się na wideo na żywo, gdy tylko strumień dostarczy obraz (zwykle w ciągu sekundy, z [prebuforem](#prebufor) niemal natychmiast).
3. Napis (jeśli jest włączony) pokazuje nazwę kamery, obiekt (na przykład *osoba*), podetykietę (na przykład rozpoznaną twarz lub tablicę rejestracyjną), pewność i godzinę. **Czerwona kropka** oznacza, że obraz jest na żywo.

Okna podglądu nigdy nie zabierają fokusu klawiatury, więc nie przerywają pisania ani gry.

### Obsługa myszą

| Czynność | Efekt |
|---|---|
| **Przeciągnięcie** | Przesuwa okno. Okno zostaje tam, gdzie je umieścisz, aż się zamknie. |
| **Kliknięcie** | **Przypina** okno: zostaje otwarte, dopóki go nie zamkniesz (napis pokazuje *przypięte*). Kliknij ponownie, aby je odpiąć. |
| **Kółko myszy** | Powiększa lub zmniejsza okno. |
| **×** (prawy górny róg) | Od razu zamyka okno. Przycisk pojawia się po najechaniu myszą na okno. |
| **Kliknięcie prawym przyciskiem** | Zamyka okno. |
| **Dwukrotne kliknięcie** | Otwiera tę kamerę w interfejsie webowym Frigate. |
| **Najechanie kursorem** | Dopóki wskaźnik myszy jest nad oknem, okno się nie zamyka. Gdy odsuniesz kursor, okno pozostaje jeszcze co najmniej 3 sekundy. |

### Kiedy okno się zamyka?

- Po upływie liczby sekund ustawionej w **Czas po ostatnim ruchu**, licząc od ostatniego ruchu obiektu (lub dowolnego innego pasującego obiektu na tej kamerze).
- Najpóźniej po upływie liczby sekund ustawionej w **najdłużej**, nawet jeśli obiekt wciąż się porusza.
- Nigdy, gdy jest przypięte lub gdy wskaźnik myszy znajduje się nad nim.

### Więcej niż jedna kamera

Każda kamera ma najwyżej jedno okno. Nowe wykrycia na kamerze, która ma już okno, tylko je przedłużają i aktualizują napis. Okna różnych kamer układają się w stos od wybranego rogu. **Maks. liczba okien** ogranicza, ile okien może być otwartych jednocześnie; **Gdy wszystkie są zajęte** decyduje, czy nowa kamera zastępuje najstarsze okno (w tym samym miejscu na ekranie; przypięte okna nigdy nie są zastępowane), czy jest pomijana.

## 6. Okno ustawień

Otwórz je, klikając lewym przyciskiem myszy ikonę w zasobniku, lub poleceniem **Ustawienia…** w menu. **Każda opcja ma opis: najedź na nią kursorem myszy.** Naciśnij **F1** lub kliknij **Pomoc**, aby otworzyć tę instrukcję.

Większość zmian działa **od razu** na otwarte okna podglądu, więc widzisz efekt przed zapisaniem. **OK** zapisuje i zamyka, **Zastosuj** zapisuje i pozostawia okno otwarte, **Anuluj** (lub zamknięcie okna) odrzuca niezapisane zmiany.

### 6.1 Połączenie

| Opcja | Opis |
|---|---|
| **Adres Frigate** | Adres serwera Frigate z portem, np. `http://192.168.1.10:5000` lub `http://frigate.local:5000`. Port 5000 nie wymaga logowania. |
| **Źródło zdarzeń** | **WebSocket Frigate (zalecane)** — FrigatePiP łączy się bezpośrednio z Frigate; nic więcej nie jest potrzebne. **MQTT** — wykrycia są odczytywane z Twojego brokera MQTT (tego skonfigurowanego we Frigate). Oba sposoby dają ten sam wynik; WebSocket jest zwykle nieco szybszy. |
| **Serwer MQTT**, **port** | Nazwa hosta lub adres IP oraz port (zwykle 1883) brokera MQTT. Używane tylko przy źródle MQTT. |
| **Użytkownik / hasło** | Dane logowania MQTT. Zostaw puste, jeśli broker pozwala na dostęp anonimowy. |
| **Prefiks tematów MQTT** | `mqtt.topic_prefix` z konfiguracji Frigate, zwykle `frigate`. |
| **Adres RTSP (go2rtc)** | Szablon adresu wideo na żywo. `{host}` jest zastępowany hostem z adresu Frigate, a `{stream}` nazwą strumienia go2rtc. Domyślny `rtsp://{host}:8554/{stream}` pasuje do standardowej instalacji Frigate. Jeśli go2rtc wymaga logowania, użyj `rtsp://user:password@{host}:8554/{stream}`. |
| **Testuj połączenie** | Sprawdza wpisane wyżej wartości (Frigate, kamery, zdarzenia, wideo RTSP i dźwięk), nie zapisując ich. |
| **Język** | Język FrigatePiP. **Automatycznie (Windows)** oznacza język wyświetlania systemu Windows; gdy ten język nie jest dostępny, używany jest angielski. Zob. [Język](#9-język). |
| **Uruchamiaj FrigatePiP razem z Windows** | Uruchamia FrigatePiP automatycznie po zalogowaniu do Windows. |
| **Sprawdzaj aktualizacje** | Raz dziennie FrigatePiP pyta GitHub, czy jest nowa wersja, a jeśli tak — pokazuje powiadomienie i proponuje aktualizację. Nie są wysyłane żadne dane o Tobie ani o kamerach. |
| **Sprawdź teraz** / **Zaktualizuj do …** | Od razu szuka nowej wersji; gdy ją znajdzie, przycisk pobiera ją i instaluje (ustawienia zostają zachowane). Tekst obok pokazuje wynik. |

### 6.2 Wykrywanie

| Opcja | Opis |
|---|---|
| **Pokazuj okno dla** | Które wiadomości Frigate otwierają okno: **Każdego wykrytego obiektu (najszybciej)** — każdy obiekt śledzony przez Frigate; **Tylko alertów z Review** — tylko elementy Review o poziomie ważności *alert*; **Alertów i detekcji z Review** — wszystkie elementy Review. Elementy Review podlegają regułom alertów i detekcji (etykiety i wymagane strefy) ustawionym w sekcji `review` konfiguracji Frigate i docierają nieco później niż zdarzenia obiektów. |
| **Kamery** | Kamery, które mogą otworzyć okno. Odznaczone kamery są pomijane. Gdy zaznaczone są wszystkie kamery, uwzględniane są także kamery dodane do Frigate później. |
| **Obiekty** | Rodzaje obiektów (etykiety), które mogą otworzyć okno, np. osoba, samochód, pies. Lista pokazuje obiekty śledzone przez Twoje kamery. Gdy zaznaczone są wszystkie, liczy się każdy rodzaj obiektu. |
| **Wymagane strefy** | Jeśli zaznaczysz strefy, okno otworzy się dopiero, gdy obiekt wejdzie do jednej z nich. Bez zaznaczenia liczy się każde miejsce w kadrze. |
| **Pomijaj obiekty stojące w miejscu** | Obiekty oznaczone przez Frigate jako stojące (na przykład zaparkowane auto) nie otwierają okien i nie przedłużają ich pokazu. Dotyczy opcji *Każdego wykrytego obiektu*. |
| **Minimalna pewność** | Wykrycia o mniejszej pewności są pomijane. 0% oznacza, że decydują progi ustawione we Frigate. Dotyczy opcji *Każdego wykrytego obiektu*. |

Fałszywe wykrycia oznaczone przez Frigate (false positives) są zawsze pomijane. Każdy obiekt otwiera okno tylko raz; gdy znów się poruszy, jedynie przedłuża otwarte okno.

### 6.3 Okno podglądu

| Opcja | Opis |
|---|---|
| **Monitor** | Monitor, na którym pojawiają się okna: **Główny** lub konkretny monitor (numerowany od lewej, z podaną rozdzielczością). |
| **Róg ekranu** | Róg, w którym pojawiają się okna. Kolejne okna układają się w stos od tego rogu. |
| **Szerokość okna** | Szerokość w pikselach (przy skalowaniu 100%; FrigatePiP dostosowuje ją do skalowania monitora). Wysokość wynika z proporcji obrazu kamery. Rozmiar pojedynczego okna możesz też zmienić kółkiem myszy. |
| **Odstęp od krawędzi** | Odstęp między oknami a krawędziami ekranu (oraz między oknami). |
| **Czas po ostatnim ruchu** | Ile sekund okno zostaje otwarte po ostatnim ruchu. |
| **najdłużej** | Maksymalny czas w sekundach, przez jaki okno jest pokazywane, nawet jeśli obiekt wciąż się rusza (chyba że okno jest przypięte). |
| **Maks. liczba okien** | Ile okien (kamer) może być otwartych jednocześnie (1–8). |
| **Gdy wszystkie są zajęte** | **Zastąp najstarsze okno (w tym samym miejscu)** — nowa kamera zajmuje miejsce najstarszego nieprzypiętego okna; **Pomiń nowe zdarzenie** — nowe wykrycie jest pomijane. |
| **Zawsze na wierzchu** | Utrzymuje okna nad wszystkimi innymi oknami. |
| **Napis na obrazie** | Pokazuje na obrazie kamerę, obiekt, pewność i godzinę. |
| **Dźwięk powiadomienia** | Odtwarza dźwięk Windows *Gwiazdka*, gdy pojawia się nowe okno. To nie jest dźwięk z kamery (zob. [Dźwięk z kamery](#dźwięk-z-kamery)). |
| **Nie pokazuj podczas gry lub prezentacji na pełnym ekranie** | Żadne okna się nie pojawiają, gdy działa gra, film lub prezentacja na pełnym ekranie (wykrycia są pomijane, a nie odkładane na później). |

### 6.4 Wideo i dźwięk

#### Obraz na żywo

| Wybór | Opis |
|---|---|
| **RTSP z go2rtc** (domyślnie) | Płynne wideo z liczbą klatek na sekundę kamery i dekodowaniem sprzętowym (H.264 i H.265), małe opóźnienie. Wymaga portu 8554. |
| **MJPEG z Frigate** | Około 5 klatek na sekundę w rozdzielczości detekcji, przez port 5000. Używaj, gdy RTSP nie jest dostępny. |
| **Tylko odświeżane klatki** | Tylko klatki z Frigate, regularnie odświeżane. Najlżejsza opcja. |

#### Strumień

| Wybór | Opis |
|---|---|
| **Automatycznie (wg rozmiaru okna)** | Lekki strumień **sub** dla zwykłych okien, a strumień **main** w pełnej rozdzielczości, gdy okno jest szersze niż wartość **Main od szerokości okna** (w pikselach fizycznych). |
| **Sub (lekki)** | Zawsze strumień sub. |
| **Main (pełna rozdzielczość)** | Zawsze strumień main. |

FrigatePiP znajduje strumienie go2rtc każdej kamery w konfiguracji Frigate (`live.streams`, `live.stream_name` lub strumienie go2rtc o nazwach `<camera>` i `<camera>_sub`). Możesz je też ustawić samodzielnie w [pliku ustawień](#ustawienia-poszczególnych-kamer).

#### Dźwięk z kamery

**Dźwięk z kamery** odtwarza dźwięk kamery (AAC lub G.711 ze strumienia RTSP) w każdym otwartym oknie. Działa osobno, więc nigdy nie opóźnia okna ani wideo. Głośność możesz ustawić w Mikserze głośności Windows (*FrigatePiP*). Wymaga opcji **RTSP z go2rtc**.

#### Prebufor

**Prebufor** utrzymuje połączenie z sub-strumieniem każdej kamery, dzięki czemu wideo na żywo startuje od razu (ok. 0,15 s), zamiast czekać na najbliższą klatkę kluczową (do ok. 1 s, zależnie od interwału klatek kluczowych kamery). Koszt: ok. 0,1–0,5 Mb/s ruchu w sieci lokalnej na kamerę, przez cały czas.

#### Gotowe GPU

**Gotowe GPU** utrzymuje zainicjowane urządzenie graficzne, dzięki czemu wideo startuje ok. 130 ms szybciej. Koszt: ok. 1% jednego rdzenia CPU w spoczynku (wątek sterownika graficznego).

### 6.5 Dolna część okna

| Element | Opis |
|---|---|
| **Podgląd:** kamera | Kamera używana do podglądu ustawień na żywo. |
| **Pokaż na żywo** | Otwiera prawdziwe okno podglądu tej kamery. Od razu odzwierciedla Twoje zmiany (róg, rozmiar, napis, strumień, dźwięk…), zanim je zapiszesz. Jest przypięte i nie wlicza się do limitu okien. |
| **OK** / **Anuluj** / **Zastosuj** | Zapisz i zamknij / odrzuć zmiany / zapisz i pozostaw okno otwarte. |
| **Pomoc** | Otwiera tę instrukcję (także **F1**). |

## 7. Plik ustawień

Wszystkie ustawienia są przechowywane w `%APPDATA%\FrigatePiP\config.ini` (zwykły plik tekstowy). Okno ustawień zapisuje je wszystkie; plik jest potrzebny tylko do ustawień poszczególnych kamer. **Plik ustawień (zaawansowane)…** w menu w zasobniku otwiera go w Notatniku. Zmiany są stosowane automatycznie po zapisaniu pliku. Komentarze zaczynają się od `;`. Wartości tak/nie można zapisać jako `true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`.

| Sekcja | Klucz | Domyślnie | Znaczenie |
|---|---|---|---|
| `[app]` | `language` | `auto` | `auto` (język Windows) lub `en`, `pl`, `de`, `es`, `pt`, `fr`. |
| | `check_updates` | `true` | Raz dziennie szukaj nowej wersji. |
| `[frigate]` | `url` | `http://frigate.local:5000` | Adres Frigate. |
| | `source` | `websocket` | `websocket` lub `mqtt`. |
| | `rtsp` | `rtsp://{host}:8554/{stream}` | Szablon adresu RTSP. |
| `[mqtt]` | `host` | *(puste)* | Broker MQTT (tylko przy `source = mqtt`). |
| | `port` | `1883` | Port brokera. |
| | `user`, `password` | *(puste)* | Dane logowania. |
| | `prefix` | `frigate` | Prefiks tematów. |
| `[trigger]` | `mode` | `events` | `events` (każdy obiekt), `alerts` (alerty z Review) lub `reviews` (alerty i detekcje). |
| | `cameras` | *(puste = wszystkie)* | Nazwy kamer oddzielone przecinkami, np. `front_door, garden`. |
| | `labels` | *(puste = wszystkie)* | Obiekty oddzielone przecinkami, np. `person, car`. |
| | `zones` | *(puste = dowolne miejsce)* | Wymagane strefy. |
| | `ignore_stationary` | `true` | Pomijanie obiektów stojących w miejscu. |
| | `min_score` | `0` | Minimalna pewność 0.0–1.0. |
| `[display]` | `monitor` | `primary` | `primary` lub numer monitora od lewej (1, 2, 3…). |
| | `position` | `bottom-right` | `top-left`, `top-right`, `bottom-left`, `bottom-right`. |
| | `margin` | `16` | Odstęp od krawędzi ekranu w pikselach. |
| | `width` | `480` | Szerokość okna w pikselach. |
| | `duration` | `15` | Sekundy po ostatnim ruchu. |
| | `max_duration` | `120` | Maksymalny czas pokazywania okna w sekundach. |
| | `topmost` | `true` | Zawsze na wierzchu. |
| | `sound` | `false` | Dźwięk powiadomienia. |
| | `overlay` | `true` | Napis na obrazie. |
| | `max_popups` | `4` | Maks. liczba okien. |
| | `replace` | `true` | `true` = zastąp najstarsze okno, `false` = pomiń nowe zdarzenie. |
| | `skip_fullscreen` | `false` | Niepokazywanie okien podczas aplikacji pełnoekranowych. |
| `[video]` | `live` | `rtsp` | `rtsp`, `mjpeg` lub `snapshot`. |
| | `stream` | `auto` | `auto`, `sub` lub `main`. |
| | `auto_main_width` | `800` | Szerokość okna (px), od której `auto` używa strumienia main. |
| | `prebuffer` | `false` | Prebufor. |
| | `gpu_warm` | `false` | Gotowe GPU. |
| | `camera_audio` | `false` | Dźwięk z kamery. |

### Ustawienia poszczególnych kamer

Dodaj sekcję `[camera.<name>]`, gdzie `<name>` to nazwa kamery we Frigate:

```ini
[camera.front_door]
; Name shown in the caption, the menu and the settings
name = Front door
; go2rtc stream names
main = front_door
sub = front_door_sub
```

## 8. Wiersz poleceń

| Polecenie | Efekt |
|---|---|
| `frigate-pip.exe` | Uruchamia FrigatePiP (jednocześnie działa tylko jedna kopia). |
| `frigate-pip.exe --settings` | Otwiera okno ustawień (uruchamia FrigatePiP, jeśli nie działa). |
| `frigate-pip.exe --test <camera>` | Od razu pokazuje okno podglądu kamery, np. `--test front_door`. Możesz utworzyć skrót Windows z klawiszem skrótu do ulubionej kamery. |
| `frigate-pip.exe --quit` | Zamyka działający FrigatePiP. |

Jeśli FrigatePiP już działa, polecenie zostaje do niego przekazane.

## 9. Język

FrigatePiP jest dostępny w językach: angielskim, polskim, niemieckim, hiszpańskim, portugalskim i francuskim. Przy opcji **Automatycznie (Windows)** używa języka wyświetlania systemu Windows (*Ustawienia → Czas i język → Język i region → Język wyświetlania systemu Windows*), a jeśli ten język nie jest dostępny — angielskiego. Język możesz wybrać samodzielnie w **Połączenie → Język**; okno ustawień przełącza się od razu.

## 10. Pliki, prywatność i zasoby

| Co | Gdzie |
|---|---|
| Program | `%LOCALAPPDATA%\Programs\FrigatePiP\frigate-pip.exe` (wersja z instalatorem) |
| Ustawienia | `%APPDATA%\FrigatePiP\config.ini` |
| Log | `%APPDATA%\FrigatePiP\frigate-pip.log` (do 512 KB; poprzedni jest zachowywany jako `frigate-pip.old.log`) |
| Autostart | Wartość rejestru `FrigatePiP` w `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |

**Prywatność:** FrigatePiP łączy się tylko z adresami, które wpiszesz (Frigate, go2rtc i broker MQTT), oraz — jeśli nie wyłączysz opcji **Sprawdzaj aktualizacje** — raz dziennie z GitHubem, aby sprawdzić, czy jest nowa wersja. Nie ma telemetrii, kont ani usług w chmurze. Obrazy są tylko wyświetlane i nigdy nie są zapisywane. Polecenia **Pomoc** otwierają tę instrukcję na GitHubie w Twojej przeglądarce.

**Zasoby:** podczas oczekiwania na wykrycia FrigatePiP praktycznie nie obciąża CPU i zajmuje ok. 5–10 MB pamięci. Wideo jest dekodowane przez kartę graficzną, gdy to możliwe. Bez działającej karty graficznej (np. w niektórych maszynach wirtualnych) rysuje obraz programowym rendererem WARP.

## 11. Rozwiązywanie problemów

Zacznij od **Testuj połączenie** w ustawieniach i od **Pokaż log** w menu w zasobniku.

**Ikona pozostaje szara (brak połączenia)**
- Sprawdź adres Frigate i port (5000) w przeglądarce na tym samym komputerze: `http://<frigate>:5000/api/version` powinien pokazać wersję.
- W Dockerze port 5000 musi być opublikowany. Port 8971 (z logowaniem) nie jest obsługiwany.
- Dodatek do Home Assistant: włącz port 5000 w ustawieniach *Sieć* dodatku.
- Adres musi zaczynać się od `http://`. HTTPS (np. przez reverse proxy) nie jest obsługiwany.
- Przy MQTT: sprawdź adres brokera, dane logowania i prefiks tematów.

**Nie pojawiają się żadne okna**
- Czy ikona jest pomarańczowa? Powiadomienia są wstrzymane — wybierz **Wznów teraz**.
- Sprawdź kartę **Wykrywanie**: kamery, obiekty i wymagane strefy. Gdy zaznaczone są wymagane strefy, obiekty muszą do nich wejść.
- Tryby *Review* zależą od konfiguracji review we Frigate; wypróbuj opcję **Każdego wykrytego obiektu**.
- **Nie pokazuj podczas gry lub prezentacji na pełnym ekranie** pomija okna, gdy na pierwszym planie jest aplikacja pełnoekranowa.
- **Maks. liczba okien** w połączeniu z **Pomiń nowe zdarzenie** pomija wykrycia, gdy wszystkie okna są otwarte.
- **Minimalna pewność** może być ustawiona zbyt wysoko.

**Okno pokazuje tylko nieruchomy obraz**
- Wideo na żywo wymaga portu RTSP 8554 serwera go2rtc. Sprawdź wiersz **Wideo RTSP** w wynikach **Testuj połączenie**.
- Jeśli go2rtc wymaga logowania, dodaj dane logowania do pola **Adres RTSP**.
- W przypadku kamer H.265 zainstaluj *Rozszerzenia wideo HEVC* albo użyj sub-strumienia H.264.
- Tymczasowo możesz wybrać **MJPEG z Frigate**.
- W Windows N bez *Media Feature Pack* wideo RTSP jest niedostępne; FrigatePiP automatycznie używa MJPEG (w logu: *MJPEG from Frigate instead of RTSP*).

**Wideo startuje powoli**
- Włącz **Prebufor** i **Gotowe GPU**.
- Skróć interwał klatek kluczowych (I-frame interval / GOP) sub-strumienia kamery do 1–2 s.

**Brak dźwięku z kamery**
- Opcja **Dźwięk z kamery** musi być zaznaczona, a **Obraz na żywo** ustawiony na **RTSP z go2rtc**.
- Strumień go2rtc musi zawierać dźwięk — **Testuj połączenie** pokazuje *dźwięk … Hz* lub *bez dźwięku*.
- Sprawdź głośność *FrigatePiP* w Mikserze głośności Windows.

**Coś innego** — zgłoś [problem na GitHubie](https://github.com/maciejgornicz/frigate-pip/issues) i dołącz odpowiedni fragment logu. Usuń adresy i nazwy kamer, których nie chcesz udostępniać.

---

## 12. Wersja konsolowa dla Linuksa

FrigatePiP działa także w Linuksie **bez pulpitu** — na przykład na Raspberry Pi z monitorem na ścianie albo na serwerze, z którym łączysz się przez SSH. Pokazuje te same okna podglądu, w jednym z trzech trybów:

| Tryb | Gdzie pojawiają się okna | Działa w tle? | Typowe zastosowanie | Jak uruchomić |
|---|---|---|---|---|
| **Framebuffer** | Na ekranie konsoli Linuksa bez X ani Wayland, w wybranym rogu, nad tekstem konsoli | **Tak** — jako usługa systemd | Raspberry Pi lub inny komputer z monitorem i bez pulpitu (wyświetlacz na ścianie) | `frigate-pip --output framebuffer`, jako usługa: zob. [Automatyczne uruchamianie](#automatyczne-uruchamianie-systemd) |
| **Terminal** | W oknie terminala, w którym działa FrigatePiP: wiersz z napisem, obraz i wiersz stanu | **Nie** — terminal pokazuje FrigatePiP, dopóki program działa (jak `top`) | Okno terminala lub panel tmux pozostawiony otwarty jako monitor kamer, także przez SSH | `frigate-pip` |
| **Okienka tmux** | Jako wyskakujące okienko tmux nad tym, czym akurat zajmujesz się w tmux; zamyka się samo | **Tak** — w ukrytym oknie tmux | Praca w tmux (lokalnie lub przez SSH), gdy chcesz, żeby kamery same wyskakiwały | `tmux new -d -s frigate-pip 'frigate-pip --output tmux'` — zob. [Okienka tmux](#okienka-tmux) |

**Na komputerze z Windows — także gdy pracujesz w WSL — używaj aplikacji dla Windows:** działa w obszarze powiadomień, a jej okna pojawiają się nad wszystkim, także nad terminalami. Wersja dla Linuksa w WSL przydaje się tylko do wypróbowania.

Gdy okno na framebufferze się zamyka, to, co było pod nim, wraca na ekran. W terminalu i w okienkach tmux obraz jest rysowany za pomocą **grafiki Kitty** (Kitty, WezTerm, Ghostty), **Sixel** (foot, Konsole, xterm, Windows Terminal 1.22+) lub — w każdym terminalu z 24-bitowym kolorem — kolorowymi znakami półbloku (niska rozdzielczość; okienka tmux zawsze używają półbloków).

Wideo dekoduje program **ffmpeg**; dane dostarcza mu własny klient RTSP FrigatePiP, dzięki czemu obraz pojawia się kilka milisekund po najbliższej klatce kluczowej kamery.

### Wymagania

- Linux na **x86_64** lub **ARM64** (Raspberry Pi 3, 4 lub 5 z systemem 64-bitowym).
- **ffmpeg**: `sudo apt install ffmpeg` (Debian, Ubuntu, Raspberry Pi OS) lub pakiet z Twojej dystrybucji.
- Do napisów na framebufferze: czcionka DejaVu (`fonts-dejavu-core`, zwykle instalowana razem z ffmpeg).
- Do framebuffera: członkostwo w grupie `video` (`sudo usermod -aG video $USER`, a potem ponowne zalogowanie).
- Frigate dostępny tak samo jak w wersji dla Windows (port API 5000, port RTSP 8554).

### Instalacja

1. Pobierz `FrigatePiP-<version>-linux-x86_64.tar.gz` lub `FrigatePiP-<version>-linux-arm64.tar.gz` (Raspberry Pi) ze strony [Releases](https://github.com/maciejgornicz/frigate-pip/releases).
2. Rozpakuj archiwum i umieść program w jednym z katalogów w ścieżce PATH:

   ```
   tar xzf FrigatePiP-*-linux-*.tar.gz
   sudo install frigate-pip /usr/local/bin/
   ```

3. Uruchom program raz: utworzy plik `~/.config/frigate-pip/config.ini`. W sekcji `[frigate]` ustaw `url` na adres swojego Frigate.
4. Sprawdź połączenie: `frigate-pip --test-connection`.
5. Uruchom `frigate-pip`. Za pomocą `--test <camera>` (lub klawiszy **1–9**) możesz od razu zobaczyć kamerę.

Program to pojedynczy plik bez dodatkowych bibliotek. Aby go zaktualizować, podmień plik; aby go usunąć, usuń plik i katalog `~/.config/frigate-pip`.

### Gdzie pojawiają się okna

Przy `--output auto` (domyślnie) FrigatePiP używa **framebuffera**, gdy działa w konsoli Linuksa (`TERM=linux`) lub bez terminala (jako usługa), a w pozostałych przypadkach — **terminala**. Tryb **okienek tmux** jest używany tylko wtedy, gdy go wybierzesz (`--output tmux` lub `output = tmux`). W terminalu tryb obrazu jest wybierany automatycznie: grafika Kitty, jeśli jest dostępna, w przeciwnym razie Sixel, jeśli terminal go zgłasza, a w ostateczności półbloki. Oba ustawienia można zmienić w pliku ustawień lub w wierszu poleceń.

### Okienka tmux

W tym trybie FrigatePiP nie ma własnego okna: czeka na wykrycia w tle i przy każdym z nich otwiera wyskakujące okienko [tmux](https://github.com/tmux/tmux) z kamerą nad panelami ostatnio używanego klienta tmux. Wymaga tmux 3.2 lub nowszego.

- Uruchom go raz, w ukrytej sesji tmux: `tmux new -d -s frigate-pip 'frigate-pip --output tmux'`. Aby go zatrzymać: `tmux kill-session -t frigate-pip`. Aby używać klawiszy (wstrzymanie, pokazanie kamery): `tmux attach -t frigate-pip`.
- Okienko pojawia się w wybranym **rogu ekranu** (`position`), a jego rozmiar zależy od **szerokości okna** (`width`). Zamyka się samo po zwykłym czasie (`duration`, `max_duration`) albo od razu po naciśnięciu **q**, **x** lub **Esc**.
- tmux pokazuje jedno okienko naraz: wykrycie na innej kamerze zastępuje otwarte okienko (lub jest pomijane, gdy `replace = false`).
- Aby obraz miał pełne kolory, tmux musi przekazywać dalej 24-bitowy kolor: dodaj `set -as terminal-features ",xterm-256color:RGB"` do `~/.tmux.conf` (użyj swojej wartości `$TERM` spoza tmux) i uruchom tmux ponownie. Gdy brakuje kolorów, FrigatePiP zapisuje tę wskazówkę w logu.
- Jeśli żaden klient tmux nie jest podłączony, wykrycia są tylko zapisywane w logu.

### Klawisze

Gdy FrigatePiP działa w terminalu lub w konsoli:

| Klawisz | Efekt |
|---|---|
| **q** (lub Ctrl+C) | Wyjście. |
| **x** (lub Esc) | Zamknij wszystkie okna podglądu. |
| **p** | Wstrzymaj / wznów powiadomienia. |
| **1**–**9** | Pokaż kamerę 1–9 (w kolejności z konfiguracji Frigate). |

### Wiersz poleceń

| Opcja | Efekt |
|---|---|
| `--output auto\|framebuffer\|terminal\|tmux` | Gdzie rysować okna (zob. tabelę trybów powyżej). |
| `--graphics auto\|kitty\|sixel\|blocks` | Jak rysować obrazy w terminalu. |
| `--framebuffer /dev/fb0` | Urządzenie framebuffera. |
| `--test <camera>` | Pokazuje kamerę zaraz po uruchomieniu. |
| `--test-connection` | Sprawdza połączenie z Frigate (jak **Testuj połączenie** w Windows) i kończy działanie. |
| `--version`, `--help` | Wersja, krótka pomoc. |

### Ustawienia

Wersja dla Linuksa używa tego samego pliku `config.ini` (w `~/.config/frigate-pip/` lub `$XDG_CONFIG_HOME/frigate-pip/`) co wersja dla Windows — zob. [Plik ustawień](#7-plik-ustawień). Zmiany są stosowane w ciągu dwóch sekund od zapisania pliku. Używane są następujące ustawienia: wszystko w `[app]` (język), `[frigate]`, `[mqtt]`, `[trigger]`, a w `[display]`: `position`, `margin`, `width`, `duration`, `max_duration`, `overlay`, `max_popups`, `replace`; w `[video]`: `live`, `stream`, `auto_main_width`, `camera_audio`; oraz sekcje `[camera.<name>]`. Ustawienia dostępne tylko w Windows (`monitor`, `topmost`, `sound`, `skip_fullscreen`, `prebuffer`, `gpu_warm`, `check_updates`) są pomijane.

Dodatkowe ustawienia w sekcji `[console]`:

| Klucz | Domyślnie | Znaczenie |
|---|---|---|
| `output` | `auto` | `auto`, `framebuffer`, `terminal` lub `tmux`. |
| `graphics` | `auto` | `auto`, `kitty`, `sixel` lub `blocks` (obrazy w terminalu). |
| `framebuffer` | `/dev/fb0` | Urządzenie framebuffera. |
| `ffmpeg` | `ffmpeg` | Program ffmpeg (nazwa lub pełna ścieżka). |
| `font` | *(DejaVu Sans Bold)* | Czcionka TrueType do napisów na framebufferze. |

Język wynika z ustawień regionalnych systemu (`LANG`), chyba że w `[app]` ustawiono `language`. Dźwięk z kamery jest odtwarzany przez PulseAudio/PipeWire, jeśli są dostępne, a w przeciwnym razie przez ALSA.

### Automatyczne uruchamianie (systemd)

Dla monitora wyświetlającego konsolę (np. Raspberry Pi bez pulpitu) utwórz plik `/etc/systemd/system/frigate-pip.service` (zastąp `pi` nazwą swojego użytkownika):

```ini
[Unit]
Description=FrigatePiP
After=network-online.target
Wants=network-online.target

[Service]
User=pi
SupplementaryGroups=video audio
ExecStart=/usr/local/bin/frigate-pip --output framebuffer
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Następnie: `sudo systemctl enable --now frigate-pip`. Log trafia do dziennika systemd (`journalctl -u frigate-pip`) oraz do `~/.config/frigate-pip/frigate-pip.log`. Aby migający kursor konsoli nie pojawiał się na oknach, dodaj `vt.global_cursor_default=0` do wiersza poleceń jądra (na Raspberry Pi: `/boot/firmware/cmdline.txt`).

### Rozwiązywanie problemów w Linuksie

- **„cannot start ffmpeg”** — zainstaluj ffmpeg (`sudo apt install ffmpeg`) lub ustaw `ffmpeg` w `[console]`.
- **„/dev/fb0: Permission denied”** — dodaj użytkownika do grupy `video` i zaloguj się ponownie (dla usługi: `SupplementaryGroups=video`).
- **Brak napisu na framebufferze** — zainstaluj `fonts-dejavu-core` lub ustaw `font` w `[console]`.
- **Obraz w terminalu jest mocno rozpikselowany** — to tryb półbloków; użyj terminala z grafiką Kitty lub Sixel (zob. wyżej) albo ustaw `graphics`.
- **Nie pojawia się okienko tmux** — sprawdź, czy klient tmux jest podłączony (`tmux list-clients`), czy tmux jest w wersji 3.2 lub nowszej (`tmux -V`) i czy proces w tle działa (`tmux ls` pokazuje sesję `frigate-pip`); w logu jest zapisane, dlaczego okienko nie zostało pokazane.
- **Blade kolory w okienkach tmux** — włącz 24-bitowy kolor w tmux (zob. [Okienka tmux](#okienka-tmux)).
- **Brak dźwięku** — dźwięk z kamery wymaga PulseAudio/PipeWire lub ALSA; użytkownik usługi musi należeć do grupy `audio`.
- **Aby uzyskać szczegółowy raport**, uruchom FrigatePiP poleceniem `FRIGATEPIP_FFMPEG_LOG=/tmp/ffmpeg.log frigate-pip` i dołącz do zgłoszenia koniec tego pliku oraz logu.
