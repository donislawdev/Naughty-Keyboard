---
title: Wiersz poleceń
seoTitle: Wiersz poleceń nkb - każde polecenie i każda opcja
description: Wiersz poleceń nkb wypisuje, pokazuje i sprawdza paczki wartości testowych i wpisuje jedną z nich w pole. Każde polecenie, każda opcja i każdy kod wyjścia.
lead: "`nkb` wypisuje, pokazuje i sprawdza paczki, a na Windows wpisuje jedną wartość w pole z fokusem. Działa w terminalu, w skrypcie i w pipeline CI."
layout: cli
---

`nkb` to osobny program, niezależny od palety, i nie łączy się z żadną biblioteką graficzną, więc działa na serwerze buildów bez żadnego pulpitu. Wszystko, co wypisuje dla człowieka, idzie na standardowe wyjście błędów, a wszystko, co bierze skrypt, na standardowe wyjście, więc `nkb emit pack > plik` zostawia plik z samymi wartościami.

Każde polecenie odpowiada na `--help` składnią, opcjami i opisem tego, co robi. Strony niżej są zbudowane z tych odpowiedzi, słowo w słowo, przez własny test programu, więc nie mogą się rozjechać z programem, który uruchamiasz. Program mówi po angielsku, więc pomoc i wyjście w przykładach są po angielsku, a opisy opcji obok - po polsku.
