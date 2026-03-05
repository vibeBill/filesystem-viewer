impl App {
    pub fn new(working_dir: &str) -> Result<Self> {
        let working_dir = working_dir.to_string();
        let git_manager = GitStatusManager::new(&working_dir);

        let (tx, rx) = mpsc::channel();

        let mut app = Self {
            working_dir,
            files: Vec::new(),
            cached_filtered: Vec::new(),
            file_index: HashMap::new(),
            selected: 0,
            scroll_offset: 0,
            state: AppState::Running,
            focus: FocusArea::Tree,
            display_mode: DisplayMode::All,
            git_manager,
            last_refresh: std::time::Instant::now(),
            refresh_interval: 2,
            error_message: None,
            show_help: false,
            tx: Some(tx),
            rx: Some(rx),
            collapsed_dirs: HashSet::new(),
            list_height: 20,
            editor_content: Vec::new(),
            editor_original_content: Vec::new(),
            editor_path: String::new(),
            editor_cursor: (0, 0),
            editor_scroll: 0,
            editor_modified: false,
            editor_undo_stack: Vec::new(),
            tree_width: 40,
            hover_row: None,
            hover_col: None,
            editor_area: None,
            status_message: None,
            status_message_time: None,
            mode: AppMode::Normal,
            search_query: String::new(),
            middle_drag_origin: None,
            is_middle_dragging: false,
            editor_diff_lines: HashMap::new(),
            editor_h_scroll: 0,
        };

        // 初始刷新
        app.refresh_files()?;

        // 默认收起所有目录
        for file in &app.files {
            if file.is_dir {
                app.collapsed_dirs.insert(file.path.clone());
            }
        }
        app.update_cached_filtered();

        Ok(app)
    }

    /// 获取文件变更事件接收器
    pub fn get_change_receiver(&mut self) -> Option<std::sync::mpsc::Receiver<()>> {
        self.rx.take()
    }

    /// 刷新文件列表
    pub fn refresh_files(&mut self) -> Result<()> {
        match self.git_manager.get_status() {
            Ok(files) => {
                let known_dirs: HashSet<String> = self
                    .files
                    .iter()
                    .filter(|f| f.is_dir)
                    .map(|f| f.path.clone())
                    .collect();

                self.files = files;

                let current_dirs: HashSet<String> = self
                    .files
                    .iter()
                    .filter(|f| f.is_dir)
                    .map(|f| f.path.clone())
                    .collect();

                // 清理失效目录，并把首次出现的目录默认标记为折叠。
                self.collapsed_dirs.retain(|path| current_dirs.contains(path));
                for dir in &current_dirs {
                    if !known_dirs.contains(dir) {
                        self.collapsed_dirs.insert(dir.clone());
                    }
                }

                self.rebuild_file_index();
                self.error_message = None;
                // 刷新缓存
                self.update_cached_filtered();

                // 调整选中索引
                if !self.cached_filtered.is_empty() && self.selected >= self.cached_filtered.len() {
                    self.selected = self.cached_filtered.len() - 1;
                }
            }
            Err(e) => {
                self.error_message = Some(e.to_string());
            }
        }
        self.last_refresh = std::time::Instant::now();
        Ok(())
    }

    fn rebuild_file_index(&mut self) {
        self.file_index.clear();
        self.file_index.reserve(self.files.len());
        for (idx, file) in self.files.iter().enumerate() {
            self.file_index.insert(file.path.clone(), idx);
        }
    }

    pub fn get_file_by_path(&self, path: &str) -> Option<&FileEntry> {
        self.file_index
            .get(path)
            .and_then(|&idx| self.files.get(idx))
    }

    /// 更新缓存的过滤文件列表
    fn update_cached_filtered(&mut self) {
        let filtered: Vec<&FileEntry> = self
            .files
            .iter()
            .filter(|f| match self.display_mode {
                DisplayMode::All => true,
                DisplayMode::Changed => {
                    f.status != GitStatus::Clean && f.status != GitStatus::Ignored
                }
                DisplayMode::Tracked => f.status != GitStatus::Untracked,
            })
            .collect();

        // 保持 git/files 收集阶段的顺序：同级目录下“文件夹在前，文件在后”，且各自按名称排序。
        // 这里不再按完整路径重排，避免破坏目录优先的展示规则。
        self.cached_filtered = filtered
            .into_iter()
            .filter(|f| self.should_show(&f.path))
            .map(|f| f.path.clone())
            .collect();
    }

    /// 获取缓存的过滤文件路径
    pub fn get_filtered_paths(&self) -> &Vec<String> {
        &self.cached_filtered
    }

    /// 根据索引获取文件
    pub fn get_file_by_index(&self, index: usize) -> Option<&FileEntry> {
        self.cached_filtered
            .get(index)
            .and_then(|path| self.get_file_by_path(path))
    }

    /// 检查是否需要刷新
    pub fn should_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= Duration::from_secs(self.refresh_interval)
    }

}
