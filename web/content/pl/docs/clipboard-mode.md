---
title: Tryb schowka
slug: tryb-schowka
seoTitle: Tryb schowka - wartości testowe do wklejenia
description: Jak paleta kładzie każdą wartość testową w schowku zamiast ją wpisywać, kiedy robi to sama i jak trzyma wartości z dala od historii schowka.
lead: "Zamiast wpisywać wartość, paleta kładzie ją w schowku, a Ty wklejasz ją tam, gdzie chcesz."
---

## Kiedy tryb schowka jest właściwy?

Gdy wpisywanie jest złą drogą do pola. Niektóre pola obsługują wklejoną wartość inaczej niż wpisaną i tę różnicę warto przetestować osobno. Niektóre aplikacje reagują na każdy klawisz, więc wpisanie długiej wartości uruchamia ich kod tysiąc razy. A w niektóre okna nie da się w ogóle pisać z zewnątrz.

## Jak go włączyć?

W palecie ustaw **Send by** na **Clipboard**. Od tej chwili każda wartość trafia do schowka i nic nie jest naciskane: wklejasz ją wklejaniem samej aplikacji. Ustaw z powrotem **Keyboard**, żeby znów wpisywać. Żeby zacząć w trybie schowka, uruchom `nkb-gui --clipboard`, opcjonalnie z nazwą paczki.

Paleta nigdy nie wkleja za Ciebie. Wklejenie to naciśnięcie klawisza w cudzym oknie, a każdy powód, żeby być w trybie schowka, jest powodem, dla którego to naciśnięcie poszłoby źle.

## Kiedy paleta sama używa schowka?

Dla okna, które działa z wyższymi uprawnieniami niż paleta, na przykład programu uruchomionego jako administrator. Windows odrzuciłby naciśnięcia bez słowa, więc paleta pyta przed wpisaniem, kładzie tę wartość w schowku i mówi dlaczego. Robi tak dalej dla tego samego okna i wraca do wpisywania, gdy przejdziesz do innego.

Na macOS i Linuksie ta wersja programu nie ma skrótów globalnych, więc wartość trafia do pola przez schowek: przyciskami Copy palety albo jej przełącznikiem schowka.

## Co jeszcze kładzie wartość w schowku?

Przycisk **Copy** przy następnej i przy ostatniej wartości kładzie tę wartość w schowku tak samo i nie przesuwa palety dalej w paczce. {{< kbd "Alt+Shift+B" >}} kopiuje blok do zgłoszenia dla ostatniej wartości. To jedyne trzy miejsca, w których paleta zapisuje do schowka, każde po Twoim naciśnięciu albo kliknięciu, a schowka nie czyta nigdy.

## Czy wartość trafia do historii schowka?

Nie. Na Windows wartość, którą paleta kładzie w schowku, jest oznaczona tak, żeby nie trafiła do historii schowka ani do schowka w chmurze, więc testowa wartość z tysiącem znaków nie zapcha historii, której używasz do wszystkiego innego. Wyjątkiem jest blok do zgłoszenia: służy do wklejenia w zgłoszenie i zostaje w historii jak wszystko, co kopiujesz samodzielnie.

## Czy nkb ma tryb schowka?

Nie. Wiersz poleceń wypisuje wartości na standardowe wyjście, skąd bierze je skrypt albo potok, a `nkb send` wpisuje wartość w pole z fokusem na Windows. Program, który kładzie coś w schowku i kończy działanie, oddałby wartość temu, co przeczyta schowek jako następne, a tego nie robi się ze skryptu.
