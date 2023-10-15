# brainfuck-extended

A Brainfuck interpreter, debugger, and compiler written in Rust.

# Interpreter/Debugger

The interpreter and debugger are within the same crate (`brainfuck-extended`). To enable the debugger,
change the `const DEBUG: bool = false;` line to `const DEBUG: bool = true;`.

Run the debugger with `cargo run --release -p brainfuck-extended -- <brainfuck_source> [input_source]`.
You can quit the debugger at any time using Q.

### Keybinds

**While paused:** (starts paused)

- `C`: Continue execution (unpause)
- `Q`: Quit program
- Any other letter or arrow key: step through current instruction

**While running:**

- `P`: Pause execution
- `Q`: Quit program
- `Up arrow`: Increase update delay
- `Down arrow`: Decrease update delay

While the debugger is running, you can control the update delay. This delay decides how many instructions need
to be executed before the debugger is redrawn. It defaults to 1/1 (i.e. every instruction re-renders), and any
greater values (1/N) will cause N instructions to be skipped before drawing. For example, 1/2 draws every other instruction, 1/3 draws every third instruction.

### Note

There is a bug somewhere in the interpreter that causes complex programs to execute incorrectly. I suspect it
exists within the loop handling code as that's been the biggest challenge. This bug does not exist in the code
generator.

# Code Generator

The code generator crate (`codegen`) transpiles Brainfuck code into a Rust crate. The only optimization it
performs does nothing to increase performance, only improving source code size.

## Usage

`cargo run --release -p codegen -- <brainfuck_source> <output_crate_dir> [-f] [--dump-ast <dump_path.json>] [--fixed-input <fixed_input>]`

- `<brainfuck_source>`: Path to the Brainfuck source code file
- `<output_crate_dir>`: The directory in which to store the generated crate (see [Generated Code Structure](#generated-code-structure))
- `[-f] | [--format]`: Enable the use of `rustfmt` for formatting the generated source code
- `[--dump-ast <dump_path.json>]`: Dump the parsed syntax tree to this JSON file
- `[--fixed-input <fixed_input>]`: Replace the stdin reading code with a fixed string. All `,` instructions will
  be forced to use this string instead of stdin

## Generated Code Structure

The `codegen` crate generates an entire crate that is self-sufficient and can be run directly. It is highly recommended
that you run the generated code in `--release` mode because development builds handle overflow differently (and of
course performance will be dramatically improved). It is very likely that a transpiled program will panic when run
in a development build. This could be fixed by wrapping the program's memory in `Wrapping<T>`.

## Running generated code

For programs that do not read any user input:
`cargo run --release --manifest-path <output_crate_dir/Cargo.toml`

For programs that do read user input:
`echo "<input>" | cargo run --release --manifest-path <output_crate_dir/Cargo.toml`

Using `echo` is not required. Any method of piping input into the program will work.
