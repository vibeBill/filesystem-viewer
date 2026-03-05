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
    /// 当前工作目录相对于仓库根的前缀（如 `src/`）
    repo_prefix: String,
}

#[derive(Debug, Clone)]
struct StatusEntry {
    path: String,
    status: GitStatus,
}

include!("git/types_and_tests.rs");

include!("git/manager_core_impl.rs");
include!("git/status_impl.rs");
include!("git/files_impl.rs");
include!("git/diff_impl.rs");
