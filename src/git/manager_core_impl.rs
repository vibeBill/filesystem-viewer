impl GitStatusManager {
    pub fn new(working_dir: &str) -> Self {
        let is_git_repo = Self::detect_git_repo(Path::new(working_dir));
        let repo_prefix = if is_git_repo {
            Self::detect_repo_prefix(Path::new(working_dir)).unwrap_or_default()
        } else {
            String::new()
        };

        Self {
            working_dir: working_dir.to_string(),
            is_git_repo,
            expanded_dirs: HashSet::new(),
            repo_prefix,
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

    fn detect_repo_prefix(path: &Path) -> Option<String> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-prefix"])
            .current_dir(path)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let prefix = String::from_utf8_lossy(&output.stdout)
            .trim()
            .replace('\\', "/");
        if prefix.is_empty() {
            return Some(String::new());
        }

        Some(format!("{}/", prefix.trim_end_matches('/')))
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
}
