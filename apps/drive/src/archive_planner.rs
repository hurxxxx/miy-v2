//! AI-DO domains/files/archive_planner.py, line by line: safe file names and
//! the entries of a bundle download (folders expanded to their accessible
//! subtree, explicit files once, colliding paths numbered).
//!
//! AI-DO wrote these entries into a ZIP on the server. miy downloads the
//! entry files one by one (docs/PORTING.md §3.5), so the planner now orders
//! the downloads and its directory entries are informational.
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct ArchivePlanFolder {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct ArchivePlanFile {
    pub id: String,
    pub folder_id: Option<String>,
    pub filename: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedArchiveEntry {
    pub archive_path: String,
    pub file_id: Option<String>,
}

fn unsafe_char(c: char) -> bool {
    (c as u32) <= 0x1f
        || c == '\u{7f}'
        || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\')
}

/// `safe_filename`: the last path segment without a drive prefix, unsafe
/// characters (runs) replaced by `_`, at most 512 characters.
pub fn safe_filename(value: Option<&str>) -> String {
    let raw = value.filter(|v| !v.is_empty()).unwrap_or("unnamed").trim();
    let raw = raw.replace('\\', "/");
    let mut filename = raw.rsplit('/').next().unwrap_or("").trim().to_string();
    // _WINDOWS_DRIVE_PREFIX = ^[A-Za-z]:+
    let mut chars = filename.chars();
    if let (Some(first), Some(':')) = (chars.next(), chars.next())
        && first.is_ascii_alphabetic()
    {
        filename = filename[1..].trim_start_matches(':').trim().to_string();
    }
    let mut replaced = String::with_capacity(filename.len());
    let mut in_run = false;
    for c in filename.chars() {
        if unsafe_char(c) {
            if !in_run {
                replaced.push('_');
            }
            in_run = true;
        } else {
            replaced.push(c);
            in_run = false;
        }
    }
    let filename = replaced.trim_matches(|c| c == ' ' || c == '.');
    if filename.is_empty() || filename == "." || filename == ".." {
        return "unnamed".to_string();
    }
    filename.chars().take(512).collect()
}

pub fn plan_archive_entries(
    selected_folder_ids: &BTreeSet<String>,
    folders: &[ArchivePlanFolder],
    folder_files: &[ArchivePlanFile],
    selected_files: &[ArchivePlanFile],
) -> Vec<PlannedArchiveEntry> {
    let mut entries = Vec::new();
    let mut used_paths: HashSet<String> = HashSet::new();
    let mut included_file_ids: HashSet<String> = HashSet::new();
    let folder_by_id: HashMap<&str, &ArchivePlanFolder> =
        folders.iter().map(|f| (f.id.as_str(), f)).collect();

    if !selected_folder_ids.is_empty() {
        let mut files_by_folder: HashMap<Option<&str>, Vec<&ArchivePlanFile>> = HashMap::new();
        for file in folder_files {
            files_by_folder
                .entry(file.folder_id.as_deref())
                .or_default()
                .push(file);
        }
        let mut roots: Vec<&String> = selected_folder_ids.iter().collect();
        roots
            .sort_by_key(|id| folder_archive_path(&folder_by_id, Some(id.as_str())).to_lowercase());
        for folder_id in roots {
            let root_path = folder_archive_path(&folder_by_id, Some(folder_id));
            entries.push(PlannedArchiveEntry {
                archive_path: unique_archive_path(&mut used_paths, &root_path, true),
                file_id: None,
            });
            for child in archive_folder_descendants(folders, &folder_by_id, folder_id) {
                let folder_path = folder_archive_path(&folder_by_id, Some(&child.id));
                entries.push(PlannedArchiveEntry {
                    archive_path: unique_archive_path(&mut used_paths, &folder_path, true),
                    file_id: None,
                });
            }
            let tree = archive_folder_ids(folders, folder_id);
            let mut tree_files: Vec<&ArchivePlanFile> = tree
                .iter()
                .flat_map(|id| {
                    files_by_folder
                        .get(&Some(id.as_str()))
                        .cloned()
                        .unwrap_or_default()
                })
                .collect();
            tree_files.sort_by_key(|file| {
                (
                    folder_archive_path(&folder_by_id, file.folder_id.as_deref()).to_lowercase(),
                    file.filename.to_lowercase(),
                )
            });
            for file in tree_files {
                let folder_path = folder_archive_path(&folder_by_id, file.folder_id.as_deref());
                let path = format!("{folder_path}/{}", safe_filename(Some(&file.filename)));
                entries.push(PlannedArchiveEntry {
                    archive_path: unique_archive_path(&mut used_paths, &path, false),
                    file_id: Some(file.id.clone()),
                });
                included_file_ids.insert(file.id.clone());
            }
        }
    }

    for file in selected_files {
        if included_file_ids.contains(&file.id) {
            continue;
        }
        let path = safe_filename(Some(&file.filename));
        entries.push(PlannedArchiveEntry {
            archive_path: unique_archive_path(&mut used_paths, &path, false),
            file_id: Some(file.id.clone()),
        });
        included_file_ids.insert(file.id.clone());
    }
    entries
}

fn archive_folder_ids(folders: &[ArchivePlanFolder], root_id: &str) -> BTreeSet<String> {
    let mut children: HashMap<Option<&str>, Vec<&ArchivePlanFolder>> = HashMap::new();
    for folder in folders {
        children
            .entry(folder.parent_id.as_deref())
            .or_default()
            .push(folder);
    }
    let mut ids = BTreeSet::new();
    let mut stack = vec![root_id.to_string()];
    while let Some(current) = stack.pop() {
        if !ids.insert(current.clone()) {
            continue;
        }
        if let Some(kids) = children.get(&Some(current.as_str())) {
            stack.extend(kids.iter().map(|child| child.id.clone()));
        }
    }
    ids
}

fn archive_folder_descendants<'a>(
    folders: &'a [ArchivePlanFolder],
    folder_by_id: &HashMap<&str, &'a ArchivePlanFolder>,
    root_id: &str,
) -> Vec<&'a ArchivePlanFolder> {
    let mut ids = archive_folder_ids(folders, root_id);
    ids.remove(root_id);
    let mut result: Vec<&ArchivePlanFolder> = ids
        .iter()
        .filter_map(|id| folder_by_id.get(id.as_str()).copied())
        .collect();
    result.sort_by_key(|folder| folder_archive_path(folder_by_id, Some(&folder.id)).to_lowercase());
    result
}

fn folder_archive_path(
    folder_by_id: &HashMap<&str, &ArchivePlanFolder>,
    folder_id: Option<&str>,
) -> String {
    let Some(folder_id) = folder_id else {
        return String::new();
    };
    let mut segments = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = folder_by_id.get(folder_id).copied();
    while let Some(folder) = cursor {
        if !seen.insert(folder.id.as_str()) {
            break;
        }
        segments.push(safe_filename(Some(&folder.name)));
        cursor = folder
            .parent_id
            .as_deref()
            .and_then(|parent| folder_by_id.get(parent).copied());
    }
    segments.reverse();
    segments.join("/")
}

pub fn ensure_directory_path(path: &str) -> String {
    format!("{}/", path.trim_end_matches('/'))
}

/// pathlib's PurePosixPath parent/stem/suffix of `path`.
fn split_path(path: &str) -> (String, String, String) {
    let (parent, name) = match path.rfind('/') {
        Some(index) => (&path[..index], &path[index + 1..]),
        None => ("", path),
    };
    let (stem, suffix) = match name.rfind('.') {
        Some(index) if index > 0 && index < name.len() - 1 => (&name[..index], &name[index..]),
        _ => (name, ""),
    };
    let prefix = if parent.is_empty() {
        String::new()
    } else {
        format!("{parent}/")
    };
    (prefix, stem.to_string(), suffix.to_string())
}

fn unique_archive_path(used_paths: &mut HashSet<String>, path: &str, is_dir: bool) -> String {
    let normalized = if is_dir {
        ensure_directory_path(path)
    } else {
        path.to_string()
    };
    if used_paths.insert(normalized.clone()) {
        return normalized;
    }
    let base_path = if is_dir {
        normalized.trim_end_matches('/').to_string()
    } else {
        normalized
    };
    let (parent_prefix, stem, extension) = split_path(&base_path);
    let mut suffix = 2;
    loop {
        let candidate = format!("{parent_prefix}{stem} ({suffix}){extension}");
        let candidate = if is_dir {
            ensure_directory_path(&candidate)
        } else {
            candidate
        };
        if used_paths.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

#[cfg(test)]
mod tests {
    //! AI-DO apps/api/tests/test_file_archive_planner.py.
    use super::*;

    fn folder(id: &str, name: &str, parent_id: Option<&str>) -> ArchivePlanFolder {
        ArchivePlanFolder {
            id: id.into(),
            parent_id: parent_id.map(Into::into),
            name: name.into(),
        }
    }

    fn file(id: &str, filename: &str, folder_id: Option<&str>) -> ArchivePlanFile {
        ArchivePlanFile {
            id: id.into(),
            folder_id: folder_id.map(Into::into),
            filename: filename.into(),
        }
    }

    fn entry(path: &str, file_id: Option<&str>) -> PlannedArchiveEntry {
        PlannedArchiveEntry {
            archive_path: path.into(),
            file_id: file_id.map(Into::into),
        }
    }

    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn selected_folder_plans_directories_and_files_in_stable_path_order() {
        let entries = plan_archive_entries(
            &set(&["root"]),
            &[
                folder("root", "Project", None),
                folder("notes", "Notes", Some("root")),
                folder("assets", "Assets", Some("root")),
            ],
            &[
                file("readme", "readme.txt", Some("root")),
                file("asset", "logo.png", Some("assets")),
                file("note", "daily.txt", Some("notes")),
            ],
            &[],
        );
        assert_eq!(
            entries,
            vec![
                entry("Project/", None),
                entry("Project/Assets/", None),
                entry("Project/Notes/", None),
                entry("Project/readme.txt", Some("readme")),
                entry("Project/Assets/logo.png", Some("asset")),
                entry("Project/Notes/daily.txt", Some("note")),
            ]
        );
    }

    #[test]
    fn selected_folder_and_explicit_child_file_include_child_once() {
        let entries = plan_archive_entries(
            &set(&["root"]),
            &[folder("root", "Project", None)],
            &[file("nested", "nested.txt", Some("root"))],
            &[file("nested", "nested.txt", Some("root"))],
        );
        assert_eq!(
            entries,
            vec![
                entry("Project/", None),
                entry("Project/nested.txt", Some("nested"))
            ]
        );
    }

    #[test]
    fn colliding_sanitized_file_paths_get_deterministic_suffixes() {
        let entries = plan_archive_entries(
            &set(&[]),
            &[],
            &[],
            &[
                file("first", "..\\evil.txt", None),
                file("second", "C:/evil.txt", None),
                file("third", "evil.txt", None),
            ],
        );
        assert_eq!(
            entries,
            vec![
                entry("evil.txt", Some("first")),
                entry("evil (2).txt", Some("second")),
                entry("evil (3).txt", Some("third")),
            ]
        );
    }

    #[test]
    fn empty_folder_selection_still_emits_directory_entry() {
        let entries = plan_archive_entries(
            &set(&["empty"]),
            &[folder("empty", "Empty", None)],
            &[],
            &[],
        );
        assert_eq!(entries, vec![entry("Empty/", None)]);
    }

    #[test]
    fn duplicate_explicit_file_ids_are_deduped_in_stable_order() {
        let entries = plan_archive_entries(
            &set(&[]),
            &[],
            &[],
            &[
                file("first", "first.txt", None),
                file("first", "first.txt", None),
                file("second", "second.txt", None),
            ],
        );
        assert_eq!(
            entries,
            vec![
                entry("first.txt", Some("first")),
                entry("second.txt", Some("second"))
            ]
        );
    }

    #[test]
    fn safe_filename_matches_ai_do() {
        assert_eq!(safe_filename(Some("..\\evil.txt")), "evil.txt");
        assert_eq!(safe_filename(Some("C:evil.txt")), "evil.txt");
        assert_eq!(safe_filename(Some("a<>b?.txt")), "a_b_.txt");
        assert_eq!(safe_filename(Some("  ..  ")), "unnamed");
        assert_eq!(safe_filename(None), "unnamed");
        assert_eq!(safe_filename(Some("dir/")), "unnamed");
        assert_eq!(safe_filename(Some(&"가".repeat(600))).chars().count(), 512);
        assert_eq!(safe_filename(Some("보고서 v2.pdf")), "보고서 v2.pdf");
    }

    #[test]
    fn duplicate_directories_are_numbered_like_files() {
        let mut used = HashSet::new();
        assert_eq!(unique_archive_path(&mut used, "Docs", true), "Docs/");
        assert_eq!(unique_archive_path(&mut used, "Docs", true), "Docs (2)/");
        assert_eq!(unique_archive_path(&mut used, "a/.env", false), "a/.env");
        assert_eq!(
            unique_archive_path(&mut used, "a/.env", false),
            "a/.env (2)"
        );
    }
}
