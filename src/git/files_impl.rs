impl GitStatusManager {
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
}
