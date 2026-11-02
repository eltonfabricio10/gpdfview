use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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
