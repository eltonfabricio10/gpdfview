use std::env;
use std::fs;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct PersistedDocumentView {
    pub page_index: i32,
    pub scale: f64,
    pub rotation: i32,
    pub layout_mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppBookmark {
    pub path: PathBuf,
    pub page_index: i32,
    pub label: String,
}

pub fn session_storage_dir_for(home: &Path) -> PathBuf {
    let storage = home.join(".local/share/gpdfview");
    let _ = fs::create_dir_all(&storage);
    storage
}

pub fn session_storage_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    session_storage_dir_for(Path::new(&home))
}

pub fn remember_window_geometry_for_home(home: &Path, width: i32, height: i32) {
    let storage = session_storage_dir_for(home);
    let file = storage.join("window_size.txt");
    let _ = fs::write(file, format!("{width} {height}"));
}

pub fn remember_window_geometry(width: i32, height: i32) {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    remember_window_geometry_for_home(Path::new(&home), width, height);
}

pub fn last_window_geometry_for_home(home: &Path) -> Option<(i32, i32)> {
    let file = session_storage_dir_for(home).join("window_size.txt");
    let content = fs::read_to_string(file).ok()?;
    let mut parts = content.split_whitespace();
    let width = parts.next()?.parse::<i32>().ok()?;
    let height = parts.next()?.parse::<i32>().ok()?;
    Some((width, height))
}

pub fn remember_last_directory_for_home(home: &Path, dir: &Path) {
    let storage = session_storage_dir_for(home);
    let _ = fs::create_dir_all(&storage);
    let file = storage.join("last_dir.txt");
    let trimmed = dir.to_string_lossy().trim().to_string();
    if trimmed.is_empty() {
        let _ = fs::remove_file(file);
        return;
    }
    let _ = fs::write(file, trimmed);
}

pub fn remember_last_directory(dir: &Path) {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    remember_last_directory_for_home(Path::new(&home), dir);
}

pub fn last_directory_for_home(home: &Path) -> Option<PathBuf> {
    let file = session_storage_dir_for(home).join("last_dir.txt");
    let content = fs::read_to_string(file).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

pub fn last_directory_for_home_path() -> Option<PathBuf> {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    last_directory_for_home(Path::new(&home))
}

pub fn last_window_geometry_for_home_path() -> Option<(i32, i32)> {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    last_window_geometry_for_home(Path::new(&home))
}

pub fn write_session_entries(path: &Path, entries: &[PathBuf]) {
    let _ = fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
    let joined = entries
        .iter()
        .filter_map(|entry| {
            let text = entry.to_string_lossy().to_string();
            if text.trim().is_empty() {
                None
            } else {
                Some(text)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let _ = fs::write(path, format!("{joined}\n"));
}

pub fn read_session_entries(path: &Path) -> Vec<PathBuf> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}

pub fn normalize_recent_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for path in paths.into_iter().rev() {
        if seen.insert(path.clone()) {
            unique.push(path);
        }
    }
    unique.reverse();
    unique
}

pub fn remember_recent_document(path: &Path) {
    let file = session_storage_dir().join("recent.txt");
    let mut docs = read_session_entries(&file);
    docs.insert(0, path.to_path_buf());
    let normalized = normalize_recent_paths(docs);
    write_session_entries(&file, &normalized[..normalized.len().min(8)]);
}

pub fn newest_valid_recent_document(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find(|path| path.exists()).cloned()
}

pub fn load_recent_documents() -> Vec<PathBuf> {
    let file = session_storage_dir().join("recent.txt");
    let docs = read_session_entries(&file);
    normalize_recent_paths(docs)
}

pub fn toggle_favorite_document(path: &Path) -> bool {
    let file = session_storage_dir().join("favorites.txt");
    let mut favorites = read_session_entries(&file);
    if let Some(index) = favorites.iter().position(|entry| entry == path) {
        favorites.remove(index);
        write_session_entries(&file, &favorites);
        return false;
    }
    favorites.insert(0, path.to_path_buf());
    let deduped = normalize_recent_paths(favorites);
    write_session_entries(&file, &deduped[..deduped.len().min(12)]);
    true
}

pub fn load_favorite_documents() -> Vec<PathBuf> {
    let file = session_storage_dir().join("favorites.txt");
    normalize_recent_paths(read_session_entries(&file))
}

pub fn is_favorite_document(path: &Path) -> bool {
    load_favorite_documents().iter().any(|favorite| favorite == path)
}

fn bookmarks_file_for_home(home: &Path) -> PathBuf {
    session_storage_dir_for(home).join("app_bookmarks.txt")
}

fn normalize_bookmark_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn write_app_bookmarks(home: &Path, entries: &[AppBookmark]) {
    let file = bookmarks_file_for_home(home);
    let content = entries
        .iter()
        .map(|entry| {
            format!(
                "{}\t{}\t{}",
                entry.path.to_string_lossy(),
                entry.page_index,
                entry.label
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let _ = fs::write(file, format!("{content}\n"));
}

fn load_all_app_bookmarks_for_home(home: &Path) -> Vec<AppBookmark> {
    let file = bookmarks_file_for_home(home);
    let Ok(content) = fs::read_to_string(file) else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    for line in content.lines() {
        let mut parts = line.splitn(3, '\t');
        let Some(path) = parts.next() else {
            continue;
        };
        let Some(page_index) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
            continue;
        };
        let label = parts.next().unwrap_or_default().to_string();
        entries.push(AppBookmark {
            path: PathBuf::from(path),
            page_index,
            label,
        });
    }
    entries
}

pub fn load_bookmarks_for_home(home: &Path, path: &Path) -> Vec<AppBookmark> {
    let document_path = normalize_bookmark_path(path);
    let mut bookmarks = load_all_app_bookmarks_for_home(home)
        .into_iter()
        .filter(|bookmark| normalize_bookmark_path(&bookmark.path) == document_path)
        .collect::<Vec<_>>();
    bookmarks.sort_by_key(|bookmark| bookmark.page_index);
    bookmarks
}

pub fn load_document_bookmarks(path: &Path) -> Vec<AppBookmark> {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    load_bookmarks_for_home(Path::new(&home), path)
}

pub fn toggle_bookmark_page(path: &Path, page_index: i32, label: Option<&str>) -> bool {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    toggle_bookmark_for_home(Path::new(&home), path, page_index, label)
}

pub fn toggle_bookmark_for_home(home: &Path, document_path: &Path, page_index: i32, label: Option<&str>) -> bool {
    let canonical_path = normalize_bookmark_path(document_path);
    let bookmarks = load_all_app_bookmarks_for_home(home);
    let existing = bookmarks.iter().any(|bookmark| {
        normalize_bookmark_path(&bookmark.path) == canonical_path && bookmark.page_index == page_index
    });

    if existing {
        let next = bookmarks
            .into_iter()
            .filter(|bookmark| {
                normalize_bookmark_path(&bookmark.path) != canonical_path
                    || bookmark.page_index != page_index
            })
            .collect::<Vec<_>>();
        write_app_bookmarks(home, &next);
        return false;
    }

    let bookmark_label = match label.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => value.to_string(),
        None => format!("Página {}", page_index + 1),
    };

    let mut next = bookmarks;
    next.push(AppBookmark {
        path: canonical_path,
        page_index,
        label: bookmark_label,
    });
    write_app_bookmarks(home, &next);
    true
}

pub fn remove_bookmark_page(path: &Path, page_index: i32) -> bool {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let canonical_path = normalize_bookmark_path(path);
    let mut bookmarks = load_all_app_bookmarks_for_home(Path::new(&home));
    let original_len = bookmarks.len();
    bookmarks.retain(|bookmark| {
        normalize_bookmark_path(&bookmark.path) != canonical_path || bookmark.page_index != page_index
    });
    write_app_bookmarks(Path::new(&home), &bookmarks);
    original_len != bookmarks.len()
}

fn document_state_file(home: &Path) -> PathBuf {
    session_storage_dir_for(home).join("document_state.txt")
}

fn document_state_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn read_document_states(home: &Path) -> HashMap<String, PersistedDocumentView> {
    let file = document_state_file(home);
    let Ok(content) = fs::read_to_string(file) else {
        return HashMap::new();
    };

    let mut states = HashMap::new();
    for line in content.lines() {
        let mut parts = line.split('\t');
        let Some(path_key) = parts.next() else {
            continue;
        };
        let Some(page_index) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
            continue;
        };
        let Some(scale) = parts.next().and_then(|value| value.parse::<f64>().ok()) else {
            continue;
        };
        let Some(rotation) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
            continue;
        };
        let Some(layout_mode) = parts.next() else {
            continue;
        };
        states.insert(
            path_key.to_string(),
            PersistedDocumentView {
                page_index,
                scale,
                rotation,
                layout_mode: layout_mode.to_string(),
            },
        );
    }

    states
}

fn write_document_states(home: &Path, states: &HashMap<String, PersistedDocumentView>) {
    let file = document_state_file(home);
    let mut lines = Vec::new();
    for (path, state) in states {
        lines.push(format!(
            "{}\t{}\t{}\t{}\t{}",
            path, state.page_index, state.scale, state.rotation, state.layout_mode
        ));
    }
    lines.sort_unstable();
    let _ = fs::write(file, lines.join("\n"));
}

pub fn remember_document_view_for_home(
    home: &Path,
    path: &Path,
    page_index: i32,
    scale: f64,
    rotation: i32,
    layout_mode: &str,
) {
    if layout_mode.trim().is_empty() {
        return;
    }
    let mut states = read_document_states(home);
    states.insert(
        document_state_key(path),
        PersistedDocumentView {
            page_index,
            scale,
            rotation,
            layout_mode: layout_mode.to_string(),
        },
    );
    write_document_states(home, &states);
}

pub fn remember_document_view(
    path: &Path,
    page_index: i32,
    scale: f64,
    rotation: i32,
    layout_mode: &str,
) {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    remember_document_view_for_home(
        Path::new(&home),
        path,
        page_index,
        scale,
        rotation,
        layout_mode,
    );
}

pub fn load_document_view_for_home(home: &Path, path: &Path) -> Option<PersistedDocumentView> {
    let states = read_document_states(home);
    states.get(&document_state_key(path)).cloned()
}

pub fn load_document_view(path: &Path) -> Option<PersistedDocumentView> {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    load_document_view_for_home(Path::new(&home), path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_and_loads_per_document_view() {
        let base = std::env::temp_dir().join(format!(
            "gpdfview-docstate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch available")
                .as_nanos()
        ));
        let home = base.join("home");
        let docs = home.join("docs");
        let _ = fs::create_dir_all(&docs);
        let doc = docs.join("sample.pdf");
        let _ = fs::write(&doc, b"pdf");

        remember_document_view_for_home(&home, &doc, 7, 1.25, 90, "continuous");
        let restored = load_document_view_for_home(&home, &doc).expect("state should exist");

        assert_eq!(restored.page_index, 7);
        assert_eq!(restored.rotation, 90);
        assert_eq!(restored.layout_mode, "continuous");
        assert!((restored.scale - 1.25).abs() < 0.0001);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn app_bookmarks_persist_per_document_and_page() {
        let base = std::env::temp_dir().join(format!(
            "gpdfview-bookmarks-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch available")
                .as_nanos()
        ));
        let home = base.join("home");
        let docs = home.join("docs");
        let _ = fs::create_dir_all(&docs);
        let doc = docs.join("sample.pdf");
        let _ = fs::write(&doc, b"pdf");

        let added = toggle_bookmark_for_home(&home, &doc, 3, Some("Resumo"));
        assert!(added);
        let stored = load_bookmarks_for_home(&home, &doc);
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].page_index, 3);
        assert_eq!(stored[0].label, "Resumo");

        let removed = toggle_bookmark_for_home(&home, &doc, 3, None);
        assert!(!removed);
        assert!(load_bookmarks_for_home(&home, &doc).is_empty());

        let _ = fs::remove_dir_all(&base);
    }
}
