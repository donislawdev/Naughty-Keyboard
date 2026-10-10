+++
title = "Naughty Keyboard: dane testowe, które psują formularze"
description = "Wpisuj kłopotliwe wartości testowe w dowolne pole jednym skrótem: niewidoczne spacje, niemożliwe daty, formuły, 65 535 znaków. Za darmo i offline."

limits = "**Gdzie działa dziś.** Wpisywanie w inne okna działa na Windows. Na macOS i Linuksie działają wiersz poleceń i narzędzia do paczek, a wartość trafia do pola przez schowek."

[hero]
eyebrow = "GUI + CLI"
title = "Twój formularz działa."
turn = "Dopóki ktoś nie wpisze tego."
lead = "**Naughty Keyboard** jednym skrótem wpisuje następną kłopotliwą wartość w pole, w którym stoi kursor. {values} wartości w {packs} paczkach, a każda mówi, co zwykle psuje i co zamiast tego robi poprawna aplikacja."
facts = [
  "Darmowy i otwarty, GPL-3.0",
  "Bez telemetrii i bez połączeń sieciowych",
  "Wpisuje na Windows, CLI i schowek na macOS i Linuksie",
]
chord_says = "wpisuje następną wartość"
rows_label = "Cztery wartości i to, co robią z formularzem"
rows = [
  { ref = "magic-values/word-null", says = "wraca puste" },
  { ref = "export-breakers/formula-equals", says = "eksportuje się jako formuła" },
  { ref = "dates-impossible/feb-30", says = "przeskakuje na 2 marca" },
  { ref = "whitespace/trailing-space", says = "konta nie znaleziono" },
]

[catalogue]
eyebrow = "Katalog"
title = "Wartości, których Twój formularz jeszcze nie widział"
text = "Nie są sprytne ani tajne. Produkują je prawdziwi użytkownicy i prawdziwe dane, a nikt nie wpisuje ich ręcznie, bo to żmudne."
examples = [
  "whitespace/trailing-space",
  "export-breakers/formula-equals",
  "magic-values/bool-no",
  "dates-impossible/feb-30",
  "whitespace/zwsp-only",
  "length-bombs/len-65535",
]

[steps]
eyebrow = "Jak to działa"
title = "Trzy kroki, a kursor nie opuszcza pola"
items = [
  { title = "Kliknij pole", text = "W dowolnej aplikacji na Windows: w przeglądarce, w programie na komputer, w terminalu." },
  { title = "Naciśnij", chord = "Alt+Shift+N", text = "Paleta czyści wiersz i wpisuje następną wartość z paczki. Nigdy nie zabiera fokusu klawiatury, więc kursor zostaje tam, gdzie był." },
  { title = "Spójrz na wynik", text = "Gdy wartość coś zepsuje, [[Alt+Shift+B]] kopiuje blok do zgłoszenia: która wartość, wpisana jako co, jak duża i czy dotarła w całości." },
]

[packs]
eyebrow = "Paczki: {packs}, wartości: {values}"
title = "Wybierz paczkę według tego, co testujesz"
text = "Każda wartość mówi, co zwykle psuje i co robi poprawna aplikacja. Paczek można używać za darmo, na licencji CC BY 4.0."

[cli]
eyebrow = "Wiersz poleceń"
title = "Także dla skryptów"
text = "`nkb` wypisuje każdą paczkę jako JSON, CSV albo jedną wartość na wiersz, w postaci escapowanej albo surowej. Wartości idą na standardowe wyjście, a opis tego, co wyszło, na standardowe wyjście błędów, więc przekierowany plik zawiera wartości i nic więcej. Każde polecenie ma kody wyjścia, według których pipeline może wybrać dalszą drogę."
link = "Każde polecenie i każda opcja"
terminal_title = "nkb emit"
terminal = """
$ nkb emit magic-values --format lines > magic-values.txt
nkb emit: 12 values, 47 code points, 47 bytes
$ head -4 magic-values.txt
no
null
true
NaN"""

[promises]
eyebrow = "Czego nigdy nie robi"
title = "Narzędziu, które pisze w cudzych oknach, trzeba móc zaufać"
text = "Każda z tych rzeczy to obietnica, a większości pilnuje test w repozytorium, który zatrzymuje budowanie, gdy obietnica zostanie złamana."
link = "Co czyta, co zapisuje i jak to sprawdzić"
items = [
  { title = "Bez połączeń sieciowych", text = "Bez telemetrii, bez sprawdzania aktualizacji, nic nie jest pobierane w trakcie działania." },
  { title = "Nie czyta zawartości okien", text = "Tylko rodzaj kontrolki z fokusem i nazwę pliku jej programu. Nigdy tytułu, tekstu ani wartości." },
  { title = "Schowek tylko na Twoją prośbę", text = "Blok do zgłoszenia, tryb schowka i przyciski Copy, każde po naciśnięciu albo kliknięciu. Nigdy nie wkleja za Ciebie i nigdy nie czyta schowka." },
  { title = "Bez haka klawiatury", text = "Skróty rejestruje systemowym wywołaniem do skrótów globalnych, które mówi mu tylko o jego własnych kombinacjach." },
  { title = "Bez uprawnień administratora", text = "Nie prosi o nie i sam nie podnosi swoich uprawnień." },
  { title = "Paczka jest cała albo jej nie ma", text = "Plik paczki jest sprawdzany przed odczytem i odrzucany przy każdym błędzie." },
]

[cta]
title = "Znajdź to, zanim znajdą to Twoi użytkownicy"
text = "Za darmo, bez konta, bez telemetrii. Paleta i wiersz poleceń dla Windows, macOS i Linuksa."
+++
