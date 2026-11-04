use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use waymenu_plugin_api::PluginResult;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageEntry {
    pub repo: String,
    pub name: String,
    pub version: String,
    pub installed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryMode {
    All,
    Install,
    Remove,
    Info,
    SysUpgrade,
    CleanCache,
}

fn cache_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(dir).join("waymenu").join("pacman_packages.cache")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cache").join("waymenu").join("pacman_packages.cache")
    } else {
        std::env::temp_dir().join("waymenu_pacman_packages.cache")
    }
}

fn is_cache_valid(path: &Path) -> bool {
    let Ok(cache_meta) = fs::metadata(path) else {
        return false;
    };
    let Ok(cache_mtime) = cache_meta.modified() else {
        return false;
    };

    // If cache was generated in the last 10 minutes, use it immediately
    if let Ok(elapsed) = cache_mtime.elapsed() {
        if elapsed < Duration::from_secs(600) {
            return true;
        }
    }

    // Otherwise check pacman db mtime
    if let Ok(sync_meta) = fs::metadata("/var/lib/pacman/sync") {
        if let Ok(sync_mtime) = sync_meta.modified() {
            if sync_mtime > cache_mtime {
                return false;
            }
        }
    }
    if let Ok(local_meta) = fs::metadata("/var/lib/pacman/local") {
        if let Ok(local_mtime) = local_meta.modified() {
            if local_mtime > cache_mtime {
                return false;
            }
        }
    }

    true
}

/// Discover all available packages from sync repositories and local database.
pub fn discover_packages() -> Vec<PackageEntry> {
    let cache = cache_path();
    if let Ok(content) = fs::read_to_string(&cache) {
        let pkgs = parse_pacman_sl_output(&content);
        if !pkgs.is_empty() {
            if !is_cache_valid(&cache) {
                let cache_clone = cache.clone();
                std::thread::spawn(move || {
                    let _ = refresh_disk_cache(&cache_clone);
                });
            }
            return pkgs;
        }
    }

    refresh_disk_cache(&cache)
}

pub fn refresh_disk_cache(cache: &Path) -> Vec<PackageEntry> {
    let mut packages = Vec::new();
    let mut known = HashSet::new();
    let mut cache_content = String::new();

    // Query sync databases via pacman -Sl
    if let Ok(output) = Command::new("pacman").arg("-Sl").output() {
        if output.status.success() {
            if let Ok(text) = std::str::from_utf8(&output.stdout) {
                cache_content.push_str(text);
                packages = parse_pacman_sl_output(text);
                for p in &packages {
                    known.insert(p.name.clone());
                }
            }
        }
    }

    // Augment with foreign/AUR packages in /var/lib/pacman/local
    let local_pkgs = discover_local_packages(&known);
    for lp in &local_pkgs {
        cache_content.push_str(&format!("{} {} {} [installed]\n", lp.repo, lp.name, lp.version));
    }
    packages.extend(local_pkgs);

    if !cache_content.is_empty() {
        if let Some(parent) = cache.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(cache, cache_content);
    }

    packages
}

/// Parses lines of `pacman -Sl` output into [`PackageEntry`] structs.
pub fn parse_pacman_sl_output(output: &str) -> Vec<PackageEntry> {
    let mut packages = Vec::with_capacity(output.lines().count());
    for line in output.lines() {
        let mut parts = line.split_ascii_whitespace();
        let Some(repo) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        let Some(version) = parts.next() else {
            continue;
        };
        let installed = parts.any(|part| part.starts_with("[installed"));
        packages.push(PackageEntry {
            repo: repo.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            installed,
        });
    }
    packages
}

/// Discovers foreign/local packages installed from AUR or manual builds.
pub fn discover_local_packages(known: &HashSet<String>) -> Vec<PackageEntry> {
    let mut local_pkgs = Vec::new();
    let Ok(entries) = fs::read_dir("/var/lib/pacman/local") else {
        return local_pkgs;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(name_str) = file_name.to_str() else {
            continue;
        };
        if name_str == "ALPM_DB_VERSION" {
            continue;
        }

        let mut parts = name_str.rsplitn(3, '-');
        let _pkgrel = parts.next();
        let version = parts.next();
        let name = parts.next();

        if let (Some(name), Some(version)) = (name, version) {
            if !known.contains(name) {
                local_pkgs.push(PackageEntry {
                    repo: "local".to_string(),
                    name: name.to_string(),
                    version: version.to_string(),
                    installed: true,
                });
            }
        }
    }

    local_pkgs
}

/// Parses the user's raw query into a [`QueryMode`] and actual search term.
pub fn parse_query(raw_query: &str) -> (QueryMode, &str) {
    let trimmed = raw_query.trim_start();
    if trimmed.is_empty() {
        return (QueryMode::All, "");
    }
    if trimmed.eq_ignore_ascii_case("-syu")
        || trimmed.eq_ignore_ascii_case("update")
        || trimmed.eq_ignore_ascii_case("upgrade")
    {
        return (QueryMode::SysUpgrade, "");
    }
    if trimmed.eq_ignore_ascii_case("-sc") || trimmed.eq_ignore_ascii_case("clean") {
        return (QueryMode::CleanCache, "");
    }

    if let Some(rest) = trimmed
        .strip_prefix("-rns ")
        .or_else(|| trimmed.strip_prefix("-Rns "))
        .or_else(|| trimmed.strip_prefix("-r "))
        .or_else(|| trimmed.strip_prefix("-R "))
        .or_else(|| trimmed.strip_prefix("rm "))
    {
        return (QueryMode::Remove, rest.trim());
    }

    if let Some(rest) = trimmed
        .strip_prefix("-qi ")
        .or_else(|| trimmed.strip_prefix("-Qi "))
        .or_else(|| trimmed.strip_prefix("-q "))
        .or_else(|| trimmed.strip_prefix("-Q "))
    {
        return (QueryMode::Info, rest.trim());
    }

    if let Some(rest) = trimmed
        .strip_prefix("-s ")
        .or_else(|| trimmed.strip_prefix("-S "))
    {
        return (QueryMode::Install, rest.trim());
    }

    (QueryMode::All, trimmed.trim())
}

/// Scores a package against a search query.
#[allow(dead_code)]
pub fn score_package(pkg: &PackageEntry, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(if pkg.installed { 500 } else { 0 });
    }
    let query_lower = query.to_lowercase();
    score_package_lowercased(pkg, query, &query_lower)
}

pub fn score_package_lowercased(
    pkg: &PackageEntry,
    query_raw: &str,
    query_lower: &str,
) -> Option<i64> {
    if query_lower.is_empty() {
        return Some(if pkg.installed { 500 } else { 0 });
    }

    let name = &pkg.name;

    // Exact match
    if name.eq_ignore_ascii_case(query_raw) {
        return Some(10_000 + if pkg.installed { 100 } else { 0 });
    }

    // Prefix match
    let starts_prefix = if name.is_ascii() && query_lower.is_ascii() {
        name.len() >= query_lower.len()
            && name[..query_lower.len()].eq_ignore_ascii_case(query_lower)
    } else {
        name.to_lowercase().starts_with(query_lower)
    };

    if starts_prefix {
        let penalty = (name.len() - query_lower.len()) as i64 * 10;
        return Some(7_000 - penalty.min(2_000) + if pkg.installed { 100 } else { 0 });
    }

    // Substring match
    if let Some(pos) = name.find(query_lower) {
        let pos_penalty = pos as i64 * 50;
        let len_penalty = (name.len() - query_lower.len()) as i64 * 10;
        return Some(4_000 - pos_penalty.min(1_000) - len_penalty.min(1_000));
    }

    // Skip expensive ordered subsequence search for 1-char queries to ensure instant typing
    if query_lower.len() <= 1 {
        return None;
    }

    // Ordered subsequence match without allocating per candidate
    let mut chars = name.char_indices();
    let mut prev = None;
    let mut score = 2_000i64;
    for needle in query_lower.chars() {
        let (idx, _) = chars.find(|(_, c)| c.to_ascii_lowercase() == needle)?;
        if let Some(last) = prev {
            let gap = idx.saturating_sub(last + 1) as i64;
            score -= gap.min(20) * 10;
        } else {
            score -= (idx as i64).min(20) * 10;
        }
        prev = Some(idx);
    }
    score -= (name.len() as i64).min(100);
    Some(score.max(10))
}

/// Filters and ranks packages based on query and query mode.
pub fn filter_and_rank_packages(packages: &[PackageEntry], raw_query: &str) -> Vec<PluginResult> {
    let (mode, query) = parse_query(raw_query);

    match mode {
        QueryMode::SysUpgrade => vec![PluginResult {
            id: "sysupgrade".into(),
            title: "System Upgrade (sudo pacman -Syu)".into(),
            description: "Synchronize repositories and upgrade all system packages".into(),
            score: 10_000,
            icon: Some("🔄".into()),
            image: None,
            metadata: Some("pacman".into()),
        }],
        QueryMode::CleanCache => vec![PluginResult {
            id: "cleancache".into(),
            title: "Clean Cache (sudo pacman -Sc)".into(),
            description: "Remove cached package tarballs to reclaim disk space".into(),
            score: 10_000,
            icon: Some("🧹".into()),
            image: None,
            metadata: Some("pacman".into()),
        }],
        QueryMode::All if query.is_empty() => vec![
            PluginResult {
                id: "sysupgrade".into(),
                title: "System Upgrade (sudo pacman -Syu)".into(),
                description: "Synchronize repositories and upgrade all packages".into(),
                score: 10_000,
                icon: Some("🔄".into()),
                image: None,
                metadata: Some("pacman".into()),
            },
            PluginResult {
                id: "cleancache".into(),
                title: "Clean Cache (sudo pacman -Sc)".into(),
                description: "Clean package cache to free up disk space".into(),
                score: 9_000,
                icon: Some("🧹".into()),
                image: None,
                metadata: Some("pacman".into()),
            },
            PluginResult {
                id: "orphans".into(),
                title: "Remove Orphan Packages".into(),
                description: "Find and remove unused dependencies (pacman -Qtdq)".into(),
                score: 8_000,
                icon: Some("🗑️".into()),
                image: None,
                metadata: Some("pacman".into()),
            },
            PluginResult {
                id: "help".into(),
                title: "Pacman Search Help".into(),
                description:
                    "Type a package name (e.g. 'pac firefox', 'pac -R <pkg>', 'pac -Q <pkg>')"
                        .into(),
                score: 7_000,
                icon: Some("ℹ️".into()),
                image: None,
                metadata: Some("pacman".into()),
            },
        ],
        _ => {
            let query_lower = query.to_lowercase();
            let max_results = 25;
            let mut scored = Vec::with_capacity(256);
            for pkg in packages {
                // If in Remove or Info mode, only consider installed packages
                if (mode == QueryMode::Remove || mode == QueryMode::Info) && !pkg.installed {
                    continue;
                }

                if let Some(score) = score_package_lowercased(pkg, query, &query_lower) {
                    scored.push((pkg, score));
                }
            }

            if scored.len() > max_results {
                scored.select_nth_unstable_by(max_results, |(pkg_a, score_a), (pkg_b, score_b)| {
                    score_b
                        .cmp(score_a)
                        .then_with(|| pkg_a.name.cmp(&pkg_b.name))
                });
                scored.truncate(max_results);
            }

            // Sort only the top results
            scored.sort_unstable_by(|(pkg_a, score_a), (pkg_b, score_b)| {
                score_b
                    .cmp(score_a)
                    .then_with(|| pkg_a.name.cmp(&pkg_b.name))
            });

            let mut results = Vec::with_capacity(scored.len());
            for (pkg, score) in scored {
                match mode {
                    QueryMode::Remove => {
                        results.push(PluginResult {
                            id: format!("remove:{}", pkg.name),
                            title: format!("Remove {}", pkg.name),
                            description: format!(
                                "{} {} · sudo pacman -Rns {}",
                                pkg.repo, pkg.version, pkg.name
                            ),
                            score,
                            icon: Some("🗑️".into()),
                            image: None,
                            metadata: Some("pacman".into()),
                        });
                    }
                    QueryMode::Info => {
                        results.push(PluginResult {
                            id: format!("info:{}", pkg.name),
                            title: format!("{} (v{}) [installed]", pkg.name, pkg.version),
                            description: format!("{} · pacman -Qi {}", pkg.repo, pkg.name),
                            score,
                            icon: Some("ℹ️".into()),
                            image: None,
                            metadata: Some("pacman".into()),
                        });
                    }
                    QueryMode::All | QueryMode::Install => {
                        if pkg.installed {
                            results.push(PluginResult {
                                id: format!("install:{}", pkg.name),
                                title: format!("{} [installed]", pkg.name),
                                description: format!(
                                    "{} {} · Press Enter to reinstall/update",
                                    pkg.repo, pkg.version
                                ),
                                score,
                                icon: Some("📦".into()),
                                image: None,
                                metadata: Some("pacman".into()),
                            });

                            // Add a convenient Uninstall option if high match
                            if score >= 5_000 && !query.is_empty() {
                                results.push(PluginResult {
                                    id: format!("remove:{}", pkg.name),
                                    title: format!("Uninstall {}", pkg.name),
                                    description: format!("sudo pacman -Rns {}", pkg.name),
                                    score: score - 1,
                                    icon: Some("🗑️".into()),
                                    image: None,
                                    metadata: Some("pacman".into()),
                                });
                            }
                        } else {
                            results.push(PluginResult {
                                id: format!("install:{}", pkg.name),
                                title: pkg.name.clone(),
                                description: format!(
                                    "{} {} · Press Enter to install (sudo pacman -S {})",
                                    pkg.repo, pkg.version, pkg.name
                                ),
                                score,
                                icon: Some("📦".into()),
                                image: None,
                                metadata: Some("pacman".into()),
                            });
                        }
                    }
                    _ => {}
                }
            }

            if results.is_empty() && !query.is_empty() {
                results.push(PluginResult {
                    id: format!("install:{query}"),
                    title: format!("Install '{query}' with pacman"),
                    description: format!(
                        "Attempt to install package '{query}' (sudo pacman -S {query})"
                    ),
                    score: 0,
                    icon: Some("📦".into()),
                    image: None,
                    metadata: Some("pacman".into()),
                });
            }

            results
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pacman_sl_lines() {
        let sample = "core acl 2.4.0-1 [installed]\nextra firefox 135.0-1 [installed: 134.0-1]\nextra ripgrep 15.2.0-1\n";
        let parsed = parse_pacman_sl_output(sample);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].name, "acl");
        assert!(parsed[0].installed);
        assert_eq!(parsed[1].name, "firefox");
        assert!(parsed[1].installed);
        assert_eq!(parsed[2].name, "ripgrep");
        assert!(!parsed[2].installed);
    }

    #[test]
    fn parses_query_modes() {
        assert_eq!(parse_query(""), (QueryMode::All, ""));
        assert_eq!(parse_query("-Syu"), (QueryMode::SysUpgrade, ""));
        assert_eq!(parse_query("update"), (QueryMode::SysUpgrade, ""));
        assert_eq!(parse_query("-Sc"), (QueryMode::CleanCache, ""));
        assert_eq!(parse_query("-R firefox"), (QueryMode::Remove, "firefox"));
        assert_eq!(parse_query("rm firefox"), (QueryMode::Remove, "firefox"));
        assert_eq!(parse_query("-Qi ripgrep"), (QueryMode::Info, "ripgrep"));
        assert_eq!(parse_query("-S neovim"), (QueryMode::Install, "neovim"));
        assert_eq!(parse_query("foot"), (QueryMode::All, "foot"));
    }

    #[test]
    fn ranks_exact_and_prefix_matches_higher() {
        let pkgs = vec![
            PackageEntry {
                repo: "extra".into(),
                name: "firefox-developer-edition".into(),
                version: "135.0".into(),
                installed: false,
            },
            PackageEntry {
                repo: "extra".into(),
                name: "firefox".into(),
                version: "135.0".into(),
                installed: true,
            },
            PackageEntry {
                repo: "extra".into(),
                name: "libfirefox".into(),
                version: "1.0".into(),
                installed: false,
            },
        ];

        let results = filter_and_rank_packages(&pkgs, "firefox");
        assert!(!results.is_empty());
        assert_eq!(results[0].title, "firefox [installed]");
    }

    #[test]
    fn remove_mode_filters_only_installed_packages() {
        let pkgs = vec![
            PackageEntry {
                repo: "extra".into(),
                name: "installed-app".into(),
                version: "1.0".into(),
                installed: true,
            },
            PackageEntry {
                repo: "extra".into(),
                name: "uninstalled-app".into(),
                version: "1.0".into(),
                installed: false,
            },
        ];

        let results = filter_and_rank_packages(&pkgs, "-R app");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "remove:installed-app");
    }
}
