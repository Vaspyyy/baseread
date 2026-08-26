# BASISREAD

Experimental, dynamically typed, English-like automation language.

The Rust prototype supports variables, output, functions, control flow, lists,
filesystem automation, shell commands, and desktop-entry application launching:

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
```

Run the example after installing Rust:

```sh
cargo run -- examples/hello.basis
cargo run -- build examples/hello.basis -o hello
cargo run -- --tokens examples/hello.basis
```
