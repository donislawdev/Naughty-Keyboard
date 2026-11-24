---
title: Imiona i nazwiska ze znakami specjalnymi do testów
url: /pl/poradniki/imiona-i-nazwiska-do-testow/
seoTitle: Imiona i nazwiska ze znakami specjalnymi do testów
description: Prawdziwe imiona i nazwiska, które psują pola formularza - apostrof, łącznik, dwa nazwiska, jedno imię, nazwisko Null, litery spoza ASCII - i co każde psuje.
lead: "Pole na imię i nazwisko buduje się zwykle pod imię i nazwisko ze zwykłych liter. Prawdziwe nazwiska takie nie są. Oto nazwiska, którymi warto je przetestować, każde z tym, co zwykle psuje."
---

## Jakimi nazwiskami testować pole na imię i nazwisko?

Takimi, jakie mają prawdziwi ludzie, a których formularz się nie spodziewał: z apostrofem, z łącznikiem, z dwoma nazwiskami, z jednym imieniem, z literami spoza ASCII i takimi, które program czyta jako wartość. Żadne z nich nie jest podstępem. Pole, które któreś odrzuca albo zmienia, odrzuca prawdziwego człowieka, zwykle w dniu, w którym próbuje się zarejestrować.

## Co sprawdzić po każdym nazwisku?

Za każdym razem te same cztery rzeczy:

1. **Czy zapisało się i wyświetla tak, jak je wpisano?** Otwórz rekord jeszcze raz i zobacz nazwisko na liście i w e-mailu, który wysyła aplikacja.
2. **Czy wyszukiwanie je znajduje?** Wpisane tak samo, a także bez apostrofu albo bez polskiej litery.
3. **Czy powitanie jest poprawne?** „Szanowny Panie O'Brien” złożone z pola, a nie „Szanowny Panie O” albo „Szanowny Panie Obrien”.
4. **Czy przetrwa następny system?** Eksport, fakturę, etykietę kuriera.

## Apostrofy, łączniki i dwa nazwiska

Przetestuj znaki wewnątrz prawdziwych nazwisk. Apostrof to najczęstsze prawdziwe nazwisko, które psuje formularze, a spacja albo łącznik w nazwisku psuje każdą regułę, która dzieli nazwisko na słowa.

{{< values "names-identity/apostrophe-surname" "names-identity/hyphen-first-name" "names-identity/two-surnames" "names-identity/particle-prefix" >}}

## Nazwiska, które program czyta jako wartość

Przetestuj nazwiska, które są też słowami zrozumiałymi dla programu. Null i True to prawdziwe nazwiska, a warstwa, która traktuje tekst luźno, zamienia je na „nic” i „tak”.

{{< values "names-identity/surname-null" "names-identity/surname-true" >}}

## Jedno imię, jedna litera, cyfra, tytuł

Przetestuj nazwiska, które łamią kształt założony przez formularz: człowieka z jednym imieniem, imię z jednej litery, cyfrę w nazwisku i tytuł wpisany w pole nazwiska.

{{< values "names-identity/single-name" "names-identity/single-letter-name" "names-identity/digits-in-name" "names-identity/title-inside" >}}

## Litery spoza ASCII

Przetestuj litery, które Twoi użytkownicy mają w nazwiskach, i dwie, które wyglądają tak samo, a zapisują się inaczej. Kodowanie węższe niż pełny zestaw znaków zamienia je przy zapisie w znaki zapytania.

{{< values "locale-pl/diacritics-full" "locale-pl/surname-hyphenated" "look-alike-pairs/cyrillic-small-a" "unicode-text/emoji-in-name" >}}

## Wielkie litery, długość i spacje

Przetestuj nazwisko zapisane wielkimi literami, jak w dokumentach, nazwisko dłuższe niż kolumna i układ, pod które je zaprojektowano, oraz spacje, które przychodzą z nazwiskiem skopiowanym skądinąd.

{{< values "names-identity/all-caps-surname" "names-identity/very-long-name" "look-alike-pairs/nbsp-name" "whitespace/trailing-space" >}}

## Jak szybko przejść przez wszystkie?

Paletą, jeden skrót na nazwisko: wybierz paczkę **Imiona, nazwiska i tożsamość**, kliknij pole, naciśnij {{< kbd "Alt+Shift+N" >}}, spójrz na wynik i naciśnij go jeszcze raz dla następnego. Gdy nazwisko coś psuje, {{< kbd "Alt+Shift+B" >}} kopiuje blok do zgłoszenia. [Pierwsze kroki](/docs/getting-started/) zajmują około dwóch minut.

Do testu automatycznego `nkb emit` wypisuje paczkę jako JSON, CSV albo jedną wartość na wiersz:

```console
$ nkb emit names-identity --format json > names.json
```

Każde nazwisko jest na [stronie paczki](/packs/names-identity/), razem z tym, co robi poprawna aplikacja. Dla pola e-mail zobacz [przypadki testowe pola e-mail](/email-field-test-cases/).
