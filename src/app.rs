use crate::git::{GitStatusManager, FileEntry, GitStatus};
use std::sync::mpsc;
use std::time::Duration;
use notify::{Watcher, RecursiveMode};
use anyhow::Result;
use std::path::Path;
use std::collections::HashSet;
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::Rect;

/// 应用状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppState {
    Running,
    Quit,
}

/// 应用模式（包括搜索等特殊模式）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppMode {
    Normal,     // 正常模式
    Search,     // 搜索模式
}

/// 聚焦区域
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusArea {
    Tree,    // 左侧目录树
    Editor,  // 右侧编辑器
}

/// 文件显示模式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayMode {
    All,        // 显示所有文件
    Changed,    // 只显示变更文件
    Tracked,    // 只显示已跟踪文件
}

/// 应用数据
pub struct App {
    /// 工作目录
    pub working_dir: String,
    /// 文件列表（完整）
    pub files: Vec<FileEntry>,
    /// 缓存的过滤文件列表（避免重复计算）
    cached_filtered: Vec<String>,  // 存储路径
    /// 当前选中的文件索引（在过滤后的列表中）
    pub selected: usize,
    /// 滚动偏移
    pub scroll_offset: usize,
    /// 应用状态
    pub state: AppState,
    /// 聚焦区域
    pub focus: FocusArea,
    /// 显示模式
    pub display_mode: DisplayMode,
    /// Git 状态管理器
    pub git_manager: GitStatusManager,
    /// 最后刷新时间
    pub last_refresh: std::time::Instant,
    /// 刷新间隔（秒）
    pub refresh_interval: u64,
    /// 错误消息
    pub error_message: Option<String>,
    /// 是否显示帮助
    pub show_help: bool,
    /// 文件变更事件通道
    pub tx: Option<std::sync::mpsc::Sender<()>>,
    rx: Option<std::sync::mpsc::Receiver<()>>,
    /// 折叠的目录集合（存储目录路径）
    pub collapsed_dirs: HashSet<String>,
    /// 列表可视区域高度
    pub list_height: usize,
    /// 编辑器内容（可编辑的行）
    pub editor_content: Vec<String>,
    /// 原始文件内容（用于 diff 对比）
    pub editor_original_content: Vec<String>,
    /// 编辑器文件路径
    pub editor_path: String,
    /// 编辑器光标位置（行，列）
    pub editor_cursor: (usize, usize),
    /// 编辑器滚动偏移
    pub editor_scroll: usize,
    /// 编辑器是否已修改
    pub editor_modified: bool,
    /// 撤销历史（用于 Ctrl+Z）
    pub editor_undo_stack: Vec<Vec<String>>,
    /// 左侧目录树宽度
    pub tree_width: usize,
    /// 鼠标悬停的行
    pub hover_row: Option<u16>,
    /// 鼠标悬停的列
    pub hover_col: Option<u16>,
    /// 编辑器区域边界
    pub editor_area: Option<Rect>,
    /// 临时消息（保存成功等提示）
    pub status_message: Option<String>,
    /// 临时消息显示时间
    pub status_message_time: Option<std::time::Instant>,
    /// 应用模式
    pub mode: AppMode,
    /// 搜索查询
    pub search_query: String,
}

impl App {
    pub fn new(working_dir: &str) -> Result<Self> {
        let working_dir = working_dir.to_string();
        let git_manager = GitStatusManager::new(&working_dir);

        let (tx, rx) = mpsc::channel();

        let mut app = Self {
            working_dir,
            files: Vec::new(),
            cached_filtered: Vec::new(),
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
                self.files = files;
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

    /// 更新缓存的过滤文件列表
    fn update_cached_filtered(&mut self) {
        let files: Vec<&FileEntry> = match self.display_mode {
            DisplayMode::All => self.files.iter().collect(),
            DisplayMode::Changed => {
                self.files.iter()
                    .filter(|f| f.status != GitStatus::Clean && f.status != GitStatus::Ignored)
                    .collect()
            }
            DisplayMode::Tracked => {
                self.files.iter()
                    .filter(|f| f.status != GitStatus::Untracked)
                    .collect()
            }
        };

        let mut sorted: Vec<&FileEntry> = files;
        sorted.sort_by(|a, b| a.path.cmp(&b.path));

        self.cached_filtered = sorted
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
        if let Some(path) = self.cached_filtered.get(index) {
            self.files.iter().find(|f| f.path == *path)
        } else {
            None
        }
    }

    /// 检查是否需要刷新
    pub fn should_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= Duration::from_secs(self.refresh_interval)
    }

    /// 选择上一个文件
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
                if self.collapsed_dirs.contains(&target_path) {
                    self.collapsed_dirs.remove(&target_path);
                } else {
                    self.collapsed_dirs.insert(target_path);
                }
                // 重新计算过滤列表
                self.update_cached_filtered();
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

    /// 打开文件编辑器（加载文件到右侧编辑器）
    pub fn open_editor(&mut self) -> Result<()> {
        if let Some(file) = self.selected_file() {
            if file.is_dir {
                // 如果是目录，切换折叠状态
                self.toggle_collapse();
                return Ok(());
            }

            let full_path = Path::new(&self.working_dir).join(&file.path);
            let content = std::fs::read_to_string(&full_path)
                .unwrap_or_else(|_| String::new());

            self.editor_path = full_path.to_string_lossy().to_string();
            self.editor_content = content.lines().map(|s| s.to_string()).collect();
            // 保存原始内容用于 diff 对比
            self.editor_original_content = self.editor_content.clone();
            self.editor_cursor = (0, 0);
            self.editor_scroll = 0;
            self.editor_modified = false;
            self.editor_undo_stack.clear();
            self.focus = FocusArea::Editor;
        }
        Ok(())
    }

    /// 编辑器中向上移动光标
    pub fn editor_up(&mut self) {
        if self.editor_cursor.0 > 0 {
            self.editor_cursor.0 -= 1;
            // 调整列位置
            if self.editor_cursor.1 > self.editor_content[self.editor_cursor.0].chars().count() {
                self.editor_cursor.1 = self.editor_content[self.editor_cursor.0].chars().count();
            }
            // 调整滚动
            if self.editor_scroll > self.editor_cursor.0 {
                self.editor_scroll = self.editor_cursor.0;
            }
        }
    }

    /// 编辑器中向下移动光标
    pub fn editor_down(&mut self) {
        if self.editor_cursor.0 < self.editor_content.len().saturating_sub(1) {
            self.editor_cursor.0 += 1;
            // 调整列位置
            if self.editor_cursor.1 > self.editor_content[self.editor_cursor.0].chars().count() {
                self.editor_cursor.1 = self.editor_content[self.editor_cursor.0].chars().count();
            }
            // 调整滚动
            let visible_lines = self.list_height.max(10);
            if self.editor_cursor.0 >= self.editor_scroll + visible_lines {
                self.editor_scroll = self.editor_cursor.0 - visible_lines + 1;
            }
        }
    }

    /// 编辑器整页向上滚动
    pub fn editor_page_up(&mut self) {
        let visible_lines = self.list_height.max(10);
        if self.editor_scroll >= visible_lines {
            self.editor_scroll -= visible_lines;
        } else {
            self.editor_scroll = 0;
        }
        if self.editor_cursor.0 < self.editor_scroll {
            self.editor_cursor.0 = self.editor_scroll;
        }
    }

    /// 编辑器整页向下滚动
    pub fn editor_page_down(&mut self) {
        let visible_lines = self.list_height.max(10);
        let max_scroll = self.editor_content.len().saturating_sub(visible_lines);
        if self.editor_scroll + visible_lines * 2 <= self.editor_content.len() {
            self.editor_scroll += visible_lines;
        } else {
            self.editor_scroll = max_scroll;
        }
        let max_cursor = self.editor_scroll + visible_lines - 1;
        if self.editor_cursor.0 > max_cursor.min(self.editor_content.len().saturating_sub(1)) {
            self.editor_cursor.0 = max_cursor.min(self.editor_content.len().saturating_sub(1));
        }
    }

    /// 编辑器中向左移动光标
    pub fn editor_left(&mut self) {
        if self.editor_cursor.1 > 0 {
            self.editor_cursor.1 -= 1;
        }
    }

    /// 编辑器中向右移动光标
    pub fn editor_right(&mut self) {
        let line_len = self.editor_content[self.editor_cursor.0].chars().count();
        if self.editor_cursor.1 < line_len {
            self.editor_cursor.1 += 1;
        }
    }

    /// 编辑器中删除字符（Backspace）
    pub fn editor_backspace(&mut self) {
        self.push_undo();
        if self.editor_cursor.1 > 0 {
            let line = &mut self.editor_content[self.editor_cursor.0];
            let mut chars: Vec<char> = line.chars().collect();
            chars.remove(self.editor_cursor.1 - 1);
            *line = chars.iter().collect();
            self.editor_cursor.1 -= 1;
            self.editor_modified = true;
        } else if self.editor_cursor.0 > 0 {
            // 合并到上一行
            let prev_len = self.editor_content[self.editor_cursor.0 - 1].len();
            let current_line = self.editor_content.remove(self.editor_cursor.0);
            self.editor_content[self.editor_cursor.0 - 1].push_str(&current_line);
            self.editor_cursor.0 -= 1;
            self.editor_cursor.1 = prev_len;
            self.editor_modified = true;
        }
    }

    /// 编辑器中删除字符（Delete）
    pub fn editor_delete(&mut self) {
        self.push_undo();
        let line_len = self.editor_content[self.editor_cursor.0].chars().count();
        if self.editor_cursor.1 < line_len {
            let line = &mut self.editor_content[self.editor_cursor.0];
            let mut chars: Vec<char> = line.chars().collect();
            chars.remove(self.editor_cursor.1);
            *line = chars.iter().collect();
            self.editor_modified = true;
        } else if self.editor_cursor.0 < self.editor_content.len() - 1 {
            // 合并下一行
            let next_line = self.editor_content.remove(self.editor_cursor.0 + 1);
            self.editor_content[self.editor_cursor.0].push_str(&next_line);
            self.editor_modified = true;
        }
    }

    /// 编辑器中插入字符
    pub fn editor_insert(&mut self, c: char) {
        self.push_undo();
        let line = &mut self.editor_content[self.editor_cursor.0];
        let mut chars: Vec<char> = line.chars().collect();
        chars.insert(self.editor_cursor.1, c);
        *line = chars.iter().collect();
        self.editor_cursor.1 += 1;
        self.editor_modified = true;
    }

    /// 编辑器中插入新行
    pub fn editor_insert_newline(&mut self) {
        self.push_undo();
        let current_line = self.editor_content[self.editor_cursor.0].clone();
        let before: String = current_line.chars().take(self.editor_cursor.1).collect();
        let after: String = current_line.chars().skip(self.editor_cursor.1).collect();

        self.editor_content[self.editor_cursor.0] = before;
        self.editor_content.insert(self.editor_cursor.0 + 1, after);
        self.editor_cursor.0 += 1;
        self.editor_cursor.1 = 0;
        self.editor_modified = true;
    }

    /// 推送撤销历史
    fn push_undo(&mut self) {
        // 限制撤销栈大小为 100
        if self.editor_undo_stack.len() >= 100 {
            self.editor_undo_stack.remove(0);
        }
        self.editor_undo_stack.push(self.editor_content.clone());
    }

    /// 撤销（Ctrl+Z）
    pub fn editor_undo(&mut self) {
        if let Some(prev_state) = self.editor_undo_stack.pop() {
            self.editor_content = prev_state;
            self.editor_modified = true;
            // 调整光标
            if self.editor_cursor.0 >= self.editor_content.len() {
                self.editor_cursor.0 = self.editor_content.len().saturating_sub(1);
            }
        }
    }

    /// 保存文件
    pub fn editor_save(&mut self) -> Result<()> {
        let content = self.editor_content.join("\n");
        std::fs::write(&self.editor_path, content)?;
        self.editor_modified = false;
        // 显示保存成功提示
        self.status_message = Some("✓ 文件已保存".to_string());
        self.status_message_time = Some(std::time::Instant::now());
        Ok(())
    }

    /// 退出编辑器（切换到目录树聚焦）
    pub fn exit_editor(&mut self) {
        // 如果有未保存的修改，提示用户
        if self.editor_modified {
            self.status_message = Some("⚠ 未保存的修改已丢弃".to_string());
            self.status_message_time = Some(std::time::Instant::now());
        }
        self.focus = FocusArea::Tree;
    }

    /// 切换聚焦区域
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            FocusArea::Tree => FocusArea::Editor,
            FocusArea::Editor => FocusArea::Tree,
        };
    }

    /// 设置编辑器区域
    pub fn set_editor_area(&mut self, area: Rect) {
        self.editor_area = Some(area);
    }

    /// 整页向上滚动
    pub fn page_up(&mut self) {
        if self.scroll_offset >= self.list_height {
            self.scroll_offset -= self.list_height;
        } else {
            self.scroll_offset = 0;
        }
        if self.selected > self.scroll_offset {
            self.selected = self.scroll_offset;
        }
    }

    /// 整页向下滚动
    pub fn page_down(&mut self) {
        let filtered_len = self.cached_filtered.len();
        if self.scroll_offset + self.list_height * 2 < filtered_len {
            self.scroll_offset += self.list_height;
        } else {
            self.scroll_offset = filtered_len.saturating_sub(self.list_height);
        }
        let new_selected = (self.selected + self.list_height).min(filtered_len - 1);
        if new_selected > self.selected {
            self.selected = new_selected;
        }
    }

    /// 向上滚动 (逐行)
    pub fn scroll_up(&mut self, lines: usize) {
        for _ in 0..lines {
            if self.scroll_offset > 0 {
                self.scroll_offset -= 1;
            }
            if self.selected > self.scroll_offset + self.list_height - 1 {
                self.selected = self.scroll_offset + self.list_height - 1;
            }
        }
    }

    /// 向下滚动 (逐行)
    pub fn scroll_down(&mut self, lines: usize) {
        let filtered_len = self.cached_filtered.len();
        for _ in 0..lines {
            if self.scroll_offset + self.list_height < filtered_len {
                self.scroll_offset += 1;
            }
            if self.selected < self.scroll_offset {
                self.selected = self.scroll_offset;
            }
        }
    }

    /// 编辑器向上滚动 (逐行)
    pub fn editor_scroll_up(&mut self, lines: usize) {
        if self.editor_scroll >= lines {
            self.editor_scroll -= lines;
        } else {
            self.editor_scroll = 0;
        }
    }

    /// 编辑器向下滚动 (逐行)
    pub fn editor_scroll_down(&mut self, lines: usize) {
        let visible_lines = self.list_height.max(10);
        let max_scroll = self.editor_content.len().saturating_sub(visible_lines);
        if self.editor_scroll + lines <= max_scroll {
            self.editor_scroll += lines;
        } else {
            self.editor_scroll = max_scroll;
        }
    }

    /// 处理鼠标点击事件
    pub fn handle_mouse_click(&mut self, row: u16, column: u16, kind: MouseEventKind) -> bool {
        // 更新 hover 位置
        self.hover_row = Some(row);
        self.hover_col = Some(column);

        match kind {
            // 左键点击
            MouseEventKind::Down(MouseButton::Left) => {
                // 检查是否点击在编辑器区域
                if let Some(editor_area) = self.editor_area {
                    if column >= editor_area.x && column < editor_area.x + editor_area.width
                        && row >= editor_area.y && row < editor_area.y + editor_area.height
                    {
                        // 点击在编辑器上
                        self.focus = FocusArea::Editor;
                        self.editor_mouse_click(row, column);
                        return false;
                    }
                }

                // 检查是否点击在目录树区域（左侧）
                if column < (self.tree_width as u16) {
                    // 使用 list_height 来判断有效行范围
                    // 目录树从 y=4 (header 3 + border 1) 开始到 4+list_height
                    if row >= 4 && row < 4 + (self.list_height as u16) {
                        let item_rel_index = (row - 4) as usize;
                        let actual_index = self.scroll_offset + item_rel_index;

                        if actual_index < self.cached_filtered.len() {
                            // 如果点击的是已经选中的项，或是文件，则尝试进入编辑模式
                            if self.selected == actual_index {
                                if let Some(file) = self.get_file_by_index(actual_index) {
                                    if file.is_dir {
                                        self.toggle_collapse();
                                    } else {
                                        let _ = self.open_editor();
                                    }
                                }
                            } else {
                                self.selected = actual_index;
                            }
                            self.focus = FocusArea::Tree;
                            return true;
                        }
                    }
                }
            }
            // 中键点击 - 整页滚动
            MouseEventKind::Down(MouseButton::Middle) => {
                if row >= 3 {
                    if column < self.tree_width as u16 {
                        self.page_down();
                    } else {
                        self.editor_page_down();
                    }
                }
            }
            _ => {}
        }
        false
    }

    /// 处理编辑器鼠标点击（设置光标位置）
    fn editor_mouse_click(&mut self, row: u16, column: u16) {
        if let Some(editor_area) = self.editor_area {
            // 检查行
            if row >= editor_area.y + 1 && row < editor_area.y + editor_area.height - 1 {
                let line_offset = (row - (editor_area.y + 1)) as usize;
                let new_line = (self.editor_scroll + line_offset).min(self.editor_content.len().saturating_sub(1));
                self.editor_cursor.0 = new_line;

                // 计算列：减去边框(1)和行号区(5)
                let text_start_x = editor_area.x + 6;
                if column >= text_start_x {
                    let col_offset = (column - text_start_x) as usize;
                    let line_len = self.editor_content.get(new_line).map(|s| s.chars().count()).unwrap_or(0);
                    self.editor_cursor.1 = col_offset.min(line_len);
                } else {
                    self.editor_cursor.1 = 0;
                }
            }
        }
    }
}

/// 统计信息
#[derive(Debug, Default)]
pub struct Stats {
    pub total: usize,
    pub modified: usize,
    pub added: usize,
    pub deleted: usize,
    pub untracked: usize,
    pub renamed: usize,
    pub clean: usize,
}

/// 文件监听器
pub struct FileWatcher {
    tx: std::sync::mpsc::Sender<()>,
}

impl FileWatcher {
    pub fn new(tx: std::sync::mpsc::Sender<()>) -> Result<Self> {
        Ok(Self { tx })
    }

    pub fn start(&mut self, path: &str) -> Result<()> {
        let tx = self.tx.clone();
        let path = Path::new(path).to_path_buf();

        // 使用 notify 的事件 watcher
        let mut watcher = notify::recommended_watcher(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(_) = res {
                    let _ = tx.send(());
                }
            }
        )?;

        watcher.watch(&path, RecursiveMode::Recursive)?;
        Ok(())
    }
}
