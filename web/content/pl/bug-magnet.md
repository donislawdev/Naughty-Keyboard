---
title: Bug Magnet i Naughty Keyboard
linkTitle: Bug Magnet
url: /pl/porownania/bug-magnet/
seoTitle: Bug Magnet i Naughty Keyboard - także poza przeglądarką
description: Bug Magnet i Naughty Keyboard porównane - menu prawego przycisku z wartościami testowymi w przeglądarce i skrót, który wpisuje je w dowolne okno.
lead: "Oba wstawiają w pole kłopotliwe wartości testowe, żeby nie wpisywać ich ręcznie. Jedno żyje w menu prawego przycisku przeglądarki, drugie w palecie sterowanej skrótami. Ta strona mówi, co robi każde z nich i kiedy rozszerzenie przeglądarki wystarczy."
---

## Czym jest Bug Magnet?

Rozszerzeniem do Chrome i Firefoksa autorstwa Gojko Adzica, rozwijanym od 2015 roku i wydanym na licencji MIT. Dodaje menu pod prawym przyciskiem myszy na polu tekstowym, obszarze tekstu albo edytowalnej części strony, a kliknięta pozycja trafia w to pole. Jego menu mają 324 wartości w 14 grupach: imiona i nazwiska, adresy e-mail, adresy URL, liczby, kwoty, rozmiary tekstu, białe znaki, Unicode, kilka testów formatów, przykładowy tekst w kilku pismach, kody pocztowe, miasta i 88 testowych numerów kart operatorów płatności. Własne menu można dodać z pliku konfiguracji.

Liczby pochodzą z [repozytorium](https://github.com/gojko/bugmagnet), policzone w listopadzie 2026. Wersja 4.0 wyszła w listopadzie 2025, a plik z wartościami ostatnio zmieniono w grudniu 2017.

## Co Naughty Keyboard robi inaczej?

- **Wpisuje w dowolne okno.** W stronę internetową, aplikację na komputer, terminal, okno dialogowe systemu. Jeden skrót wpisuje następną wartość paczki w pole, które ma fokus klawiatury, na Windows, macOS i Linuksie, a paleta nigdy nie zabiera fokusu.
- **Bez myszy między wartościami.** Wartości paczki przychodzą w stałej kolejności, jeden skrót na każdą, więc przez paczkę przechodzi się bez otwierania menu za każdym razem. Okno wartości przeszukuje wszystkie paczki, gdy potrzebna jest jedna konkretna wartość.
- **Każda wartość mówi, co psuje.** Dwa zdania na wartość: co zwykle psuje i dlaczego oraz co zamiast tego robi poprawna aplikacja. Pozycja menu ma etykietę.
- **Pisze zgłoszenie błędu.** Jeszcze jeden skrót kopiuje blok do zgłoszenia z wartością zapisaną tak, że ktoś inny może ją wpisać z powrotem, z jej rozmiarem i informacją, czy doszła w całości.
- **Liczy to, czego nie widać.** Znak niewidoczny jest rysowany jako znacznik, a każda wartość jest liczona w grafemach, punktach kodowych, bajtach i jednostkach UTF-16.
- **Robi wartości do limitu długości.** Podaj limit pola, a zrobi wartości o jeden mniejsze, równe i o jeden większe, ze znaków, które w bajtach ważą różnie. Zobacz [tekst do testu limitu znaków](/character-limit-test-text/).
- **Działa też w skrypcie.** `nkb emit` wypisuje każdą paczkę jako JSON, CSV albo jedną wartość na wiersz, do testu automatycznego.

## Kiedy Bug Magnet wystarczy?

Gdy testujesz tylko formularze stron w Chrome albo Firefoksie, a menu prawego przycisku pasuje do Twojego sposobu pracy. Nie wymaga instalowania niczego poza przeglądarką. Ma też grupy, których Naughty Keyboard nie ma: testowe numery kart operatorów płatności, przykładowy tekst w kilku pismach, kody pocztowe i miasta kilku krajów.

## Jak się porównują?

| | Bug Magnet | Naughty Keyboard |
|---|---|---|
| Czym jest | rozszerzeniem do Chrome i Firefoksa | dwoma programami, paletą i wierszem poleceń `nkb`, nad katalogiem paczek |
| Gdzie wpisuje | w pola stron internetowych | w pole z fokusem klawiatury, w dowolnej aplikacji |
| Jak wybrać wartość | menu prawego przycisku | jeden skrót na następną wartość albo okno wartości z wyszukiwaniem |
| Wielkość | 324 wartości w 14 grupach, w tym 88 numerów kart | {{< count "value" >}} wartości w {{< count "pack" >}} paczkach |
| Co mówi o wartości | jej etykietę w menu | co psuje i co robi poprawna aplikacja |
| Własne wartości | menu z pliku konfiguracji | paczki z własnego folderu albo folderu zespołu, sprawdzane przez `nkb lint` |
| Zgłoszenie błędu | nie | blok do zgłoszenia jednym skrótem |
| W teście automatycznym | nie do tego powstał | `nkb emit`, jako JSON, CSV albo jedna wartość na wiersz |
| Licencja | MIT | GPL-3.0 dla programów, CC BY 4.0 dla paczek |

## Czy można używać obu?

Tak. Zostaw Bug Magnet w przeglądarce dla numerów kart i przykładowego tekstu, a Naughty Keyboard weź do aplikacji na komputer, do pól, które testujesz wartość po wartości, i do błędów, które chcesz zgłosić z dokładnie zapisaną wartością. [Pierwsze kroki](/docs/getting-started/) zajmują około dwóch minut.
