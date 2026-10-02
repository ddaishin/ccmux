use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// ファイルツリーの並び順。どちらのモードでもディレクトリ群が先に来る。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    /// 更新日時(mtime)の降順。同時刻は名前の昇順、取得できない項目は末尾。
    #[default]
    Modified,
    /// 名前の昇順(大小無視)。従来の並び。
    Name,
}

impl SortMode {
    /// サイドバーの題に出す表記。ambiguous 幅の文字(矢印など)は使わない。
    pub fn label(self) -> &'static str {
        match self {
            SortMode::Modified => "更新順",
            SortMode::Name => "名前順",
        }
    }

    fn toggled(self) -> Self {
        match self {
            SortMode::Modified => SortMode::Name,
            SortMode::Name => SortMode::Modified,
        }
    }
}

/// A node in the file tree.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_expanded: bool,
    pub children: Vec<FileEntry>,
    pub depth: usize,
    /// 走査時の mtime。`s` での並べ直しを再走査なしで行うために保持する。
    /// 取得できなければ None(最古として扱う)。
    pub modified: Option<SystemTime>,
}

fn cmp_name(a: &FileEntry, b: &FileEntry) -> Ordering {
    a.name.to_lowercase().cmp(&b.name.to_lowercase())
}

/// 同じ群(ディレクトリ群 / ファイル群)の中を並べる。
fn sort_entries(entries: &mut [FileEntry], mode: SortMode) {
    match mode {
        SortMode::Name => entries.sort_by(cmp_name),
        // Option<SystemTime> は None < Some なので、b と a を比べれば
        // 新しい順・None は末尾になる
        SortMode::Modified => entries.sort_by(|a, b| {
            b.modified.cmp(&a.modified).then_with(|| cmp_name(a, b))
        }),
    }
}

/// ディレクトリ群を先、ファイル群を後にしたまま、それぞれの群を並べる。
fn sort_dirs_first(entries: &mut Vec<FileEntry>, mode: SortMode) {
    let (mut dirs, mut files): (Vec<_>, Vec<_>) =
        std::mem::take(entries).into_iter().partition(|e| e.is_dir);
    sort_entries(&mut dirs, mode);
    sort_entries(&mut files, mode);
    dirs.extend(files);
    *entries = dirs;
}

/// Scan a directory and return sorted entries (dirs first, then files, by `sort`).
/// Maximum entries per directory to prevent DoS from huge directories.
const MAX_ENTRIES_PER_DIR: usize = 500;

fn scan_directory_filtered(path: &Path, depth: usize, show_hidden: bool, sort: SortMode) -> Vec<FileEntry> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut result = Vec::new();
    let mut count = 0;

    for entry in entries.flatten() {
        if count >= MAX_ENTRIES_PER_DIR {
            break;
        }

        let entry_path = entry.path();
        let name = entry
            .file_name()
            .to_string_lossy()
            .to_string();

        // Always skip .git (too large and noisy)
        if name == ".git" {
            continue;
        }

        // Skip other hidden files/directories unless show_hidden is enabled
        if !show_hidden && name.starts_with('.') {
            continue;
        }

        // DirEntry::metadata() は symlink を辿らず、Windows では追加の
        // syscall も無い。symlink 判定・is_dir・mtime をこの 1 回で取る。
        let (is_dir, modified) = match entry.metadata() {
            Ok(meta) => {
                // Skip symlinks to prevent traversal outside the project
                if meta.is_symlink() {
                    continue;
                }
                (meta.is_dir(), meta.modified().ok())
            }
            // 取得失敗でも項目は残す(従来どおり)。mtime は最古扱い
            Err(_) => (entry_path.is_dir(), None),
        };

        // Don't recurse into subdirectories — children are loaded lazily on expand
        result.push(FileEntry {
            name,
            path: entry_path,
            is_dir,
            is_expanded: false,
            children: Vec::new(),
            depth,
            modified,
        });

        count += 1;
    }

    sort_dirs_first(&mut result, sort);
    result
}

/// File tree state for the sidebar.
/// Interval between automatic rescans of visible directories.
const AUTO_REFRESH_INTERVAL_SECS: u64 = 2;

#[allow(dead_code)]
pub struct FileTree {
    pub root_path: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub show_hidden: bool,
    /// 並び順(タブ単位・起動中のみ。`s` キーで切り替え)。
    pub sort_mode: SortMode,
    /// Flattened list of visible entries for rendering.
    flat_entries: Vec<FlatEntry>,
    /// Timestamp of the last automatic rescan.
    last_refresh: Instant,
}

/// A flattened entry for rendering.
#[derive(Debug, Clone)]
pub struct FlatEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_expanded: bool,
    pub depth: usize,
}

impl FileTree {
    /// Create a new file tree from a directory.
    pub fn new(root_path: PathBuf) -> Self {
        // Default: show hidden files (except .git)
        let sort_mode = SortMode::default();
        let entries = scan_directory_filtered(&root_path, 0, true, sort_mode);
        let mut tree = Self {
            root_path,
            entries,
            selected_index: 0,
            scroll_offset: 0,
            show_hidden: true,
            sort_mode,
            flat_entries: Vec::new(),
            last_refresh: Instant::now(),
        };
        tree.rebuild_flat();
        tree
    }

    /// Toggle showing hidden files and rescan.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.entries = scan_directory_filtered(&self.root_path, 0, self.show_hidden, self.sort_mode);
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.rebuild_flat();
    }

    /// 並び順を切り替える。保持している子を走査し直さずに並べ直し、
    /// 展開状態と選択中のパスを保つ(toggle_hidden のように先頭へ戻さない)。
    pub fn toggle_sort(&mut self) {
        self.set_sort_mode(self.sort_mode.toggled());
    }

    /// 並び順を指定して並べ直す(OSC 7 での作り直し時の引き継ぎにも使う)。
    pub fn set_sort_mode(&mut self, mode: SortMode) {
        if self.sort_mode == mode {
            return;
        }
        self.sort_mode = mode;
        let selected_path = self.selected_path();
        Self::resort_recursive(&mut self.entries, self.sort_mode);
        self.rebuild_flat();
        self.restore_selection(selected_path);
    }

    /// 折りたたまれていても保持している子はすべて並べ直す
    /// (再展開した直後に前のモードの順が見えないように)。
    fn resort_recursive(entries: &mut Vec<FileEntry>, mode: SortMode) {
        sort_dirs_first(entries, mode);
        for entry in entries.iter_mut() {
            Self::resort_recursive(&mut entry.children, mode);
        }
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.flat_entries
            .get(self.selected_index)
            .map(|e| e.path.clone())
    }

    /// 同じパスの項目へ選択を戻し、範囲外ならクランプする。
    fn restore_selection(&mut self, selected_path: Option<PathBuf>) {
        if let Some(ref path) = selected_path {
            if let Some(idx) = self.flat_entries.iter().position(|e| &e.path == path) {
                self.selected_index = idx;
            }
        }
        // Clamp selection
        if self.selected_index >= self.flat_entries.len() {
            self.selected_index = self.flat_entries.len().saturating_sub(1);
        }
    }

    /// Rebuild the flattened entry list from the tree structure.
    fn rebuild_flat(&mut self) {
        self.flat_entries.clear();
        for entry in &self.entries {
            Self::flatten_entry(entry, &mut self.flat_entries);
        }
    }

    fn flatten_entry(entry: &FileEntry, flat: &mut Vec<FlatEntry>) {
        flat.push(FlatEntry {
            name: entry.name.clone(),
            path: entry.path.clone(),
            is_dir: entry.is_dir,
            is_expanded: entry.is_expanded,
            depth: entry.depth,
        });

        if entry.is_dir && entry.is_expanded {
            for child in &entry.children {
                Self::flatten_entry(child, flat);
            }
        }
    }

    /// Get the flattened list of visible entries.
    pub fn visible_entries(&self) -> &[FlatEntry] {
        &self.flat_entries
    }

    /// Get the currently selected entry.
    /// Unlike `toggle_or_select`, this also returns directories — the `o`
    /// key opens whatever is selected in the OS default handler.
    pub fn selected_entry(&self) -> Option<&FlatEntry> {
        self.flat_entries.get(self.selected_index)
    }

    /// Move selection up.
    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection down.
    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.flat_entries.len() {
            self.selected_index += 1;
        }
    }

    /// Handle Enter key on selected entry.
    /// Returns Some(path) if a file was selected for preview.
    pub fn toggle_or_select(&mut self) -> Option<PathBuf> {
        if let Some(flat_entry) = self.flat_entries.get(self.selected_index).cloned() {
            if flat_entry.is_dir {
                // Toggle expand/collapse
                self.toggle_dir(&flat_entry.path);
                self.rebuild_flat();
                None
            } else {
                // Return file path for preview
                Some(flat_entry.path)
            }
        } else {
            None
        }
    }

    /// Toggle a directory's expanded state.
    fn toggle_dir(&mut self, path: &Path) {
        let show_hidden = self.show_hidden;
        let sort = self.sort_mode;
        for entry in &mut self.entries {
            if Self::toggle_dir_recursive(entry, path, show_hidden, sort) {
                return;
            }
        }
    }

    fn toggle_dir_recursive(entry: &mut FileEntry, path: &Path, show_hidden: bool, sort: SortMode) -> bool {
        if entry.path == path && entry.is_dir {
            entry.is_expanded = !entry.is_expanded;
            if entry.is_expanded && entry.children.is_empty() {
                entry.children = scan_directory_filtered(&entry.path, entry.depth + 1, show_hidden, sort);
            }
            return true;
        }
        for child in &mut entry.children {
            if Self::toggle_dir_recursive(child, path, show_hidden, sort) {
                return true;
            }
        }
        false
    }

    /// When the next auto-refresh is due (for event-loop scheduling).
    pub fn next_refresh_at(&self) -> Instant {
        self.last_refresh + Duration::from_secs(AUTO_REFRESH_INTERVAL_SECS)
    }

    /// Check if it's time to auto-refresh and do so if needed.
    /// Returns true if the tree was updated.
    pub fn auto_refresh_if_needed(&mut self) -> bool {
        if self.last_refresh.elapsed().as_secs() < AUTO_REFRESH_INTERVAL_SECS {
            return false;
        }
        self.last_refresh = Instant::now();

        // Remember the selected path so we can restore selection after rescan.
        let selected_path = self.selected_path();

        // Rescan root level
        let new_root = scan_directory_filtered(&self.root_path, 0, self.show_hidden, self.sort_mode);
        // Merge: preserve expanded state and children of existing dirs.
        let old_entries = std::mem::take(&mut self.entries);
        self.entries = Self::merge_entries(old_entries, new_root, self.show_hidden, self.sort_mode);
        self.rebuild_flat();

        self.restore_selection(selected_path);

        true
    }

    /// Merge new scan results with existing entries, preserving expanded state.
    /// 並びは新しいスキャン結果の順を保つ(並べる責務は scan 側にある)。
    fn merge_entries(mut old: Vec<FileEntry>, new: Vec<FileEntry>, show_hidden: bool, sort: SortMode) -> Vec<FileEntry> {
        new.into_iter()
            .map(|mut new_entry| {
                if new_entry.is_dir {
                    // Find and remove matching old entry to take ownership
                    if let Some(idx) = old.iter().position(|o| o.path == new_entry.path) {
                        let old_entry = old.swap_remove(idx);
                        if old_entry.is_expanded {
                            // Preserve expanded state, rescan children
                            new_entry.is_expanded = true;
                            let new_children = scan_directory_filtered(
                                &new_entry.path,
                                new_entry.depth + 1,
                                show_hidden,
                                sort,
                            );
                            new_entry.children = Self::merge_entries(
                                old_entry.children,
                                new_children,
                                show_hidden,
                                sort,
                            );
                        }
                    }
                }
                new_entry
            })
            .collect()
    }

    /// Adjust scroll offset to keep selected item visible.
    pub fn ensure_visible(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }
        if self.selected_index >= self.scroll_offset + visible_height {
            self.scroll_offset = self.selected_index - visible_height + 1;
        }
    }

    /// Scroll up by amount.
    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    /// Scroll down by amount.
    pub fn scroll_down(&mut self, amount: usize) {
        let max_offset = self.flat_entries.len().saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + amount).min(max_offset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_directory_skips_hidden() {
        let entries = scan_directory_filtered(Path::new("."), 0, false, SortMode::Name);
        for entry in &entries {
            assert!(
                !entry.name.starts_with('.'),
                "Hidden entry should be skipped: {}",
                entry.name
            );
        }
    }

    #[test]
    fn test_scan_directory_skips_git() {
        let entries = scan_directory_filtered(Path::new("."), 0, true, SortMode::Name);
        for entry in &entries {
            assert!(
                entry.name != ".git",
                ".git should always be skipped"
            );
        }
    }

    #[test]
    fn test_scan_directory_shows_dotfiles_when_enabled() {
        let entries = scan_directory_filtered(Path::new("."), 0, true, SortMode::Name);
        let has_dotfile = entries.iter().any(|e| e.name.starts_with('.'));
        // Project has .claude, .gitignore, etc.
        assert!(has_dotfile, "Should show dotfiles when show_hidden=true");
    }

    #[test]
    fn test_scan_directory_dirs_before_files() {
        let entries = scan_directory_filtered(Path::new("."), 0, false, SortMode::Name);
        let mut seen_file = false;
        for entry in &entries {
            if !entry.is_dir {
                seen_file = true;
            }
            if entry.is_dir && seen_file {
                panic!("Directory {} found after files", entry.name);
            }
        }
    }

    #[test]
    fn test_file_tree_navigation() {
        let mut tree = FileTree::new(PathBuf::from("."));
        let initial = tree.selected_index;
        assert_eq!(initial, 0);

        if tree.visible_entries().len() > 1 {
            tree.move_down();
            assert_eq!(tree.selected_index, 1);
            tree.move_up();
            assert_eq!(tree.selected_index, 0);
        }

        // Moving up at 0 should stay at 0
        tree.move_up();
        assert_eq!(tree.selected_index, 0);
    }

    /// 並べ替えテスト用の合成エントリ。secs は UNIX_EPOCH からの秒。
    fn synth(name: &str, is_dir: bool, secs: Option<u64>) -> FileEntry {
        FileEntry {
            name: name.to_string(),
            path: PathBuf::from(name),
            is_dir,
            is_expanded: false,
            children: Vec::new(),
            depth: 0,
            modified: secs.map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s)),
        }
    }

    fn names(entries: &[FileEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn test_sort_modified_desc() {
        let mut v = vec![
            synth("old.txt", false, Some(100)),
            synth("olddir", true, Some(100)),
            synth("new.txt", false, Some(300)),
            synth("mid.txt", false, Some(200)),
            synth("newdir", true, Some(300)),
        ];
        sort_dirs_first(&mut v, SortMode::Modified);
        assert_eq!(names(&v), ["newdir", "olddir", "new.txt", "mid.txt", "old.txt"]);
    }

    #[test]
    fn test_sort_same_mtime_by_name() {
        let mut v = vec![
            synth("b.txt", false, Some(100)),
            synth("A.txt", false, Some(100)),
            synth("c.txt", false, Some(100)),
        ];
        sort_dirs_first(&mut v, SortMode::Modified);
        assert_eq!(names(&v), ["A.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn test_sort_none_mtime_last() {
        let mut v = vec![
            synth("unknown.txt", false, None),
            synth("unknowndir", true, None),
            synth("old.txt", false, Some(1)),
            synth("dir", true, Some(1)),
        ];
        sort_dirs_first(&mut v, SortMode::Modified);
        // None は各群の末尾(ディレクトリ群の末尾はファイル群より前)
        assert_eq!(names(&v), ["dir", "unknowndir", "old.txt", "unknown.txt"]);
    }

    #[test]
    fn test_sort_name_mode_matches_legacy() {
        let input = vec![
            synth("zeta.txt", false, Some(300)),
            synth("Beta", true, Some(100)),
            synth("alpha.txt", false, Some(100)),
            synth("Alpha", true, Some(300)),
            synth("Gamma.txt", false, None),
        ];
        // 従来の実装(ディレクトリ先頭・名前昇順・大小無視)をそのまま再現した期待値
        let mut dirs: Vec<_> = input.iter().filter(|e| e.is_dir).cloned().collect();
        let mut files: Vec<_> = input.iter().filter(|e| !e.is_dir).cloned().collect();
        dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        dirs.extend(files);

        let mut v = input;
        sort_dirs_first(&mut v, SortMode::Name);
        assert_eq!(names(&v), names(&dirs));
    }

    #[test]
    fn test_toggle_sort_keeps_selection_and_expansion() {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "ccmux-sort-test-{}-{}",
            std::process::id(),
            nanos
        ));
        // assert が失敗しても一時ディレクトリを残さない
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let sub = root.join("sub");
        fs::create_dir_all(&sub).unwrap();
        // 名前順(a, b)と mtime 順(b が新しい)が逆になるように置く。
        // 中身を作り終えてから、書き込み可能なハンドルでファイルにだけ時刻を付ける
        // (Windows の SetFileTime は FILE_WRITE_ATTRIBUTES が要る)。
        let now = SystemTime::now();
        for (name, age) in [("a.txt", 1000u64), ("b.txt", 0u64)] {
            let f = fs::File::create(sub.join(name)).unwrap();
            f.set_modified(now - Duration::from_secs(age)).unwrap();
        }

        let mut tree = FileTree::new(root.clone());
        assert_eq!(tree.sort_mode, SortMode::Modified);

        // sub を展開して a.txt を選ぶ
        let sub_idx = tree.visible_entries().iter().position(|e| e.path == sub).unwrap();
        tree.selected_index = sub_idx;
        tree.toggle_or_select();
        let child_names = |t: &FileTree| -> Vec<String> {
            t.visible_entries()
                .iter()
                .filter(|e| e.depth == 1)
                .map(|e| e.name.clone())
                .collect()
        };
        assert_eq!(child_names(&tree), ["b.txt", "a.txt"]);
        let a_path = sub.join("a.txt");
        tree.selected_index = tree.visible_entries().iter().position(|e| e.path == a_path).unwrap();

        // 切り替え: 子の階層で順序が反転し、展開と選択パスは保たれる
        tree.toggle_sort();
        assert_eq!(tree.sort_mode, SortMode::Name);
        assert_eq!(child_names(&tree), ["a.txt", "b.txt"]);
        assert_eq!(tree.selected_entry().map(|e| e.path.clone()), Some(a_path.clone()));
        assert!(tree.visible_entries().iter().any(|e| e.path == sub && e.is_expanded));

        // 折りたたんだまま切り替えても、保持している子は並べ直される
        let sub_idx = tree.visible_entries().iter().position(|e| e.path == sub).unwrap();
        tree.selected_index = sub_idx;
        tree.toggle_or_select();
        tree.toggle_sort();
        tree.toggle_or_select();
        assert_eq!(tree.sort_mode, SortMode::Modified);
        assert_eq!(child_names(&tree), ["b.txt", "a.txt"]);

        // 同じモードの指定は何もしない(OSC 7 の引き継ぎで使う)
        tree.set_sort_mode(SortMode::Modified);
        assert_eq!(child_names(&tree), ["b.txt", "a.txt"]);
        tree.set_sort_mode(SortMode::Name);
        assert_eq!(child_names(&tree), ["a.txt", "b.txt"]);
    }

    #[test]
    fn test_sort_mode_label() {
        // サイドバーの題 " FILES <label> " に出す表記(requirements の EARS)
        assert_eq!(SortMode::default(), SortMode::Modified);
        assert_eq!(SortMode::Modified.label(), "更新順");
        assert_eq!(SortMode::Name.label(), "名前順");
    }
}
