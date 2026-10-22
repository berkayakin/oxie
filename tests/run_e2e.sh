#!/bin/sh
# Runs every tests/e2e/*.oxi program and compares stdout with the matching .out file.
cd "$(dirname "$0")/e2e" || exit 1
OXIE="${OXIE:-../../target/debug/oxie}"
# build with this checkout's oxie-build, not the one on GitHub
OXIE_HOME="${OXIE_HOME:-$(cd ../.. && pwd)}"; export OXIE_HOME
pass=0; fail=0; failed=""
for f in ${@:-*.oxi}; do
    f="${f%.oxi}"
    [ -f "$f.out" ] || continue
    if "$OXIE" run "$f.oxi" > "/tmp/oxie-e2e-$f.actual" 2> "/tmp/oxie-e2e-$f.err" && diff -q "/tmp/oxie-e2e-$f.actual" "$f.out" > /dev/null; then
        pass=$((pass + 1))
    else
        fail=$((fail + 1)); failed="$failed $f"
    fi
done
echo "passed: $pass, failed: $fail"
[ -n "$failed" ] && echo "failed:$failed"
[ "$fail" -eq 0 ]
