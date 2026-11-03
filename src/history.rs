use std::{
    collections::HashMap,
    env, fs, io,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Usage {
    pub count: u64,
    pub last_used: u64,
}

#[derive(Default, Deserialize, Serialize)]
struct HistoryFile {
    version: u8,
    entries: HashMap<String, Usage>,
}

pub struct History {
    path: PathBuf,
    data: HistoryFile,
    max_entries: usize,
}

impl History {
    pub fn load() -> Self {
        Self::load_with_limit(500)
    }

    pub fn load_with_limit(max_entries: usize) -> Self {
        let path = state_path();
        Self::load_at(path, max_entries)
    }

    fn load_at(path: PathBuf, max_entries: usize) -> Self {
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            path,
            data,
            max_entries,
        }
    }

    pub fn score(&self, id: &str) -> i64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.data
            .entries
            .get(id)
            .map(|usage| {
                let age_hours = now.saturating_sub(usage.last_used) / 3600;
                let recency = 1_000i64.saturating_sub(age_hours.min(1_000) as i64);
                usage.count.min(100) as i64 * 20 + recency
            })
            .unwrap_or(0)
    }

    pub fn record(&mut self, id: &str) -> io::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let usage = self.data.entries.entry(id.to_owned()).or_default();
        usage.count = usage.count.saturating_add(1);
        usage.last_used = now;
        while self.data.entries.len() > self.max_entries {
            if let Some(oldest) = self
                .data
                .entries
                .iter()
                .min_by_key(|(_, usage)| usage.last_used)
                .map(|(id, _)| id.clone())
            {
                self.data.entries.remove(&oldest);
            }
        }
        self.data.version = 1;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("tmp");
        let bytes = serde_json::to_vec(&self.data).map_err(io::Error::other)?;
        fs::write(&temporary, bytes)?;
        fs::rename(temporary, &self.path)
    }
}

fn state_path() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("waymenu/history.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "waymenu-{name}-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn corrupted_history_is_ignored_and_replaced() {
        let path = temporary_path("corrupt-history");
        fs::write(&path, "not json").unwrap();
        let mut history = History::load_at(path.clone(), 10);
        assert_eq!(history.score("app"), 0);
        history.record("app").unwrap();
        let json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(json["version"], 1);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn history_enforces_entry_limit() {
        let path = temporary_path("limited-history");
        let mut history = History::load_at(path.clone(), 2);
        history.record("one").unwrap();
        history.record("two").unwrap();
        history.record("three").unwrap();
        assert!(history.data.entries.len() <= 2);
        fs::remove_file(path).unwrap();
    }
}
