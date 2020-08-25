
Raw `Engine`
===========

{{#include ../links.md}}

`Engine::new` creates a scripting [`Engine`] with common functionalities (e.g. printing to the console via `print`).

In many controlled embedded environments, however, these may not be needed and unnecessarily occupy
application code storage space.

Use `Engine::new_raw` to create a _raw_ `Engine`, in which only a minimal set of
basic arithmetic and logical operators are supported (see below).

To add more functionalities to a _raw_ `Engine`, load [packages] into it.


Built-in Operators
------------------

| Operators                | Assignment operators         | Supported for types (see [standard types])                                                                      |
| ------------------------ | ---------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `+`,                     | `+=`                         | `INT`, `FLOAT` (if not [`no_float`]), `Decimal` (if [`decimal`]) ,`ImmutableString`                      |
| `-`, `*`, `/`, `%`, `~`, | `-=`, `*=`, `/=`, `%=`, `~=` | `INT`, `FLOAT` (if not [`no_float`]), `Decimal` (if [`decimal`])                                          |
| `<<`, `>>`, `^`,         | `<<=`, `>>=`, `^=`           | `INT`                                                                                                           |
| `&`, <code>\|</code>,    | `&=`, <code>\|=</code>       | `INT`, `bool`                                                                                                   |
| `&&`, <code>\|\|</code>  |                              | `bool`                                                                                                          |
| `==`, `!=`               |                              | `INT`, `FLOAT` (if not [`no_float`]),`Decimal` (if [`decimal`]), `bool`, `char`, `()`, `ImmutableString` |
| `>`, `>=`, `<`, `<=`     |                              | `INT`, `FLOAT` (if not [`no_float`]),`Decimal` (if [`decimal`]), `char`, `()`, `ImmutableString`         |
