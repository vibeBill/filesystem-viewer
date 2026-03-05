impl App {
    pub fn select_previous(&mut self) {
        if !self.cached_filtered.is_empty() && self.selected > 0 {
            self.selected -= 1;
            // 调整滚动偏移
            if self.scroll_offset > 0 && self.selected < self.scroll_offset {
                self.scroll_offset = self.selected;
            }
        }
    }

    /// 选择下一个文件
    pub fn select_next(&mut self) {
        if !self.cached_filtered.is_empty() && self.selected < self.cached_filtered.len() - 1 {
            self.selected += 1;
            // 调整滚动偏移
            if self.selected >= self.scroll_offset + self.list_height {
                self.scroll_offset = self.selected - self.list_height + 1;
            }
        }
    }

    /// 切换目录折叠状态
    pub fn toggle_collapse(&mut self) {
        if let Some(file) = self.get_file_by_index(self.selected) {
            let target_path = if file.is_dir {
                file.path.clone()
            } else {
                // 如果是文件，使用其父目录
                self.get_parent_dir(&file.path).unwrap_or_default()
            };

            if !target_path.is_empty() {
                let mut needs_refresh = false;

                if self.collapsed_dirs.contains(&target_path) {
                    // 展开目录
                    self.collapsed_dirs.remove(&target_path);
                    self.git_manager.expand_dir(&target_path);
                    needs_refresh = true;
                } else {
                    // 折叠目录
                    self.collapsed_dirs.insert(target_path.clone());
                    self.git_manager.collapse_dir(&target_path);
                    // 折叠时不需要重新读取文件系统，直接在 UI 层面隐藏即可（也可以重新读取释放内存）
                }

                if needs_refresh {
                    let _ = self.refresh_files();
                } else {
                    // 重新计算过滤列表
                    self.update_cached_filtered();
                }

                // 调整选中索引
                if self.selected >= self.cached_filtered.len() {
                    self.selected = self.cached_filtered.len().saturating_sub(1);
                }
            }
        }
    }

    /// 获取父目录路径
    fn get_parent_dir(&self, path: &str) -> Option<String> {
        let path = Path::new(path);
        path.parent().map(|p| p.to_string_lossy().to_string())
    }

    /// 检查目录是否被折叠
    pub fn is_collapsed(&self, path: &str) -> bool {
        self.collapsed_dirs.contains(path)
    }

    /// 检查路径是否应该显示（考虑折叠状态）
    pub fn should_show(&self, path: &str) -> bool {
        let path_obj = Path::new(path);
        let mut current = path_obj.parent();

        while let Some(parent) = current {
            let parent_path = parent.to_string_lossy().to_string();
            if self.collapsed_dirs.contains(&parent_path) {
                return false;
            }
            current = parent.parent();
        }
        true
    }

    /// 切换显示模式
    pub fn toggle_display_mode(&mut self) {
        self.display_mode = match self.display_mode {
            DisplayMode::All => DisplayMode::Changed,
            DisplayMode::Changed => DisplayMode::Tracked,
            DisplayMode::Tracked => DisplayMode::All,
        };
        // 刷新缓存
        self.update_cached_filtered();
        // 重置选择
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// 获取选中的文件
    pub fn selected_file(&self) -> Option<&FileEntry> {
        self.get_file_by_index(self.selected)
    }

    /// 退出应用
    pub fn quit(&mut self) {
        self.state = AppState::Quit;
    }

    /// 切换搜索模式
    pub fn toggle_search(&mut self) {
        match self.mode {
            AppMode::Normal => {
                self.mode = AppMode::Search;
                self.search_query.clear();
            }
            AppMode::Search => {
                self.mode = AppMode::Normal;
                self.search_query.clear();
            }
        }
    }

    /// 在搜索模式下输入字符
    pub fn search_input(&mut self, c: char) {
        self.search_query.push(c);
        // 搜索并跳转到匹配的文件
        self.search_and_select();
    }

    /// 在搜索模式下删除字符
    pub fn search_backspace(&mut self) {
        self.search_query.pop();
        self.search_and_select();
    }

    /// 搜索并选中匹配的文件
    fn search_and_select(&mut self) {
        if self.search_query.is_empty() {
            return;
        }

        let filtered = self.get_filtered_paths();
        let query_lower = self.search_query.to_lowercase();

        // 查找匹配的文件
        for (idx, path) in filtered.iter().enumerate() {
            if path.to_lowercase().contains(&query_lower) {
                self.selected = idx;
                // 调整滚动偏移
                if self.selected < self.scroll_offset {
                    self.scroll_offset = self.selected;
                } else if self.selected >= self.scroll_offset + self.list_height {
                    self.scroll_offset = self.selected - self.list_height + 1;
                }
                break;
            }
        }
    }

    /// 切换帮助显示
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> Stats {
        let mut stats = Stats::default();
        for file in &self.files {
            match file.status {
                GitStatus::Modified => stats.modified += 1,
                GitStatus::Added => stats.added += 1,
                GitStatus::Deleted => stats.deleted += 1,
                GitStatus::Untracked => stats.untracked += 1,
                GitStatus::Renamed => stats.renamed += 1,
                GitStatus::Clean => stats.clean += 1,
                _ => {}
            }
        }
        stats.total = self.files.len();
        stats
    }

    /// 设置列表高度
    pub fn set_list_height(&mut self, height: usize) {
        self.list_height = height;
    }

}
