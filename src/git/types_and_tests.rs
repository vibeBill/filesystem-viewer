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
    fn test_normalize_status_path_with_prefix() {
        let mgr = GitStatusManager {
            working_dir: String::new(),
            is_git_repo: true,
            expanded_dirs: HashSet::new(),
            repo_prefix: "src/".to_string(),
        };

        assert_eq!(
            mgr.normalize_status_path("src/main.rs"),
            Some("main.rs".to_string())
        );
        assert_eq!(mgr.normalize_status_path("other/main.rs"), None);
    }

    #[test]
    fn test_git_status_symbol() {
        assert_eq!(GitStatus::Modified.symbol(), "M");
        assert_eq!(GitStatus::Untracked.symbol(), "??");
    }
}
