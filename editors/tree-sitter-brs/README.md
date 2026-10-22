# tree-sitter-brs

A [tree-sitter](https://tree-sitter.github.io) grammar for brs, made for editors: it recognizes
declarations, calls, member access and strings with interpolation and groups everything else
by brackets, so it stays small, fast and tolerant of incomplete code.

```bash
npm install          # installs the tree-sitter CLI
npx tree-sitter generate
npx tree-sitter parse ../../examples/calculator/src/parser.brs
```

Queries for highlighting, outline (Zed/Kuana format), indentation, brackets, text objects,
runnables and injections are in `queries/`.
