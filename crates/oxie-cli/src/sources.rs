//! The `.oxi` files of the crate a rustc error comes from, for explanations that look beyond
//! the error's own file: the declaration of the function it calls, of a type it names.

use std::cell::OnceCell;
use std::path::{Path, PathBuf};

use oxie_compiler::ast::File;
use oxie_compiler::compile::canonicalize;

use crate::project::Project;

pub struct SourceFile {
    pub path: PathBuf,
    pub text: String,
    ast: OnceCell<File>,
}

impl SourceFile {
    pub fn new(path: PathBuf, text: String) -> SourceFile {
        SourceFile { path, text, ast: OnceCell::new() }
    }

    pub fn lines(&self) -> Vec<&str> {
        self.text.lines().collect()
    }

    /// The parsed file, parsed on first use.
    pub fn ast(&self) -> &File {
        self.ast.get_or_init(|| oxie_compiler::parser::parse_file(&self.text).0)
    }
}

pub struct Sources {
    pub files: Vec<SourceFile>,
}

impl Sources {
    /// Every `.oxi` file of the project `file` belongs to, or only `file` for a script.
    pub fn of(file: &Path) -> Sources {
        let file = canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
        let project = file.parent().and_then(Project::find).filter(|p| file.starts_with(canonicalize(p.src_dir()).unwrap_or_else(|_| p.src_dir())));
        let mut files: Vec<SourceFile> = project
            .and_then(|p| oxie_compiler::discover(&p.src_dir()).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|i| SourceFile::new(canonicalize(&i.abs_path).unwrap_or(i.abs_path), i.text))
            .collect();
        if !files.iter().any(|f| f.path == file) {
            files.push(SourceFile::new(file.clone(), std::fs::read_to_string(&file).unwrap_or_default()));
        }
        Sources { files }
    }

    /// A crate of in-memory files (for tests).
    #[cfg(test)]
    pub fn from_texts(files: &[(&str, &str)]) -> Sources {
        Sources { files: files.iter().map(|(p, t)| SourceFile::new(PathBuf::from(p), t.to_string())).collect() }
    }

    pub fn file(&self, path: &Path) -> Option<&SourceFile> {
        self.files.iter().find(|f| f.path == path).or_else(|| {
            let path = canonicalize(path).ok()?;
            self.files.iter().find(|f| f.path == path)
        })
    }

    /// The file declaring something, by a test on each file's lines: `current` when it declares
    /// it, else the only other file that does (with two, the name is ambiguous).
    pub fn declaring<'a>(&'a self, current: &Path, declares: impl Fn(&[&str]) -> bool) -> Option<&'a SourceFile> {
        let here = self.file(current);
        if let Some(f) = here.filter(|f| declares(&f.lines())) {
            return Some(f);
        }
        let mut found = self.files.iter().filter(|f| here.is_none_or(|h| h.path != f.path) && declares(&f.lines()));
        let first = found.next()?;
        found.next().is_none().then_some(first)
    }

    /// How many files declare something.
    pub fn count_declaring(&self, declares: impl Fn(&[&str]) -> bool) -> usize {
        self.files.iter().filter(|f| declares(&f.lines())).count()
    }

    /// `path` when it isn't the error's file `current` (where edits and labels go by default).
    pub fn elsewhere(&self, path: &Path, current: &Path) -> Option<PathBuf> {
        let here = self.file(current).map(|f| f.path.as_path()).unwrap_or(current);
        (path != here).then(|| path.to_path_buf())
    }
}
