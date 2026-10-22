#!/bin/sh
# Regenerates the tree-sitter parser and copies it with the queries into the Kuana extension.
set -e
cd "$(dirname "$0")/tree-sitter-brs"
[ -x node_modules/.bin/tree-sitter ] || npm install --no-audit --no-fund
./node_modules/.bin/tree-sitter generate
cd ..
rm -rf kuana-brs/grammars/brs
mkdir -p kuana-brs/grammars/brs
cp -R tree-sitter-brs/src kuana-brs/grammars/brs/src
rm -f kuana-brs/grammars/brs/src/grammar.json kuana-brs/grammars/brs/src/node-types.json
for q in highlights outline indents brackets textobjects runnables injections; do
    cp "tree-sitter-brs/queries/$q.scm" "kuana-brs/languages/brs/$q.scm"
done
echo "Kuana extension updated: $(pwd)/kuana-brs"
