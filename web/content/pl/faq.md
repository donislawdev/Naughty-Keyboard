---
title: Pytania
seoTitle: FAQ - dane testowe, Big List of Naughty Strings, skróty
description: Odpowiedzi o Naughty Keyboard - czym różni się od Big List of Naughty Strings, czy jest darmowy, czego potrzebuje, na jakich systemach działa i nie tylko.
lead: "Pytania, które ludzie zadają najpierw, z odpowiedzią w pierwszym zdaniu."
layout: faq
faq:
  - question: Czym jest Naughty Keyboard?
    answer: "Narzędziem dla testerów i programistów, które jednym skrótem wpisuje następną kłopotliwą wartość w pole, w którym stoi kursor: spację na końcu, spację o zerowej szerokości, `30.02.2026`, `=1+1`, imię długości 65 535 znaków. Każda wartość mówi, co zwykle psuje i co robi poprawna aplikacja. To okno, paleta, i wiersz poleceń, `nkb`, nad jednym silnikiem."
  - question: Czym różni się od Big List of Naughty Strings?
    answer: "Tamta lista to zwykły plik z napisami pod nagłówkami, i to dobry plik. Naughty Keyboard robi resztę pracy: wpisuje wartość w pole za Ciebie, po jednej na naciśnięcie i w stałej kolejności, mówi, co każda wartość zwykle psuje i co robi poprawna aplikacja, pokazuje niewidoczny znak jako znacznik, liczy wartość na cztery sposoby i zamienia znalezisko w blok do zgłoszenia. Jeśli potrzebujesz listy, `nkb emit` wypisze w tej postaci każdą paczkę. Więcej mówi [porównanie](/big-list-of-naughty-strings/)."
  - question: Czy jest darmowy?
    answer: "Tak. Program jest na licencji GPL-3.0-only, a wbudowane paczki na CC BY 4.0, co każda paczka podaje we własnym polu licencji. Nie ma konta ani płatnej wersji."
  - question: Czy jest po polsku?
    answer: "Jeszcze nie. Okna palety mówią dziś po angielsku, a wiersz poleceń jest po angielsku z założenia, bo jego komunikaty czytają też skrypty. Dane mogą być w dowolnym języku: paczka `locale-pl` ma polskie identyfikatory, formaty i znaki. Ta dokumentacja podaje nazwy przycisków i okien tak, jak je widać w programie."
  - question: Czy potrzebuje internetu?
    answer: "Nie. Nie otwiera żadnego połączenia sieciowego, nie ma konta i niczego nigdzie nie wysyła. Jak to jest sprawdzane, mówi strona [Bezpieczeństwo](/security/)."
  - question: Na jakich systemach działa?
    answer: "Windows 10 i 11, macOS 11 lub nowszy na Apple silicon i Linux na x64. Wpisywanie w inne okna działa na Windows. Na macOS i Linuksie wiersz poleceń działa w pełni, a paleta kładzie wartości w schowku, skąd je wklejasz. Szczegóły mają [Uczciwe ograniczenia](/docs/limits/)."
  - question: Czy mogę użyć wartości w testach automatycznych?
    answer: "Tak. `nkb emit` wypisuje każdą paczkę jako JSON, CSV albo jedną wartość na wiersz, w postaci escapowanej albo surowej, a każde polecenie ma kody wyjścia, według których pipeline może wybrać dalszą drogę. Jak, pokazuje [nkb emit](/docs/cli/emit/)."
  - question: Czy mogę zmienić skróty?
    answer: "Tak, każdy z dziesięciu, w oknie skrótów albo w `settings.toml`. Kombinacja, która zabrałaby klawisz wszystkim aplikacjom, zostaje odrzucona z podaniem przyczyny. Listę mają [Skróty](/docs/shortcuts/)."
  - question: Czy mogę dodać własne wartości?
    answer: "Własną paczkę możesz już dziś napisać i sprawdzić poleceniami `nkb new-pack`, `nkb lint` i `nkb fmt`. Wczytywanie własnych paczek do palety nie jest jeszcze podłączone. Wartość, która kiedyś kosztowała Cię popołudnie, należy do katalogu, a formularz [Suggest a value](https://github.com/donislawdev/Naughty-Keyboard/issues/new?template=suggest_value.yml) pyta po angielsku o wartość, o to, co psuje, i o to, gdzie ją widziano."
  - question: Czy to narzędzie do ataków?
    answer: "Nie. Jest zbudowane do testowania oprogramowania, za które odpowiadasz, i mówi to okno powitalne: używaj go tylko na systemach, które wolno Ci testować. Wbudowane paczki zawierają wartości, które produkują prawdziwi użytkownicy i prawdziwe dane, a nie gotowe exploity."
  - question: Dlaczego wpisuje, a nie wkleja?
    answer: "Bo pole spotyka większość wartości z klawiatury, a niektóre aplikacje traktują wpisywanie i wklejanie inaczej, co warto przetestować osobno. Gdy chcesz drugiej drogi, [tryb schowka](/docs/clipboard-mode/) kładzie każdą wartość w schowku."
  - question: Skąd nazwa Naughty?
    answer: "Od Big List of Naughty Strings, od której zaczyna się pomysł tego katalogu i którą zna większość testerów. To ich słowo."
---
