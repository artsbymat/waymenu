use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const MAX_ENTRIES: usize = 50;
const MAX_ENTRY_BYTES: usize = 4096;
const MAX_FILE_BYTES: usize = MAX_ENTRIES * (6 * MAX_ENTRY_BYTES + 64);

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Entry {
    pub expression: String,
    pub answer: String,
}

impl Entry {
    fn fits(&self) -> bool {
        self.expression.len() + self.answer.len() <= MAX_ENTRY_BYTES
    }
}

pub(super) struct History {
    path: Option<PathBuf>,
    pub entries: Vec<Entry>,
}

impl History {
    pub(super) fn state_path() -> Option<PathBuf> {
        let absolute_dir = |key| {
            env::var_os(key)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        };
        absolute_dir("XDG_STATE_HOME")
            .or_else(|| absolute_dir("HOME").map(|home| home.join(".local/state")))
            .map(|directory| directory.join("waymenu/rink/history.json"))
    }

    pub(super) fn load(path: Option<PathBuf>) -> Self {
        let entries = path
            .as_deref()
            .map(|path| {
                read(path).unwrap_or_else(|error| {
                    eprintln!("waymenu-rink: could not read calculator history: {error}");
                    Vec::new()
                })
            })
            .unwrap_or_default();
        Self { path, entries }
    }

    pub(super) fn record(&mut self, entry: Entry) -> io::Result<()> {
        if !entry.fits() {
            return Ok(());
        }
        self.update(|entries| {
            entries.retain(|saved| saved.expression != entry.expression);
            entries.truncate(MAX_ENTRIES - 1);
            entries.insert(0, entry);
        })
    }

    pub(super) fn delete(&mut self, expression: &str) -> io::Result<()> {
        self.update(|entries| entries.retain(|saved| saved.expression != expression))
    }

    fn update(&mut self, edit: impl FnOnce(&mut Vec<Entry>)) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let directory = path.parent().expect("history path has a parent directory");
        fs::create_dir_all(directory)?;
        // Use a separate lock file because persisting history replaces its inode.
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))?;
        lock.lock()?;

        let mut entries = read(path)?;
        edit(&mut entries);
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer(&mut file, &entries).map_err(io::Error::other)?;
        file.persist(path)?;
        self.entries = entries;
        Ok(())
    }
}

fn read(path: &Path) -> io::Result<Vec<Entry>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut data = Vec::new();
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut data)?;
    if data.len() > MAX_FILE_BYTES {
        return Ok(Vec::new());
    }
    let mut entries: Vec<Entry> = serde_json::from_slice(&data).unwrap_or_default();
    entries.retain(Entry::fits);
    entries.truncate(MAX_ENTRIES);
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_recent_unique_entries_across_instances() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        fs::write(&path, "invalid json").unwrap();
        let mut first = History::load(Some(path.clone()));
        let mut second = History::load(Some(path.clone()));
        assert!(first.entries.is_empty());
        for index in 0..MAX_ENTRIES {
            first
                .record(Entry {
                    expression: index.to_string(),
                    answer: index.to_string(),
                })
                .unwrap();
        }
        second
            .record(Entry {
                expression: MAX_ENTRIES.to_string(),
                answer: "new".into(),
            })
            .unwrap();
        first
            .record(Entry {
                expression: "1".into(),
                answer: "updated".into(),
            })
            .unwrap();

        let reloaded = History::load(Some(path));
        assert_eq!(reloaded.entries.len(), MAX_ENTRIES);
        assert_eq!(reloaded.entries[0].expression, "1");
        assert_eq!(reloaded.entries[0].answer, "updated");
        assert_eq!(reloaded.entries[1].answer, "new");
        assert_eq!(reloaded.entries.last().unwrap().expression, "2");
    }
}
