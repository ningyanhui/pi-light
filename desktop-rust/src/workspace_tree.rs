// SPDX-License-Identifier: Apache-2.0
//
// 当前活动工作区的只读文件树。默认忽略常见巨型目录；不内嵌编辑器。

use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
};

/// 默认忽略的目录名（大小写敏感匹配最后一段）。
pub const DEFAULT_IGNORED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    "out",
    "__pycache__",
    ".venv",
    "venv",
    ".next",
    ".turbo",
    ".cache",
    "coverage",
    ".idea",
    ".vs",
    "Pods",
    "DerivedData",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeStatus {
    Ready,
    Empty,
    Missing,
    NoPermission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    IgnoredDir,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
}

#[derive(Debug, Clone)]
pub struct FileTree {
    root: PathBuf,
    status: TreeStatus,
    expanded: HashSet<PathBuf>,
    cache: HashMap<PathBuf, Vec<TreeEntry>>,
}

impl Default for FileTree {
    fn default() -> Self {
        Self {
            root: PathBuf::new(),
            status: TreeStatus::Missing,
            expanded: HashSet::new(),
            cache: HashMap::new(),
        }
    }
}

impl FileTree {
    pub fn load(root: PathBuf) -> Self {
        let mut tree = Self {
            root,
            status: TreeStatus::Missing,
            expanded: HashSet::new(),
            cache: HashMap::new(),
        };
        tree.reload();
        tree
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn status(&self) -> TreeStatus {
        self.status
    }

    pub fn reload(&mut self) {
        self.cache.clear();
        self.expanded.clear();
        match read_root(&self.root) {
            Ok(entries) => {
                self.status = if entries.is_empty() {
                    TreeStatus::Empty
                } else {
                    TreeStatus::Ready
                };
                if self.status == TreeStatus::Ready {
                    self.expanded.insert(self.root.clone());
                }
                self.cache.insert(self.root.clone(), entries);
            }
            Err(kind) => {
                self.status = kind;
            }
        }
    }

    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded.contains(path)
    }

    pub fn toggle(&mut self, path: &Path) {
        if !self.expanded.remove(path) {
            self.ensure_children(path);
            self.expanded.insert(path.to_path_buf());
        }
    }

    pub fn children(&mut self, path: &Path) -> &[TreeEntry] {
        self.ensure_children(path);
        self.cache.get(path).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn relative_display(root: &Path, path: &Path) -> String {
        match path.strip_prefix(root) {
            Ok(rel) if !rel.as_os_str().is_empty() => unix_rel(rel),
            _ => unix_rel(path),
        }
    }

    /// 供 `@` 唤起：浅层遍历可见文件，忽略巨型目录。不打开编辑器。
    pub fn collect_file_rels(&mut self, limit: usize) -> Vec<String> {
        if !matches!(self.status, TreeStatus::Ready) {
            return Vec::new();
        }
        let root = self.root.clone();
        let mut out = Vec::new();
        collect_rels(self, &root, &root, &mut out, limit, 0);
        out
    }

    fn ensure_children(&mut self, path: &Path) {
        if self.cache.contains_key(path) {
            return;
        }
        let entries = match fs::read_dir(path) {
            Ok(rd) => collect_entries(path, rd),
            Err(_) => Vec::new(),
        };
        self.cache.insert(path.to_path_buf(), entries);
    }
}

pub fn is_ignored_name(name: &str) -> bool {
    DEFAULT_IGNORED_DIRS.contains(&name)
}

fn read_root(root: &Path) -> Result<Vec<TreeEntry>, TreeStatus> {
    if !root.exists() {
        return Err(TreeStatus::Missing);
    }
    match fs::read_dir(root) {
        Ok(rd) => Ok(collect_entries(root, rd)),
        Err(err) => Err(classify_read_error(&err)),
    }
}

fn classify_read_error(err: &io::Error) -> TreeStatus {
    match err.kind() {
        io::ErrorKind::NotFound => TreeStatus::Missing,
        io::ErrorKind::PermissionDenied => TreeStatus::NoPermission,
        _ => {
            let raw = err.raw_os_error();
            if raw == Some(13) || raw == Some(5) {
                TreeStatus::NoPermission
            } else {
                TreeStatus::Missing
            }
        }
    }
}

fn collect_entries(parent: &Path, rd: fs::ReadDir) -> Vec<TreeEntry> {
    let mut entries: Vec<TreeEntry> = rd
        .filter_map(|item| item.ok())
        .map(|item| {
            let name = item.file_name().to_string_lossy().into_owned();
            let path = item.path();
            let is_dir = item.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let kind = if is_dir {
                if is_ignored_name(&name) {
                    EntryKind::IgnoredDir
                } else {
                    EntryKind::Dir
                }
            } else {
                EntryKind::File
            };
            TreeEntry { name, path, kind }
        })
        .filter(|entry| {
            // 隐藏文件仍显示（如 .gitignore），只折叠已知巨型目录。
            parent.join(&entry.name) == entry.path
        })
        .collect();
    entries.sort_by(|a, b| {
        let dir_a = !matches!(a.kind, EntryKind::File);
        let dir_b = !matches!(b.kind, EntryKind::File);
        dir_b.cmp(&dir_a).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
    });
    entries
}

fn collect_rels(
    tree: &mut FileTree,
    root: &Path,
    dir: &Path,
    out: &mut Vec<String>,
    limit: usize,
    depth: usize,
) {
    if out.len() >= limit || depth > 8 {
        return;
    }
    let entries = tree.children(dir).to_vec();
    for entry in entries {
        if out.len() >= limit {
            break;
        }
        match entry.kind {
            EntryKind::File => out.push(FileTree::relative_display(root, &entry.path)),
            EntryKind::Dir => collect_rels(tree, root, &entry.path, out, limit, depth + 1),
            EntryKind::IgnoredDir => {}
        }
    }
}

fn unix_rel(path: &Path) -> String {
    path.iter()
        .map(|c| c.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn ignore_list_covers_heavy_dirs() {
        assert!(is_ignored_name("node_modules"));
        assert!(is_ignored_name(".git"));
        assert!(is_ignored_name("target"));
        assert!(!is_ignored_name("src"));
    }

    #[test]
    fn load_missing_path_is_honest() {
        let tree = FileTree::load(PathBuf::from("/definitely/not/a/pi-light-tree-xyz"));
        assert_eq!(tree.status(), TreeStatus::Missing);
    }

    #[test]
    fn load_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let tree = FileTree::load(dir.path().to_path_buf());
        assert_eq!(tree.status(), TreeStatus::Empty);
    }

    #[test]
    fn expand_two_levels_and_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let ui = src.join("ui");
        fs::create_dir_all(&ui).unwrap();
        fs::write(ui.join("dock.rs"), "fn x() {}\n").unwrap();
        fs::create_dir_all(dir.path().join("node_modules").join("pkg")).unwrap();
        fs::write(dir.path().join("README.md"), "hi\n").unwrap();

        let mut tree = FileTree::load(dir.path().to_path_buf());
        assert_eq!(tree.status(), TreeStatus::Ready);
        let roots = tree.children(dir.path()).to_vec();
        assert!(roots
            .iter()
            .any(|e| e.name == "src" && e.kind == EntryKind::Dir));
        assert!(roots
            .iter()
            .any(|e| e.name == "node_modules" && e.kind == EntryKind::IgnoredDir));
        assert!(roots
            .iter()
            .any(|e| e.name == "README.md" && e.kind == EntryKind::File));

        let src_entry = roots.iter().find(|e| e.name == "src").unwrap();
        tree.toggle(&src_entry.path);
        assert!(tree.is_expanded(&src_entry.path));
        let src_kids = tree.children(&src_entry.path).to_vec();
        let ui_entry = src_kids.iter().find(|e| e.name == "ui").unwrap();
        tree.toggle(&ui_entry.path);
        let files = tree.children(&ui_entry.path).to_vec();
        let dock = files.iter().find(|e| e.name == "dock.rs").unwrap();
        assert_eq!(
            FileTree::relative_display(dir.path(), &dock.path),
            "src/ui/dock.rs"
        );
    }

    #[test]
    fn ignored_dir_is_not_expanded_by_default() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("node_modules").join("x")).unwrap();
        let tree = FileTree::load(dir.path().to_path_buf());
        let nm = dir.path().join("node_modules");
        assert!(!tree.is_expanded(&nm));
        assert!(!tree.is_expanded(&nm));
    }

    #[test]
    fn collect_file_rels_skips_ignored() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src").join("main.rs"), "fn main() {}\n").unwrap();
        fs::create_dir_all(dir.path().join("node_modules").join("pkg")).unwrap();
        fs::write(
            dir.path().join("node_modules").join("pkg").join("x.js"),
            "1\n",
        )
        .unwrap();
        let mut tree = FileTree::load(dir.path().to_path_buf());
        let files = tree.collect_file_rels(32);
        assert!(files.iter().any(|p| p == "src/main.rs"));
        assert!(!files.iter().any(|p| p.contains("node_modules")));
    }
}
