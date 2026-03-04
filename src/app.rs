use crate::git::{DiffLineType, FileEntry, GitStatus, GitStatusManager};
use anyhow::Result;
use crossterm::event::{MouseButton, MouseEventKind};
use notify::{RecursiveMode, Watcher};
use ratatui::layout::Rect;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// 应用状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppState {
    Running,
    Quit,
}

/// 应用模式（包括搜索等特殊模式）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppMode {
    Normal, // 正常模式
    Search, // 搜索模式
}

/// 聚焦区域
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusArea {
    Tree,     // 左侧目录树
    Editor,   // 右侧编辑器
    Terminal, // 底部终端
}

#[derive(Debug, Clone)]
pub struct TerminalTab {
    pub name: String,
    pub input: String,
    pub output: Vec<String>,
}

#[derive(Debug, Clone)]
struct TerminalCommandResult {
    tab_index: usize,
    output: Result<String, String>,
}

/// 文件显示模式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayMode {
    All,     // 显示所有文件
    Changed, // 只显示变更文件
    Tracked, // 只显示已跟踪文件
}

/// 应用数据
pub struct App {
    /// 工作目录
    pub working_dir: String,
    /// 文件列表（完整）
    pub files: Vec<FileEntry>,
    /// 缓存的过滤文件列表（避免重复计算）
    cached_filtered: Vec<String>, // 存储路径
    /// 路径到 files 下标的索引缓存（避免 O(n) 查找）
    file_index: HashMap<String, usize>,
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
    /// 终端区域边界
    pub terminal_area: Option<Rect>,
    /// 临时消息（保存成功等提示）
    pub status_message: Option<String>,
    /// 临时消息显示时间
    pub status_message_time: Option<std::time::Instant>,
    /// 应用模式
    pub mode: AppMode,
    /// 搜索查询
    pub search_query: String,
    /// 中键拖拽起始 y 坐标
    pub middle_drag_origin: Option<u16>,
    /// 是否正在中键拖拽
    pub is_middle_dragging: bool,
    /// 编辑器 git diff 行信息（行号 -> 变更类型）
    pub editor_diff_lines: HashMap<usize, DiffLineType>,
    /// 编辑器水平滚动偏移（用于长行）
    pub editor_h_scroll: usize,
    /// 终端 tabs
    pub terminal_tabs: Vec<TerminalTab>,
    /// 当前终端 tab 下标
    pub active_terminal_tab: usize,
    /// 终端滚动偏移
    pub terminal_scroll: usize,
    /// 后台终端命令结果发送端
    terminal_result_tx: std::sync::mpsc::Sender<TerminalCommandResult>,
    /// 后台终端命令结果接收端
    terminal_result_rx: std::sync::mpsc::Receiver<TerminalCommandResult>,
}

impl App {
    pub fn new(working_dir: &str) -> Result<Self> {
        let working_dir = working_dir.to_string();
        let git_manager = GitStatusManager::new(&working_dir);

        let (tx, rx) = mpsc::channel();
        let (terminal_result_tx, terminal_result_rx) = mpsc::channel();

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
            terminal_area: None,
            status_message: None,
            status_message_time: None,
            mode: AppMode::Normal,
            search_query: String::new(),
            middle_drag_origin: None,
            is_middle_dragging: false,
            editor_diff_lines: HashMap::new(),
            editor_h_scroll: 0,
            terminal_tabs: vec![TerminalTab::new("终端 1")],
            active_terminal_tab: 0,
            terminal_scroll: 0,
            terminal_result_tx,
            terminal_result_rx,
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
        let mut sorted: Vec<&FileEntry> = self
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

        sorted.sort_unstable_by(|a, b| a.path.cmp(&b.path));

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
        self.cached_filtered
            .get(index)
            .and_then(|path| self.get_file_by_path(path))
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

    /// 打开文件编辑器（加载文件到右侧编辑器）
    pub fn open_editor(&mut self) -> Result<()> {
        if let Some(file) = self.selected_file() {
            if file.is_dir {
                // 如果是目录，切换折叠状态
                self.toggle_collapse();
                return Ok(());
            }

            let file_path = file.path.clone();
            let full_path = Path::new(&self.working_dir).join(&file_path);
            let content = std::fs::read_to_string(&full_path).unwrap_or_else(|_| String::new());

            self.editor_path = full_path.to_string_lossy().to_string();
            self.editor_content = content.lines().map(|s| s.to_string()).collect();
            // 保存原始内容用于 diff 对比
            self.editor_original_content = self.editor_content.clone();
            self.editor_cursor = (0, 0);
            self.editor_scroll = 0;
            self.editor_h_scroll = 0;
            self.editor_modified = false;
            self.editor_undo_stack.clear();
            // 加载 git diff 行信息
            self.editor_diff_lines = self.git_manager.get_file_diff_lines(&file_path);
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
            FocusArea::Editor => FocusArea::Terminal,
            FocusArea::Terminal => FocusArea::Tree,
        };
    }

    pub fn create_terminal_tab(&mut self) {
        let tab_name = format!("终端 {}", self.terminal_tabs.len() + 1);
        self.terminal_tabs.push(TerminalTab::new(&tab_name));
        self.active_terminal_tab = self.terminal_tabs.len().saturating_sub(1);
        self.terminal_scroll = 0;
    }

    pub fn next_terminal_tab(&mut self) {
        if !self.terminal_tabs.is_empty() {
            self.active_terminal_tab = (self.active_terminal_tab + 1) % self.terminal_tabs.len();
            self.terminal_scroll = 0;
        }
    }

    pub fn prev_terminal_tab(&mut self) {
        if !self.terminal_tabs.is_empty() {
            self.active_terminal_tab = if self.active_terminal_tab == 0 {
                self.terminal_tabs.len() - 1
            } else {
                self.active_terminal_tab - 1
            };
            self.terminal_scroll = 0;
        }
    }

    pub fn terminal_input_char(&mut self, c: char) {
        if let Some(tab) = self.terminal_tabs.get_mut(self.active_terminal_tab) {
            tab.input.push(c);
        }
    }

    pub fn terminal_backspace(&mut self) {
        if let Some(tab) = self.terminal_tabs.get_mut(self.active_terminal_tab) {
            tab.input.pop();
        }
    }

    pub fn terminal_execute(&mut self) {
        let tab_index = self.active_terminal_tab;
        let Some(tab) = self.terminal_tabs.get_mut(tab_index) else {
            return;
        };

        let command_text = tab.input.trim().to_string();
        if command_text.is_empty() {
            return;
        }

        tab.output.push(format!("> {}", command_text));

        tab.output.push("[后台执行中...]".to_string());
        tab.input.clear();
        self.terminal_scroll = tab.output.len().saturating_sub(1);

        let working_dir = self.working_dir.clone();
        let sender = self.terminal_result_tx.clone();
        thread::spawn(move || {
            let output = run_shell_command(&working_dir, &command_text).map_err(|e| e.to_string());
            let _ = sender.send(TerminalCommandResult { tab_index, output });
        });
    }

    pub fn poll_terminal_output(&mut self) {
        while let Ok(result) = self.terminal_result_rx.try_recv() {
            let Some(tab) = self.terminal_tabs.get_mut(result.tab_index) else {
                continue;
            };

            match result.output {
                Ok(output) => {
                    tab.output
                        .extend(output.lines().map(|line| line.to_string()));
                }
                Err(err) => tab.output.push(format!("[错误] {}", err)),
            }

            tab.output.push(String::new());

            if self.active_terminal_tab == result.tab_index {
                self.terminal_scroll = tab.output.len().saturating_sub(1);
            }
        }
    }

    pub fn terminal_scroll_up(&mut self, lines: usize) {
        self.terminal_scroll = self.terminal_scroll.saturating_sub(lines);
    }

    pub fn terminal_scroll_down(&mut self, lines: usize) {
        if let Some(tab) = self.terminal_tabs.get(self.active_terminal_tab) {
            let max_scroll = tab.output.len().saturating_sub(1);
            self.terminal_scroll = (self.terminal_scroll + lines).min(max_scroll);
        }
    }

    /// 设置编辑器区域
    pub fn set_editor_area(&mut self, area: Rect) {
        self.editor_area = Some(area);
    }

    /// 设置终端区域
    pub fn set_terminal_area(&mut self, area: Rect) {
        self.terminal_area = Some(area);
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
                    if column >= editor_area.x
                        && column < editor_area.x + editor_area.width
                        && row >= editor_area.y
                        && row < editor_area.y + editor_area.height
                    {
                        // 点击在编辑器上
                        self.focus = FocusArea::Editor;
                        self.editor_mouse_click(row, column);
                        return false;
                    }
                }

                // 检查是否点击在终端区域
                if let Some(terminal_area) = self.terminal_area {
                    if column >= terminal_area.x
                        && column < terminal_area.x + terminal_area.width
                        && row >= terminal_area.y
                        && row < terminal_area.y + terminal_area.height
                    {
                        self.focus = FocusArea::Terminal;
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
            // 中键按下 - 记录起始位置，开始拖拽模式
            MouseEventKind::Down(MouseButton::Middle) => {
                self.middle_drag_origin = Some(row);
                self.is_middle_dragging = true;
            }
            _ => {}
        }
        false
    }

    /// 处理中键拖拽滚动
    pub fn handle_middle_drag(&mut self, row: u16, column: u16) {
        if !self.is_middle_dragging {
            return;
        }

        if let Some(origin_y) = self.middle_drag_origin {
            let delta = row as i32 - origin_y as i32;

            if delta.abs() >= 1 {
                let lines = delta.unsigned_abs() as usize;

                if column < self.tree_width as u16 {
                    // 拖拽目录树区域
                    if delta < 0 {
                        self.scroll_up(lines);
                    } else {
                        self.scroll_down(lines);
                    }
                } else {
                    // 拖拽编辑器区域
                    if delta < 0 {
                        self.editor_scroll_up(lines);
                    } else {
                        self.editor_scroll_down(lines);
                    }
                }

                // 更新起始位置
                self.middle_drag_origin = Some(row);
            }
        }
    }

    /// 停止中键拖拽
    pub fn stop_middle_drag(&mut self) {
        self.is_middle_dragging = false;
        self.middle_drag_origin = None;
    }

    /// 处理编辑器鼠标点击（设置光标位置）
    fn editor_mouse_click(&mut self, row: u16, column: u16) {
        if let Some(editor_area) = self.editor_area {
            // 检查行
            if row >= editor_area.y + 1 && row < editor_area.y + editor_area.height - 1 {
                let line_offset = (row - (editor_area.y + 1)) as usize;
                let new_line = (self.editor_scroll + line_offset)
                    .min(self.editor_content.len().saturating_sub(1));
                self.editor_cursor.0 = new_line;

                // 计算列：减去边框(1) + gutter(1) + 行号区(4) + 空格(1)
                let text_start_x = editor_area.x + 7;
                if column >= text_start_x {
                    let col_offset = (column - text_start_x) as usize;
                    let line_len = self
                        .editor_content
                        .get(new_line)
                        .map(|s| s.chars().count())
                        .unwrap_or(0);
                    self.editor_cursor.1 = col_offset.min(line_len);
                } else {
                    self.editor_cursor.1 = 0;
                }
            }
        }
    }
}

impl TerminalTab {
    fn new(name: &str) -> Self {
        let mut output = vec![
            "欢迎使用内置终端。".to_string(),
            "可执行示例：pnpm run dev / codex / claude / git status".to_string(),
        ];
        output.push(String::new());

        Self {
            name: name.to_string(),
            input: String::new(),
            output,
        }
    }
}

fn run_shell_command(working_dir: &str, command: &str) -> Result<String> {
    let (program, args) = shell_command(command);
    let output = Command::new(program)
        .args(args)
        .current_dir(working_dir)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let mut merged = String::new();

    if !stdout.is_empty() {
        merged.push_str(&stdout);
        if !merged.ends_with('\n') {
            merged.push('\n');
        }
    }

    if !stderr.is_empty() {
        merged.push_str(&stderr);
        if !merged.ends_with('\n') {
            merged.push('\n');
        }
    }

    if !output.status.success() {
        merged.push_str(&format!("命令退出状态：{}\n", output.status));
    }

    if merged.is_empty() {
        merged.push_str("(命令执行完成，无输出)");
    }

    Ok(merged)
}

fn shell_command(command: &str) -> (&'static str, Vec<String>) {
    #[cfg(target_os = "windows")]
    {
        let git_bash = [
            "C:/Program Files/Git/bin/bash.exe",
            "C:/Program Files (x86)/Git/bin/bash.exe",
        ];
        for path in git_bash {
            if Path::new(path).exists() {
                return (path, vec!["-lc".to_string(), command.to_string()]);
            }
        }
        return (
            "powershell",
            vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                command.to_string(),
            ],
        );
    }

    #[cfg(not(target_os = "windows"))]
    {
        ("bash", vec!["-lc".to_string(), command.to_string()])
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
        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(_) = res {
                    let _ = tx.send(());
                }
            })?;

        watcher.watch(&path, RecursiveMode::Recursive)?;
        Ok(())
    }
}
