use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

/// Git 文件状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GitStatus {
    Modified,  // M - 修改
    Added,     // A - 新增
    Deleted,   // D - 删除
    Untracked, // ?? - 未跟踪
    Renamed,   // R - 重命名
    Copied,    // C - 复制
    Unmerged,  // U - 未合并
    Ignored,   // ! - 忽略
    Clean,     // 无状态
}

impl GitStatus {
    /// 从 git status --porcelain 状态码解析
    pub fn from_code(code: &str) -> Self {
        match code {
            " M" => GitStatus::Modified,
            "M " => GitStatus::Modified,
            "MM" => GitStatus::Modified,
            "A " => GitStatus::Added,
            "D " => GitStatus::Deleted,
            " D" => GitStatus::Deleted,
            "??" => GitStatus::Untracked,
            "R " => GitStatus::Renamed,
            "C " => GitStatus::Copied,
            "U " => GitStatus::Unmerged,
            "UU" => GitStatus::Unmerged,
            "!!" => GitStatus::Ignored,
            _ => GitStatus::Clean,
        }
    }

    /// 获取状态符号
    pub fn symbol(&self) -> &'static str {
        match self {
            GitStatus::Modified => "M",
            GitStatus::Added => "A",
            GitStatus::Deleted => "D",
            GitStatus::Untracked => "??",
            GitStatus::Renamed => "R",
            GitStatus::Copied => "C",
            GitStatus::Unmerged => "U",
            GitStatus::Ignored => "!",
            GitStatus::Clean => " ",
        }
    }

    /// 获取状态优先级（用于文件夹状态传播）
    pub fn priority(&self) -> i32 {
        match self {
            GitStatus::Modified => 10,
            GitStatus::Added => 9,
            GitStatus::Deleted => 8,
            GitStatus::Renamed => 7,
            GitStatus::Copied => 6,
            GitStatus::Unmerged => 5,
            GitStatus::Untracked => 4,
            GitStatus::Ignored => 1,
            GitStatus::Clean => 0,
        }
    }
}

/// 文件信息
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub status: GitStatus,
    pub is_dir: bool,
    pub depth: usize,
}

/// Git 状态管理器
pub struct GitStatusManager {
    working_dir: String,
    is_git_repo: bool,
    /// 记录已展开的目录相对路径（用于懒加载）
    expanded_dirs: HashSet<String>,
}

#[derive(Debug, Clone)]
struct StatusEntry {
    path: String,
    status: GitStatus,
}

impl GitStatusManager {
    pub fn new(working_dir: &str) -> Self {
        let is_git_repo = Self::detect_git_repo(Path::new(working_dir));

        Self {
            working_dir: working_dir.to_string(),
            is_git_repo,
            expanded_dirs: HashSet::new(),
        }
    }

    /// 向上遍历祖先目录查找 .git
    fn detect_git_repo(path: &Path) -> bool {
        Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(path)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// 添加展开的目录
    pub fn expand_dir(&mut self, path: &str) {
        self.expanded_dirs.insert(path.to_string());
    }

    /// 移除展开的目录
    pub fn collapse_dir(&mut self, path: &str) {
        self.expanded_dirs.remove(path);
    }

    /// 检查是否是 Git 仓库
    pub fn is_git_repo(&self) -> bool {
        self.is_git_repo
    }

    /// 获取 Git 状态
    pub fn get_status(&self) -> Result<Vec<FileEntry>> {
        // 首先获取所有文件
        let mut entries = self.get_all_files()?;

        if !self.is_git_repo {
            // 非 git 仓库，所有文件默认为 Untracked
            for entry in &mut entries {
                entry.status = GitStatus::Untracked;
            }
            // 文件夹状态传播（虽然都是 Untracked，但为了统一逻辑）
            self.propagate_statuses(&mut entries);
            return Ok(entries);
        }

        // VSCode 类似策略：仅请求当前目录（pathspec = .）下状态，减少大仓库扫描开销。
        let output = Command::new("git")
            .args([
                "status",
                "--porcelain",
                "-u",
                "--ignored=matching",
                "--renames",
                "--",
                ".",
            ])
            .current_dir(&self.working_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Git 命令失败：{}", stderr);
        }

        let mut git_status_map: HashMap<String, GitStatus> = HashMap::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut status_entries = Vec::new();

        for line in stdout.lines() {
            if let Some(parsed) = Self::parse_status_line(line) {
                git_status_map.insert(parsed.path.clone(), parsed.status);
                status_entries.push(parsed);
            }
        }

        // 更新已扫描节点的状态
        for entry in &mut entries {
            if let Some(&status) = git_status_map.get(&entry.path) {
                entry.status = status;
            }
        }

        // 关键修复：即使子目录未展开，也把 git 输出中的子路径状态向已显示父目录提升。
        self.apply_status_from_git_paths(&mut entries, &status_entries);
        self.propagate_statuses(&mut entries);

        Ok(entries)
    }

    fn parse_status_line(line: &str) -> Option<StatusEntry> {
        if line.len() < 4 {
            return None;
        }

        let status_code = &line[0..2];
        let mut file_path = line[3..].trim().to_string();

        // 重命名/复制格式：old -> new，使用 new 路径。
        if let Some((_, new_path)) = file_path.rsplit_once(" -> ") {
            file_path = new_path.to_string();
        }

        let file_path = file_path
            .trim_matches('"')
            .trim_end_matches('/')
            .to_string();

        if file_path.is_empty() {
            return None;
        }

        Some(StatusEntry {
            path: file_path,
            status: GitStatus::from_code(status_code),
        })
    }

    fn apply_status_from_git_paths(&self, entries: &mut [FileEntry], statuses: &[StatusEntry]) {
        let mut path_index: HashMap<String, usize> = HashMap::new();
        for (idx, entry) in entries.iter().enumerate() {
            path_index.insert(entry.path.clone(), idx);
        }

        for status_entry in statuses {
            if status_entry.status == GitStatus::Ignored {
                continue;
            }

            let mut current = Path::new(&status_entry.path);
            while let Some(parent) = current.parent() {
                let parent_path = parent.to_string_lossy();
                if parent_path.is_empty() || parent_path == "." {
                    break;
                }

                if let Some(&idx) = path_index.get(parent_path.as_ref()) {
                    if entries[idx].status.priority() < status_entry.status.priority() {
                        entries[idx].status = status_entry.status;
                    }
                }

                current = parent;
            }
        }
    }

    /// 核心逻辑：将子目录/文件的状态传播到父目录
    fn propagate_statuses(&self, entries: &mut Vec<FileEntry>) {
        use std::collections::HashMap;

        // children 关系：parent -> Vec<child_index>
        let mut children_map: HashMap<String, Vec<usize>> = HashMap::new();

        for (i, entry) in entries.iter().enumerate() {
            if let Some(parent) = std::path::Path::new(&entry.path).parent() {
                let parent_str = parent.to_string_lossy().replace('\\', "/");
                if !parent_str.is_empty() && parent_str != "." {
                    children_map.entry(parent_str).or_default().push(i);
                }
            }
        }

        // 按 depth 从深到浅排序 index
        let mut indices: Vec<usize> = (0..entries.len()).collect();
        indices.sort_by_key(|&i| std::cmp::Reverse(entries[i].depth));

        // 目录聚合状态
        for i in indices {
            if !entries[i].is_dir {
                continue;
            }

            let mut strongest_status = entries[i].status;

            if let Some(children) = children_map.get(&entries[i].path) {
                for &child_idx in children {
                    let child_status = entries[child_idx].status;

                    // 🚨 Ignored 不向上传播
                    if child_status == GitStatus::Ignored {
                        continue;
                    }

                    if child_status.priority() > strongest_status.priority() {
                        strongest_status = child_status;
                    }
                }
            }

            entries[i].status = strongest_status;
        }
    }

    /// 获取所有文件（包括已跟踪和未跟踪的）
    fn get_all_files(&self) -> Result<Vec<FileEntry>> {
        let mut entries = Vec::new();
        self.collect_all_files(Path::new(&self.working_dir), 0, &mut entries)?;
        Ok(entries)
    }

    /// 递归收集所有文件
    fn collect_all_files(
        &self,
        dir: &Path,
        depth: usize,
        entries: &mut Vec<FileEntry>,
    ) -> Result<()> {
        // 仅跳过 .git 内部目录
        if dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == ".git")
            .unwrap_or(false)
        {
            return Ok(());
        }

        if let Ok(read_dir) = std::fs::read_dir(dir) {
            let mut dir_entries: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();

            // 目录优先排序
            dir_entries.sort_by(|a, b| {
                let a_path = a.path();
                let b_path = b.path();
                let a_is_dir = a_path.is_dir();
                let b_is_dir = b_path.is_dir();
                match (a_is_dir, b_is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a_path.file_name().cmp(&b_path.file_name()),
                }
            });

            for entry in dir_entries {
                let path = entry.path();
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                // 仅跳过 .git 目录本身
                if file_name == ".git" {
                    continue;
                }

                let relative_path = path
                    .strip_prefix(&self.working_dir)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"));

                let relative_path_str = relative_path.trim_start_matches('/').to_string();
                if relative_path_str.is_empty() {
                    continue;
                }

                let is_dir = path.is_dir();

                entries.push(FileEntry {
                    path: relative_path_str.clone(),
                    status: GitStatus::Clean,
                    is_dir,
                    depth,
                });

                // 性能优化：目录懒加载
                // 只有根目录下的直接子项，或者是已被展开的目录，才继续递归扫描
                if is_dir && (depth == 0 || self.expanded_dirs.contains(&relative_path_str)) {
                    self.collect_all_files(&path, depth + 1, entries)?;
                }
            }
        }
        Ok(())
    }

    /// 获取文件级别的 diff 行信息（用于 VSCode 风格 gutter 标记）
    pub fn get_file_diff_lines(&self, file_path: &str) -> HashMap<usize, DiffLineType> {
        let mut result = HashMap::new();

        if !self.is_git_repo {
            return result;
        }

        let output = Command::new("git")
            .args(["diff", "--unified=0", "--no-color", "--", file_path])
            .current_dir(&self.working_dir)
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.starts_with("@@") {
                    // 解析 @@ -old_start,old_count +new_start,new_count @@
                    let parts: Vec<&str> = line.split("@@").collect();
                    if parts.len() < 2 {
                        continue;
                    }

                    let range_info = parts[1].trim();
                    let mut old_count: usize = 0;
                    let mut new_start: usize = 0;
                    let mut new_count: usize = 0;

                    // 解析 -old_start,old_count
                    if let Some(old_part) = range_info.split('+').next() {
                        let old_part = old_part.trim().trim_start_matches('-');
                        let old_parts: Vec<&str> = old_part.split(',').collect();
                        old_count = old_parts
                            .get(1)
                            .and_then(|s| s.trim().parse().ok())
                            .unwrap_or(1);
                    }

                    // 解析 +new_start,new_count
                    if let Some(new_part) = range_info.split('+').nth(1) {
                        let new_parts: Vec<&str> = new_part.trim().split(',').collect();
                        new_start = new_parts
                            .first()
                            .and_then(|s| s.trim().parse().ok())
                            .unwrap_or(0);
                        new_count = new_parts
                            .get(1)
                            .and_then(|s| s.trim().parse().ok())
                            .unwrap_or(1);
                    }

                    if new_count == 0 && old_count > 0 {
                        // 纯删除：在 new_start 行标记删除
                        if new_start > 0 {
                            result.insert(new_start, DiffLineType::Deleted);
                        }
                    } else if old_count == 0 && new_count > 0 {
                        // 纯新增
                        for i in 0..new_count {
                            result.insert(new_start + i, DiffLineType::Added);
                        }
                    } else {
                        // 修改（有删有增）
                        for i in 0..new_count {
                            result.insert(new_start + i, DiffLineType::Modified);
                        }
                    }
                }
            }
        }

        result
    }
}

/// Diff 行类型（用于 gutter 标记）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiffLineType {
    Added,    // 新增行
    Modified, // 修改行
    Deleted,  // 删除行标记
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_status_from_code() {
        assert_eq!(GitStatus::from_code(" M"), GitStatus::Modified);
        assert_eq!(GitStatus::from_code("A "), GitStatus::Added);
        assert_eq!(GitStatus::from_code("??"), GitStatus::Untracked);
        assert_eq!(GitStatus::from_code("D "), GitStatus::Deleted);
    }

    #[test]
    fn test_parse_status_line_rename() {
        let parsed =
            GitStatusManager::parse_status_line("R  old/file.txt -> new/file.txt").unwrap();
        assert_eq!(parsed.path, "new/file.txt");
        assert_eq!(parsed.status, GitStatus::Renamed);
    }

    #[test]
    fn test_git_status_symbol() {
        assert_eq!(GitStatus::Modified.symbol(), "M");
        assert_eq!(GitStatus::Untracked.symbol(), "??");
    }
}
