---
title: Szablony paczek
slug: szablony-paczek
seoTitle: Szablony paczek - wartości testowe do pobrania i zmiany
description: "Cztery paczki wartości testowych do pobrania i przerobienia na własne: pierwsza paczka, pole z limitem długości, formularz rejestracji i daty rezerwacji."
lead: "Cztery paczki do pobrania i przerobienia na własne. Każda przechodzi `nkb lint` bez błędu i bez ostrzeżenia, a każda pokazuje inną część formatu."
---

## Jak użyć szablonu?

Pobierz go do własnego folderu, a paleta od razu go pokaże. W palecie naciśnij {{< kbd "Alt+Shift+Space" >}}, kliknij **Pack folders**, potem **Create and open**, żeby otworzyć ten folder. Potem przerób szablon na własną paczkę: nadaj mu własne `id`, zmień tak samo nazwę pliku i zastąp wartości swoimi. [Własna paczka](/docs/write-a-pack/) objaśnia każdy wiersz.

Każdy szablon działa od razu, więc można wypróbować go w palecie, zanim cokolwiek się w nim zmieni. Szablony są po angielsku, a własną paczkę można pisać [po polsku](/docs/write-a-pack/#czy-mogę-napisać-paczkę-po-polsku).

## Pierwsza paczka

Trzy wartości dla pola z nazwiskiem: spacja na końcu, apostrof i pusta wartość. Najkrótsza paczka na start, z komentarzem przy wierszach, które są ważne za pierwszym razem.

{{< template "first-pack" >}}

## Pole z limitem długości

Wartości na granicy limitu 50 znaków: jeden znak poniżej, dokładnie limit i jeden znak ponad, a do tego ta sama długość w polskich literach, w emoji, w spacjach i w literach z osobnym akcentem, które inaczej liczą się w bajtach, w punktach kodowych i w jednostkach, które liczy przeglądarka. Zmień 50 na limit swojego pola, w każdym `count` i w każdej nazwie. Pokazuje, jak `type = "repeat"` zapisuje długą wartość w kilku wierszach.

{{< template "text-field-limit" >}}

## Formularz rejestracji

Pole na adres e-mail z drugim polem, które go powtarza, i login, który ma być unikalny. Pokazuje dwie pary: ten sam adres ze spacją na końcu i bez niej oraz dwa loginy, które wyglądają tak samo, a są zapisane w dwóch alfabetach, drugi z cyrylicką literą zapisaną jako sekwencja ucieczki.

{{< template "sign-up-form" >}}

## Daty rezerwacji

Zameldowanie o godzinie, którą zmiana czasu pomija albo powtarza, pobyt, który kończy się następnego ranka, i data daleko w przyszłości. Pokazuje `type = "next-date"`, datę zapisaną jako reguła, więc nigdy nie przechodzi w przeszłość, i parę z początkiem i końcem.

{{< template "booking-dates" >}}
