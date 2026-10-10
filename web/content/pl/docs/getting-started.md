---
title: Pierwsze kroki
slug: pierwsze-kroki
seoTitle: Pierwsze kroki - pierwsza wartość testowa w dwie minuty
description: Uruchom paletę, wpisz pierwszą wartość testową w pole skrótem Alt+Shift+N, skopiuj blok do zgłoszenia, gdy coś się zepsuje, i używaj tych samych paczek w nkb.
lead: "Od pobrania do pierwszej kłopotliwej wartości w polu w mniej więcej dwie minuty."
---

## Czego potrzebuję?

Komputera z Windows 10 albo 11, żeby wpisywać w inne aplikacje. Na macOS i Linuksie wiersz poleceń i narzędzia do paczek działają w pełni, a paleta kładzie wartość w schowku, skąd ją wklejasz. Strona [Pobierz](/download/) ma archiwum dla każdego systemu, a nic się nie instaluje: rozpakowujesz i uruchamiasz.

## Jak wpisać pierwszą wartość?

1. Uruchom `nkb-gui`. Za pierwszym razem otwiera się okno powitalne z polem do wypróbowania pierwszej wartości, żeby pierwsza wartość nie wylądowała w cudzej aplikacji. Jego zamknięcie otwiera paletę, a paleta pamięta, że okno powitalne już było.
2. Kliknij pole, które chcesz przetestować, w dowolnej aplikacji.
3. Naciśnij {{< kbd "Alt+Shift+N" >}}. Paleta czyści wiersz i wpisuje w pole następną wartość z paczki.
4. Zobacz, co aplikacja z nią zrobiła, a potem znów naciśnij {{< kbd "Alt+Shift+N" >}}, żeby wpisać następną.

Paleta nigdy nie zabiera fokusu klawiatury, więc kursor przez cały czas zostaje w polu. Pokazuje wartość, która będzie następna, i to, jak daleko jesteś w paczce.

Okna programu mówią dziś po angielsku, więc nazwy przycisków i okien podajemy tak, jak je widać na ekranie.

## Co zrobić, gdy wartość coś zepsuje?

Naciśnij {{< kbd "Alt+Shift+B" >}}. Paleta kopiuje blok do zgłoszenia dla ostatniej wpisanej wartości: która wartość, z której wersji paczki, wpisana jako co, jak duża i czy dotarła w całości. Wklej go do zgłoszenia. Wartość jest w nim zapisana tak, żeby ktoś inny mógł ją wpisać z powrotem, a nigdy jako znacznik, którym paleta rysuje niewidoczny znak. Blok jest zawsze po angielsku. Ten sam blok stoi pod **Report block**, gdy rozwiniesz w palecie **Last sent**, a obok niego jest przycisk **Copy**.

## Czy mogę wybrać inną paczkę?

Naciśnij {{< kbd "Alt+Shift+Space" >}} albo kliknij nazwę paczki na górze palety. Okno, które się otworzy, przeszukuje naraz wartości wszystkich paczek, więc wpisanie `pesel` znajduje polski identyfikator, a wpisanie `date` niemożliwe daty. Strona [Paczki](/packs/) pokazuje każdą z nich z każdą wartością.

## Jak użyć tych samych wartości w skrypcie?

Wiersz poleceń czyta te same paczki. Wypisz je, pokaż jedną albo wypisz jej wartości w formacie, który test potrafi wczytać:

```console
$ nkb packs
$ nkb show whitespace
$ nkb emit magic-values --format lines > magic-values.txt
```

[Wiersz poleceń](/docs/cli/) opisuje każde polecenie i każdą opcję.

## Czy mogę bezpiecznie skierować go na swoją aplikację?

Pisze w oknie, na które patrzysz, bo to jest cały produkt, i w nic więcej. Nie otwiera połączeń sieciowych, nie czyta tekstu z żadnego okna i dotyka schowka tylko na Twoją prośbę. Strona [Bezpieczeństwo](/security/) mówi, co czyta, co zapisuje i jak to sprawdzić. Używaj go na systemach, które wolno Ci testować.
