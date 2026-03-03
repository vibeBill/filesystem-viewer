use std::process::Command;
use std::path::Path;
use std::collections::HashMap;
use anyhow::Result;

/// Git 文件状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GitStatus {
    Modified,      // M - 修改
    Added,         // A - 新增
    Deleted,       // D - 删除
    Untracked,     // ?? - 未跟踪
    Renamed,       // R - 重命名
    Copied,        // C - 复制
    Unmerged,      // U - 未合并
    Ignored,       // ! - 忽略
    Clean,         // 无状态
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
}

impl GitStatusManager {
    pub fn new(working_dir: &str) -> Self {
        let is_git_repo = Path::new(working_dir)
            .join(".git")
            .exists();

        Self {
            working_dir: working_dir.to_string(),
            is_git_repo,
        }
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

        // 获取 Git 状态
        let output = Command::new("git")
            .args(["status", "--porcelain", "-u", "--renames"])
            .current_dir(&self.working_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Git 命令失败：{}", stderr);
        }

        // 构建 Git 状态映射
        let mut git_status_map: HashMap<String, GitStatus> = HashMap::new();
        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            if line.len() < 4 {
                continue;
            }

            let status_code = &line[0..2];
            let file_path = line[3..].trim_matches('"').to_string(); // 处理带引号的路径

            if file_path.is_empty() {
                continue;
            }

            let status = GitStatus::from_code(status_code);
            git_status_map.insert(file_path, status);
        }

        // 更新文件条目的 Git 状态
        for entry in &mut entries {
            if let Some(&status) = git_status_map.get(&entry.path) {
                entry.status = status;
            }
        }

        // 核心优化：传播状态到父目录
        self.propagate_statuses(&mut entries);

        Ok(entries)
    }

    /// 核心逻辑：将子目录/文件的状态传播到父目录
    fn propagate_statuses(&self, entries: &mut Vec<FileEntry>) {
        let mut path_to_status: HashMap<String, GitStatus> = HashMap::new();

        // 1. 收集所有已有状态
        for entry in entries.iter() {
            if entry.status != GitStatus::Clean {
                path_to_status.insert(entry.path.clone(), entry.status);
            }
        }

        // 2. 向上迭代传播
        let mut updates = Vec::new();
        for (path, status) in path_to_status.iter() {
            let mut current_path = Path::new(path);
            while let Some(parent) = current_path.parent() {
                let parent_str = parent.to_string_lossy().replace('\\', "/");
                if parent_str.is_empty() || parent_str == "." {
                    break;
                }
                updates.push((parent_str.clone(), *status));
                current_path = parent;
            }
        }

        // 3. 应用更新（按优先级）
        let mut final_statuses: HashMap<String, GitStatus> = HashMap::new();
        for (path, status) in updates {
            let existing = final_statuses.entry(path).or_insert(GitStatus::Clean);
            if status.priority() > existing.priority() {
                *existing = status;
            }
        }

        // 4. 写回 entries
        for entry in entries.iter_mut() {
            if entry.is_dir {
                if let Some(&status) = final_statuses.get(&entry.path) {
                    if status.priority() > entry.status.priority() {
                        entry.status = status;
                    }
                }
            }
        }
    }

    /// 获取所有文件（包括已跟踪和未跟踪的）
    fn get_all_files(&self) -> Result<Vec<FileEntry>> {
        let mut entries = Vec::new();
        self.collect_all_files(Path::new(&self.working_dir), 0, &mut entries)?;
        Ok(entries)
    }

    /// 递归收集所有文件
    fn collect_all_files(&self, dir: &Path, depth: usize, entries: &mut Vec<FileEntry>) -> Result<()> {
        // 跳过 .git 和其他大目录，优化性能
        if dir.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == ".git" || n == "node_modules" || n == "target" || n == ".idea" || n == ".vscode")
            .unwrap_or(false)
        {
            return Ok(());
        }

        if let Ok(read_dir) = std::fs::read_dir(dir) {
            let mut dir_entries: Vec<_> = read_dir
                .filter_map(|e| e.ok())
                .collect();

            // 目录优先排序
            dir_entries.sort_by(|a, b| {
                let a_is_dir = a.path().is_dir();
                let b_is_dir = b.path().is_dir();
                match (a_is_dir, b_is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.path().file_name().cmp(&b.path().file_name()),
                }
            });

            for entry in dir_entries {
                let path = entry.path();
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                // 跳过隐藏文件
                if file_name.starts_with('.') && file_name != ".gitignore" {
                    continue;
                }

                let relative_path = path
                    .strip_prefix(&self.working_dir)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"));

                let relative_path = relative_path.trim_start_matches('/').to_string();
                if relative_path.is_empty() { continue; }

                let is_dir = path.is_dir();

                entries.push(FileEntry {
                    path: relative_path,
                    status: GitStatus::Clean,
                    is_dir,
                    depth,
                });

                if is_dir {
                    self.collect_all_files(&path, depth + 1, entries)?;
                }
            }
        }
        Ok(())
    }

    /// 获取未跟踪的文件（在非 Git 仓库中使用）- 已弃用，逻辑已整合到 get_status
    fn _get_untracked_files(&self) -> Result<Vec<FileEntry>> {
        let mut entries = Vec::new();
        self.collect_all_files(Path::new(&self.working_dir), 0, &mut entries)?;
        for entry in &mut entries {
            entry.status = GitStatus::Untracked;
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_status_from_code() {
        assert_eq!(GitStatus::from_code("M"), GitStatus::Modified);
        assert_eq!(GitStatus::from_code("A "), GitStatus::Added);
        assert_eq!(GitStatus::from_code("??"), GitStatus::Untracked);
        assert_eq!(GitStatus::from_code("D "), GitStatus::Deleted);
    }

    #[test]
    fn test_git_status_symbol() {
        assert_eq!(GitStatus::Modified.symbol(), "M");
        assert_eq!(GitStatus::Untracked.symbol(), "??");
    }
}
