# Oxie for Visual Studio Code

Support for [Oxie](https://github.com/oxie-lang/oxie), a programming language with a concise,
Kotlin/TypeScript-like syntax that compiles to plain Rust.

## Features

* **Syntax highlighting**, including string templates (`"$name ${a + b}"`), `rust { ... }`
  blocks highlighted as Rust, and ` ```oxie ` code blocks in Markdown.
* **The Oxie language server** (`oxie lsp`):
  * Oxie errors as you type; after `cargo check` on open and save, rustc's errors and warnings
    on the `.oxi` lines, explained in Oxie terms, with quick fixes;
  * completion, hover with the generated Rust signature, go to definition, references, rename,
    signature help, document and workspace symbols, formatting (`oxie fmt`);
  * inlay hints that show how each method receives its object (`&self`, `&mut self`, static).
* **Run and test**: ▶ Run and Run --release above `fun main`, ▶ Run Test above each
  `test "..."` block, and Run File, Run File (Release) and Run Tests in File in the editor's run
  menu. Programs run as tasks in the terminal.
* **Show Generated Rust** (the preview button in the editor title): the Rust code for the
  current file beside it, updated as you type.
* **Tasks** for Oxie projects: `oxie build`, `run`, `test`, `check` and `fmt` under
  Terminal → Run Task. In `tasks.json`, use `{"type": "oxie", "command": "build", "args": []}`.
* **Snippets**: `main`, `fun`, `class`, `data`, `enum`, `errenum`, `interface`, `when`, `for`,
  `test`, ...

## Requirements

Oxie needs [Rust](https://rustup.rs) and the `oxie` command:

```bash
cargo install oxie
```

If `oxie` isn't on the `PATH` (or in `~/.cargo/bin`), the extension offers to install it. Set
`oxie.path` to use another build, such as a checkout of the Oxie repository
(`target/debug/oxie`).

## Settings

| Setting | Default | |
|---|---|---|
| `oxie.path` | `""` | The `oxie` command for the language server and for running files |
| `oxie.cargoCheck` | `true` | Run `cargo check` on open and save to show rustc's errors |
| `oxie.trace.server` | `"off"` | Log the language server messages (`messages`, `verbose`) |

## Restricted Mode

In an untrusted workspace only highlighting and snippets work: the language server and the run
commands build the code, which runs its build scripts.

## Development

The extension lives in `editors/vscode-oxie` of the Oxie repository.

```bash
npm install
npm run build      # bundles src/ into out/extension.js
npm test           # grammar tests, then integration tests in VS Code
npm run package    # writes oxie-<version>.vsix
```

Open this folder in VS Code and press F5 to try the extension in a new window. The integration
tests download VS Code unless `VSCODE_EXECUTABLE` points to one, and use the `oxie` on the
`PATH` unless `OXIE_PATH` is set. Grammar tests are in `test/grammar`, in the
[vscode-tmgrammar-test](https://github.com/PanAeon/vscode-tmgrammar-test) format.
