use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use rusqlite::{functions::FunctionFlags, Connection, OpenFlags};
use tempfile::TempDir;
use waymenu_plugin_api::PluginResult;

use super::super::{BROWSER_ICON, BROWSER_NAME, PLUGIN_ID};

const RESULT_LIMIT: usize = 20;

pub(super) struct History {
    root: PathBuf,
    profiles: Vec<Profile>,
    unavailable: Vec<String>,
    snapshots: TempDir,
}

struct Profile {
    name: String,
    snapshot: PathBuf,
}

impl History {
    pub(super) fn load(root: PathBuf) -> io::Result<Self> {
        let directories = fs::read_dir(&root)?;
        let snapshots = tempfile::Builder::new()
            .prefix(&format!("waymenu-{}-", PLUGIN_ID.to_string_lossy()))
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in("/tmp")?;
        let mut history = Self {
            root,
            profiles: Vec::new(),
            unavailable: Vec::new(),
            snapshots,
        };

        for entry in directories.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let source = entry.path().join("History");
            match source.metadata() {
                Ok(metadata) if metadata.is_file() => {}
                Ok(_) => continue,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => {
                    eprintln!("{}: {name}: {error}", PLUGIN_ID.to_string_lossy());
                    history.unavailable.push(name);
                    continue;
                }
            }
            let snapshot = history
                .snapshots
                .path()
                .join(format!("{}.sqlite", history.profiles.len()));
            // shortcut: copy the main file like cp, revisit SQLite backups if WAL data is needed.
            match fs::copy(&source, &snapshot) {
                Ok(_) => history.profiles.push(Profile { name, snapshot }),
                Err(error) => {
                    eprintln!("{}: {name}: {error}", PLUGIN_ID.to_string_lossy());
                    history.unavailable.push(name);
                }
            }
        }
        history
            .profiles
            .sort_unstable_by(|a, b| a.name.cmp(&b.name));
        history.unavailable.sort_unstable();
        Ok(history)
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn profile_name(&self, index: usize) -> Option<&str> {
        self.profiles
            .get(index)
            .map(|profile| profile.name.as_str())
    }

    pub(super) fn search(&self, query: &str) -> Vec<PluginResult> {
        let query = query.trim().to_lowercase();
        let mut matches = Vec::new();
        let mut unavailable = self.unavailable.clone();
        for (index, profile) in self.profiles.iter().enumerate() {
            match profile.search(index, &query) {
                Ok(results) => {
                    matches.extend(results);
                    matches.sort_unstable_by(|a, b| b.score.cmp(&a.score).then(a.id.cmp(&b.id)));
                    matches.truncate(RESULT_LIMIT);
                }
                Err(error) => {
                    eprintln!("{}: {}: {error}", PLUGIN_ID.to_string_lossy(), profile.name);
                    unavailable.push(profile.name.clone());
                }
            }
        }
        if matches.is_empty() && unavailable.is_empty() {
            matches.push(if self.profiles.is_empty() {
                notice(
                    &format!("No {BROWSER_NAME} profiles found"),
                    self.root.display().to_string(),
                )
            } else {
                notice(
                    &format!("No matching {BROWSER_NAME} history"),
                    String::new(),
                )
            });
        }
        if !unavailable.is_empty() {
            matches.push(notice(
                &format!("Could not read some {BROWSER_NAME} profiles"),
                unavailable.join(", "),
            ));
        }
        matches
    }
}

impl Profile {
    fn search(&self, index: usize, query: &str) -> rusqlite::Result<Vec<PluginResult>> {
        let db = Connection::open_with_flags(&self.snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.execute_batch("PRAGMA trusted_schema = OFF; PRAGMA cache_size = -512")?;
        db.create_scalar_function(
            "title_contains",
            2,
            FunctionFlags::SQLITE_UTF8
                | FunctionFlags::SQLITE_DETERMINISTIC
                | FunctionFlags::SQLITE_DIRECTONLY,
            |ctx| {
                let title = ctx.get_raw(0).as_str().unwrap_or_default();
                let query = ctx.get_raw(1).as_str()?;
                Ok(title.to_lowercase().contains(query))
            },
        )?;
        let mut statement = db.prepare(
            "SELECT url, title, last_visit_time
             FROM urls
             WHERE hidden = 0 AND (url GLOB 'https://?*' OR url GLOB 'http://?*')
               AND (?1 = '' OR instr(lower(url), ?1) > 0 OR title_contains(title, ?1))
             ORDER BY last_visit_time DESC, id
             LIMIT ?2",
        )?;
        let rows = statement.query_map((query, RESULT_LIMIT as i64), |row| {
            let url: String = row.get(0)?;
            let title: Option<String> = row.get(1)?;
            Ok(PluginResult {
                id: format!("{index}:{url}"),
                title: title
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or_else(|| url.clone()),
                description: format!("{} · {url}", self.name),
                score: row.get(2)?,
                icon: Some(BROWSER_ICON.into()),
                image: None,
                metadata: None,
            })
        })?;
        rows.collect()
    }
}

pub(super) fn notice(title: &str, description: String) -> PluginResult {
    PluginResult {
        id: String::new(),
        title: title.into(),
        description,
        score: 0,
        icon: None,
        image: None,
        metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn profile(root: &Path, name: &str) -> Connection {
        let directory = root.join(name);
        fs::create_dir(&directory).unwrap();
        let db = Connection::open(directory.join("History")).unwrap();
        db.execute_batch(
            "CREATE TABLE urls (
                 id INTEGER PRIMARY KEY,
                 url TEXT,
                 title TEXT,
                 last_visit_time INTEGER,
                 hidden INTEGER
             );",
        )
        .unwrap();
        db
    }

    #[test]
    fn searches_full_profile_snapshots_and_opens_in_the_source_profile() {
        let root = tempfile::tempdir().unwrap();
        let default = profile(root.path(), "Default");
        default
            .execute_batch(
                "INSERT INTO urls VALUES (1, 'https://example.org/old', 'École', 1, 0);
                 INSERT INTO urls VALUES (2, 'https://example.org/hidden', 'École', 900, 1);
                 INSERT INTO urls VALUES (3, 'file:///tmp/old', 'École', 900, 0);
                 WITH RECURSIVE recent(n) AS (VALUES (4) UNION ALL SELECT n+1 FROM recent WHERE n<550)
                 INSERT INTO urls SELECT n, 'https://recent.example/' || n, 'Recent', n, 0 FROM recent;",
            )
            .unwrap();
        let second = profile(root.path(), "Profile 1");
        second
            .execute(
                "INSERT INTO urls VALUES (1, 'https://example.org/old', 'ÉCOLE', 2, 0)",
                (),
            )
            .unwrap();
        let history = History::load(root.path().to_owned()).unwrap();
        let snapshot_dir = history.snapshots.path().to_owned();
        assert_eq!(
            fs::metadata(&snapshot_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );

        second
            .execute(
                "INSERT INTO urls VALUES (2, 'https://new.example/', 'After snapshot', 1000, 0)",
                (),
            )
            .unwrap();

        let results = history.search("école");
        assert_eq!(results.len(), 2);
        assert_ne!(results[0].id, results[1].id);
        assert!(results[0].description.starts_with("Profile 1"));
        assert!(results[1].description.starts_with("Default"));
        assert_eq!(history.search("EXAMPLE.ORG/OLD").len(), 2);
        assert_eq!(
            history.search("After snapshot")[0].title,
            format!("No matching {BROWSER_NAME} history")
        );
        assert_eq!(history.search("").len(), RESULT_LIMIT);

        let command = super::super::open_command(&history, &results[0].id).unwrap();
        assert_eq!(command.get_program(), crate::BROWSER_EXECUTABLE);
        assert_eq!(results[0].icon.as_deref(), Some(BROWSER_ICON));
        let args: Vec<_> = command.get_args().collect();
        let root_arg = format!("--user-data-dir={}", root.path().display());
        assert_eq!(
            args,
            [
                OsStr::new(&root_arg),
                OsStr::new("--profile-directory=Profile 1"),
                OsStr::new("--"),
                OsStr::new("https://example.org/old"),
            ]
        );
        assert!(super::super::open_command(&history, "1:--incognito").is_none());
        drop(history);
        assert!(!snapshot_dir.exists());

        let refreshed = History::load(root.path().to_owned()).unwrap();
        assert_eq!(
            refreshed.search("After snapshot")[0].title,
            "After snapshot"
        );
    }

    #[test]
    fn reports_a_broken_profile_and_keeps_searching_the_others() {
        let root = tempfile::tempdir().unwrap();
        let db = profile(root.path(), "Default");
        db.execute(
            "INSERT INTO urls VALUES (1, 'https://example.org/', '', 1, 0)",
            (),
        )
        .unwrap();
        let broken = root.path().join("Profile 1");
        fs::create_dir(&broken).unwrap();
        fs::write(broken.join("History"), "invalid").unwrap();

        let history = History::load(root.path().to_owned()).unwrap();
        let results = history.search("");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "https://example.org/");
        assert_eq!(results[1].description, "Profile 1");
        assert!(results[1].id.is_empty());
    }

    #[test]
    fn copies_the_main_file_while_the_browser_holds_an_exclusive_lock() {
        let root = tempfile::tempdir().unwrap();
        let writer = profile(root.path(), "Default");
        writer
            .execute_batch(
                "PRAGMA journal_mode = TRUNCATE;
                 PRAGMA locking_mode = EXCLUSIVE;
                 BEGIN EXCLUSIVE;
                 INSERT INTO urls VALUES (1, 'https://example.org/live', 'Live history', 1, 0);
                 COMMIT;
                 BEGIN EXCLUSIVE;
                 INSERT INTO urls VALUES (2, 'https://example.org/uncommitted', 'Uncommitted', 2, 0);",
            )
            .unwrap();
        let source = root.path().join("Default/History");
        assert!(
            fs::metadata(source.with_file_name("History-journal"))
                .unwrap()
                .len()
                > 0
        );

        let history = History::load(root.path().to_owned()).unwrap();
        assert_eq!(
            fs::read(&history.profiles[0].snapshot).unwrap(),
            fs::read(source).unwrap()
        );
        assert_eq!(history.search("live")[0].title, "Live history");
    }
}
