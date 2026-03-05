impl GitStatusManager {
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
