# zst
Source code for the Z# programming language transpiler.

## `echo` / `eecho`
`echo a, b;` w Z# (spacje między argumentami + nowa linia) jest transpilowane do `__zs_echo(a, b);` — pomocnik PHP wstawiany w pierwszej linii pliku
(numeracja linii zostaje zachowana) odtwarza formatowanie Z# (`true`/`false`/`null`, floaty z `.0`, tablice). `eecho(...)` → `__zs_eecho(...)` (stderr).
