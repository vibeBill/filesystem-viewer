impl GitStatusManager {
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
                if let Some(path) = self.normalize_status_path(&parsed.path) {
                    git_status_map.insert(path.clone(), parsed.status);
                    status_entries.push(StatusEntry {
                        path,
                        status: parsed.status,
                    });
                }
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

    fn normalize_status_path(&self, path: &str) -> Option<String> {
        let normalized = path.replace('\\', "/");

        if self.repo_prefix.is_empty() {
            return Some(normalized.trim_start_matches("./").to_string());
        }

        if normalized == self.repo_prefix.trim_end_matches('/') {
            return None;
        }

        normalized
            .strip_prefix(&self.repo_prefix)
            .map(|p| p.trim_start_matches("./").to_string())
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
}
