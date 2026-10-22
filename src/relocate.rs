//! Moving and renaming local files. The runtime calls these; the state only
//! sees the outcome.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::i18n::Lang;

/// What got done, and the reason it stopped early if it did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    /// `(from, to)` of each file that moved.
    pub moved: Vec<(PathBuf, PathBuf)>,
    pub error: Option<String>,
}

impl Outcome {
    fn failed(error: String) -> Self {
        Self { moved: Vec::new(), error: Some(error) }
    }
}

/// Moves `files` into the folder `dir`: `~/` is the home folder and a relative
/// path starts at the folder of the first file. The folder is created if missing.
pub fn move_to(files: &[PathBuf], dir: &str, home: Option<&Path>, lang: Lang) -> Outcome {
    let dir = dir.trim();
    if dir.is_empty() {
        return Outcome::failed(lang.pick("Falta la carpeta", "The folder is missing").into());
    }
    let target = match (dir.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        (None, Some(home)) if dir == "~" => home.to_owned(),
        _ => PathBuf::from(dir),
    };
    let target = match files.first().and_then(|f| f.parent()) {
        Some(parent) if target.is_relative() => parent.join(target),
        _ => target,
    };
    if let Err(e) = std::fs::create_dir_all(&target) {
        return Outcome::failed(format!(
            "{} {}: {e}",
            lang.pick("No pude crear", "Could not create"),
            target.display()
        ));
    }
    let mut outcome = Outcome::default();
    for from in files {
        let Some(name) = from.file_name() else { continue };
        let to = target.join(name);
        if *from == to {
            continue;
        }
        match relocate(from, &to, lang) {
            Ok(()) => outcome.moved.push((from.clone(), to)),
            Err(error) => {
                outcome.error = Some(error);
                break;
            }
        }
    }
    outcome
}

/// Gives `from` the name `name`, keeping its extension unless the new name brings its own.
pub fn rename(from: &Path, name: &str, lang: Lang) -> Outcome {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Outcome::failed(lang.pick("Nombre no válido", "Invalid name").into());
    }
    let file = match from.extension().map(|e| e.to_string_lossy().into_owned()) {
        Some(ext) if !name.to_lowercase().ends_with(&format!(".{}", ext.to_lowercase())) => format!("{name}.{ext}"),
        _ => name.to_owned(),
    };
    let to = from.with_file_name(file);
    if to == from {
        return Outcome::default();
    }
    match relocate(from, &to, lang) {
        Ok(()) => Outcome { moved: vec![(from.to_owned(), to)], error: None },
        Err(error) => Outcome::failed(error),
    }
}

/// One file to its new path; across disks it is copied and then removed.
fn relocate(from: &Path, to: &Path, lang: Lang) -> Result<(), String> {
    if to.exists() {
        return Err(format!("{} {}", to.display(), lang.pick("ya existe", "already exists")));
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            std::fs::copy(from, to).and_then(|_| std::fs::remove_file(from)).map_err(|e| {
                let _ = std::fs::remove_file(to);
                format!("{}: {e}", from.display())
            })
        }
        Err(e) => Err(format!("{}: {e}", from.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::temp_dir;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }

    #[test]
    fn moves_into_a_new_folder_and_resolves_relative_paths() {
        let root = temp_dir("move");
        let (a, b) = (root.join("in/a.mkv"), root.join("in/b.mkv"));
        touch(&a);
        touch(&b);
        let out = move_to(&[a.clone(), b.clone()], "../pelis/serie", None, Lang::Es);
        let target = root.join("in/../pelis/serie");
        assert_eq!(out.moved, [(a.clone(), target.join("a.mkv")), (b, target.join("b.mkv"))]);
        assert_eq!(out.error, None);
        assert!(!a.exists() && target.join("a.mkv").exists());
    }

    #[test]
    fn home_and_existing_targets() {
        let root = temp_dir("home");
        let a = root.join("a.mp3");
        touch(&a);
        touch(&root.join("home/Musica/a.mp3"));
        let out = move_to(std::slice::from_ref(&a), "~/Musica", Some(&root.join("home")), Lang::En);
        assert!(out.moved.is_empty() && out.error.unwrap().contains("already exists"));
        assert!(a.exists(), "nothing is overwritten");
    }

    #[test]
    fn rename_keeps_the_extension_and_refuses_paths() {
        let root = temp_dir("rename");
        let a = root.join("viejo.mkv");
        touch(&a);
        assert_eq!(rename(&a, "a/b", Lang::Es).error.as_deref(), Some("Nombre no válido"));
        let out = rename(&a, " nuevo ", Lang::Es);
        assert_eq!(out.moved, [(a.clone(), root.join("nuevo.mkv"))]);
        assert!(root.join("nuevo.mkv").exists() && !a.exists());
        let b = root.join("nuevo.mkv");
        assert_eq!(rename(&b, "otro.MKV", Lang::Es).moved, [(b, root.join("otro.MKV"))]);
        assert_eq!(rename(&root.join("otro.MKV"), "otro", Lang::Es), Outcome::default(), "same name: nothing");
    }
}
