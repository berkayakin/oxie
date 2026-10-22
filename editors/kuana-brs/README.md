# brs for Kuana

Language support for [brs](../../README.md) in the Kuana editor:

* syntax highlighting, outline, breadcrumbs and sticky scroll, bracket matching, indentation
  and text objects (tree-sitter grammar from `../tree-sitter-brs`);
* the brs language server: diagnostics (brs and rustc errors mapped to `.brs` lines),
  completion, hover with the generated Rust, go to definition, references, rename, signature
  help, formatting, inlay hints;
* ▶ run buttons on `fun main` and on each `test "..."` block;
* snippets.

## Install

1. Install the `brs` command (the language server is `brs lsp`):

   ```bash
   cargo install --path crates/brs-cli
   ```

2. In Kuana: Settings → Extensions → **Install from Folder…** and choose this folder
   (or link it: `ln -s "$PWD" ~/.config/kuana/extensions/brs`, then **Reload Extensions**).

Kuana compiles the bundled grammar (`grammars/brs/src/parser.c`) the first time; validate
the extension with `kuana extension check editors/kuana-brs`.

## Updating the grammar

Edit `../tree-sitter-brs/grammar.js` or its `queries/`, then run
`../build-kuana-extension.sh` and **Reload Extensions** in Kuana.
