# BASISREAD language guide

BASISREAD is a dynamically typed, English-like scripting language for Linux
automation and terminal games. Blocks always end with `end`.

## Running a program

```sh
cargo run -- examples/hello.basis
cargo run -- --check examples/hello.basis
cargo run -- build examples/hello.basis -o hello
```

The default runner is interactive, so programs can use `ask` and terminal
controls. `--tokens` prints the lexer output when debugging syntax.

## Values and variables

Strings use quotes. Numbers and booleans do not.

```basisread
set name to "Ransom"
set health to 20
set alive to true
set nothing_here to nothing

say name
say health plus 5
say alive
```

Text supports interpolation:

```basisread
say "{name} has {health} health."
```

Supported values are text, numbers, booleans, lists, objects, and `nothing`.

## Lists and objects

```basisread
set inventory to ["torch", "key"]
add "potion" to inventory
remove "key" from inventory
say length of inventory

set player to {
    health: 20,
    gold: 5,
    inventory: []
}
say player.health
set player.health to player.health minus 2
add "torch" to player.inventory
say "Gold: {player.gold}"
```

Objects are dynamic maps. Define a field in the object before assigning to a
nested property. Lists and objects can be inspected with `at`:

```basisread
say inventory at 0
say player at "health"
```

## Conditions

```basisread
when health is greater than 0, do
    say "alive"
else, do
    say "dead"
end
```

Comparisons include `is`, `is not`, `is greater than`, `is less than`,
`contains`, `starts with`, and `ends with`. Conditions can use `and`, `or`,
and `not`.

## Loops

```basisread
repeat 3 times, do
    say "again"
end

while health is greater than 0
    set health to health minus 1
end

for each item in inventory, do
    say item
end
```

`stop` leaves the current loop. `skip` continues with the next iteration.

## Functions

```basisread
define greet using person, do
    return "Hello, " joined with person
end

say greet using "Ransom"
```

Functions can read global values. A function with no parameters can be used as
an expression by its name.

## Input and randomness

```basisread
set action to ask "Explore, rest, or inventory? "
set key to ask key
set roll to random integer from 1 to 6
set chance to random number from 0 to 1
set direction to random choice from ["north", "south", "east", "west"]
```

Use `set random seed to 42` when repeatable randomness is useful for tests or
replays.

## Files and saved state

```basisread
write "hello" to file "notes.txt"
append " again" to file "notes.txt"
say read file "notes.txt"

set game to {health: 18, gold: 3, inventory: ["key"]}
save game to file "save.json"
set game to load file "save.json"
say game.health
```

Other filesystem expressions include `file exists`, `folder exists`, `list
files in`, and `list folders in`.

## Terminal controls

These commands are useful for terminal games and ordinary scripts:

```basisread
clear terminal
say "You are poisoned." in red
move cursor to 10, 3
hide cursor
show cursor
say terminal width
say terminal height
wait 0.5 seconds
```

Colors include black, red, green, yellow, blue, magenta, cyan, white, gray,
and their bright variants. ANSI controls are emitted during interactive runs;
buffered library runs keep their output plain.

## Recovering from errors

```basisread
try, do
    set save to load file "missing-save.json"
else, do
    say "No save found. Starting a new game."
end
```

## Application automation

`run` resolves a Linux `.desktop` entry by its visible name, ID, or filename.
Matching ignores capitalization and tolerates small spelling mistakes.

```basisread
run firefox
run firefox with "--private-window"
```

## Complete example

See [`examples/dungeon.basis`](../examples/dungeon.basis) for a small
turn-based dungeon loop using objects, nested assignment, input, randomness,
functions, conditions, and terminal clearing.
