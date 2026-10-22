#!/bin/sh
# Runs every tests/e2e/*.brs program and compares stdout with the matching .out file.
cd "$(dirname "$0")/e2e" || exit 1
BRS="${BRS:-../../target/debug/brs}"
pass=0; fail=0; failed=""
for f in ${@:-*.brs}; do
    f="${f%.brs}"
    [ -f "$f.out" ] || continue
    if "$BRS" run "$f.brs" > "/tmp/brs-e2e-$f.actual" 2> "/tmp/brs-e2e-$f.err" && diff -q "/tmp/brs-e2e-$f.actual" "$f.out" > /dev/null; then
        pass=$((pass + 1))
    else
        fail=$((fail + 1)); failed="$failed $f"
    fi
done
echo "passed: $pass, failed: $fail"
[ -n "$failed" ] && echo "failed:$failed"
[ "$fail" -eq 0 ]
