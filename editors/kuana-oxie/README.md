# Oxie for Kuana

Support for the [Oxie programming language](../../README.md) in the Kuana editor:

* syntax highlighting, outline, breadcrumbs and sticky scroll, bracket matching, indentation
  and text objects (tree-sitter grammar from `../tree-sitter-oxie`);
* the Oxie language server: diagnostics (Oxie and rustc errors mapped to `.oxi` lines),
  completion, hover with the generated Rust, go to definition, references, rename, signature
  help, formatting, inlay hints;
* ▶ run buttons on `fun main` and on each `test "..."` block;
* snippets.

## Install

1. Install the `oxie` command (the language server is `oxie lsp`):

   ```bash
   cargo install oxie
   ```

2. In Kuana: Settings → Extensions → **Install from Folder…** and choose this folder
   (or link it: `ln -s "$PWD" ~/.config/kuana/extensions/oxie`, then **Reload Extensions**).

Kuana compiles the bundled grammar (`grammars/oxie/src/parser.c`) the first time; validate
the extension with `kuana extension check editors/kuana-oxie`.

## Updating the grammar

Edit `../tree-sitter-oxie/grammar.js` or its `queries/`, then run
`../build-kuana-extension.sh` and **Reload Extensions** in Kuana.
