---
name: oxie
description: Write, fix, review and explain code in Oxie, the programming language that compiles to Rust (`.oxi` files, the `oxie` command, `oxie new` projects). Use this skill for any work on `.oxi` files or Oxie projects and scripts, for translating Rust or Kotlin code to Oxie, and whenever the user mentions Oxie, even for a one-line change, because Oxie looks like Kotlin but follows Rust's rules, and its syntax changes between versions.
---

# Oxie

Oxie reads like Kotlin and compiles to plain Rust, with Rust's ownership, borrowing, traits and
crates. What you remember about it is probably incomplete, and may describe an older version:
the language changes from release to release. The `oxie` command carries a guide to its own
version, so take the rules from there, not from memory or from this file.

## Before writing Oxie

1. Run `oxie guide` and read all of it. It shows how to work with the compiler, the language
   at a glance, what works differently from Kotlin and from Rust, and the mistakes people make.
2. If it starts with a note that the project builds with another `oxie-build` version, tell
   the user: code that follows the guide may not build until the versions match, and the note
   says how to make them match.
3. Look up details as you need them: `oxie guide topics` lists the documentation pages built
   into `oxie` (the full reference, error handling, Rust interop, ...), and
   `oxie guide reference` prints one.

If `oxie guide` is an unknown command, the installed `oxie` predates the guide: ask the user to
update it (`cargo install oxie`), or work from https://oxie.bkymz.net, which describes the
latest release. If `oxie` isn't installed at all, ask before installing it.

## While writing

* Change a little, then run `oxie check`. Errors point at `.oxi` lines and explain the problem
  in Oxie terms; when a `= fix:` line says what you meant, apply it.
* `oxie emit <file.oxi> --annotate` shows the Rust a file becomes, with the reasons for what
  Oxie inferred. Use it when you aren't sure what a construct does.
* `oxie from-rust <file.rs>` translates Rust into Oxie, which helps with Rust snippets from a
  crate's documentation.
* Finish with `oxie run` and `oxie test`.
* Keep the work in `.oxi` files. Don't edit the generated Rust or the `src/main.rs` that
  includes it, and don't route around an Oxie error with Rust files or `rust { }` blocks:
  Oxie errors have Oxie fixes, and the guide shows them.
