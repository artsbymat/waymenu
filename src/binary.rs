use std::{
    collections::HashSet,
    env, fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command},
};

use crate::SearchResult;

#[derive(Clone, Debug)]
pub struct Binary {
    pub name: String,
    pub path: PathBuf,
}

pub fn discover() -> Vec<Binary> {
    let mut seen = HashSet::new();
    let mut binaries = Vec::new();
    let Some(path) = env::var_os("PATH") else {
        return binaries;
    };
    for directory in env::split_paths(&path) {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            if seen.contains(name) {
                continue;
            }
            if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                continue;
            }
            let path = entry.path();
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
                continue;
            }
            if seen.insert(name.to_owned()) {
                binaries.push(Binary {
                    name: name.to_owned(),
                    path,
                });
            }
        }
    }
    binaries
}

impl Binary {
    pub fn as_result(&self) -> SearchResult {
        SearchResult {
            id: self.path.display().to_string(),
            title: self.name.clone(),
            description: self.path.display().to_string(),
            score: 0,
            icon: None,
            image: None,
            metadata: Some("executable".into()),
        }
    }
    pub fn launch(&self) -> std::io::Result<Child> {
        Command::new(&self.path).spawn()
    }
}
