# Examples

Each directory is a complete brs project. Run one with `brs run` inside it (or `brs test` for
its tests). The test suite runs all of them and compares the output with `expected.txt`.

| Example | Shows |
|---|---|
| [hello](hello) | a quick tour: values, templates, collections, lambdas, null safety |
| [calculator](calculator) | a lexer, recursive-descent parser and evaluator; recursive enums, `error enum`, classes with inferred `&mut self`, `when` |
| [todo](todo) | a command-line todo list stored in a file; CLI arguments, file IO, `companion object`, `throws` + `context` |
| [bank](bank) | interfaces with trait objects, typed errors, `try`/`catch`, `impl Display` for a class |
| [game_of_life](game_of_life) | 2D grid simulation, `"""` strings, `toString()` → `Display` |
| [word_count](word_count) | reading a file, `HashMap` counting, sorting, a unit test |
| [async_tasks](async_tasks) | tokio: `JoinSet`, channels, `async fun ... throws` |
| [json_api](json_api) | serde: `@derive(Serialize, Deserialize)`, `@serde(...)` attributes, default values |
| [geometry](geometry) | a library written in brs with a `pub` API, used from a Rust integration test |
| [mixed](mixed) | a Rust program with a module written in brs that calls back into Rust |
