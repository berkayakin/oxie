//! `oxie guide`: how to write Oxie, and the documentation of this version. Both are built into
//! the binary, so what an AI agent (or a person) reads matches the compiler it runs. Projects
//! and agent setups only point here, instead of copying rules that a newer Oxie would change:
//! the `AGENTS.md` of every project made so far and the Claude Code skill run `oxie guide`, so
//! the command and its `topics` must keep working in every later version.

use std::process::ExitCode;

use crate::project::{self, Project};

/// What `oxie guide` prints: the workflow, the language at a glance, how it differs from Kotlin
/// and Rust, and the mistakes to avoid.
pub const GUIDE: &str = include_str!("../guide/writing-oxie.md");

/// `oxie guide <topic>`: the pages of `docs/`. A published crate can only include its own
/// files, so they are copies in `guide/docs/`, which the e2e tests keep equal to `docs/`.
pub const TOPICS: &[(&str, &str, &str)] = &[
    ("tour", "the language by example", include_str!("../guide/docs/guide.md")),
    ("reference", "every construct and the Rust it compiles to", include_str!("../guide/docs/reference.md")),
    ("errors", "error handling in depth", include_str!("../guide/docs/errors.md")),
    ("cheatsheet", "Kotlin and TypeScript next to Oxie", include_str!("../guide/docs/cheatsheet.md")),
    ("interop", "crates, Cargo, and mixing Oxie with Rust", include_str!("../guide/docs/rust-interop.md")),
    ("tooling", "the oxie command, its diagnostics, scripts and editors", include_str!("../guide/docs/tooling.md")),
    ("performance", "what Oxie code costs", include_str!("../guide/docs/performance.md")),
];

pub fn run(args: &[String]) -> Result<ExitCode, String> {
    let version = env!("CARGO_PKG_VERSION");
    match args.first().map(String::as_str) {
        None => {
            println!("This is the guide to oxie {version}, the Oxie installed here.\n");
            let here = std::env::current_dir().ok().and_then(|d| Project::find(&d));
            if let Some(warning) = here.as_ref().and_then(project::version_warning) {
                println!("Note: {warning}.\n");
            }
            print!("{GUIDE}");
        }
        Some("topics") => {
            for (name, about, _) in TOPICS {
                println!("{name:<12} {about}");
            }
        }
        Some(topic) => {
            let (_, _, text) = TOPICS.iter().find(|(name, ..)| *name == topic).ok_or_else(|| {
                let names: Vec<&str> = TOPICS.iter().map(|(name, ..)| *name).collect();
                format!("there is no guide topic `{topic}`; the topics are {}", names.join(", "))
            })?;
            print!("{text}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::{GUIDE, TOPICS};

    #[test]
    fn the_guide_lists_every_topic() {
        for (name, ..) in TOPICS {
            assert!(GUIDE.contains(&format!("* `{name}` — ")), "guide/writing-oxie.md doesn't list the topic `{name}`");
        }
    }
}
