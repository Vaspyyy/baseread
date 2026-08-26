# BASISREAD

Experimental, dynamically typed, English-like automation and terminal game language.

The Rust prototype supports variables, output, functions, control flow, lists,
filesystem automation, shell commands, desktop-entry application launching, and
dynamic game state:

```basisread
run firefox
set name to "Ransom"
say "Hello, {name}"
create folder "backup"
copy "notes.txt" to "backup/notes.txt"
say read file "notes.txt"
shell "echo finished"
start shell "long-running-command"
include "common.basis"
set player to {health: 20, inventory: []}
set action to ask "What next? "
set roll to random integer from 1 to 6
say "{action}: {roll}"
```

Run the example after installing Rust:

```sh
cargo run -- examples/hello.basis
cargo run -- examples/dungeon.basis
cargo run -- build examples/hello.basis -o hello
cargo run -- --tokens examples/hello.basis
```
