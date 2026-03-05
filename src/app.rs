use crate::git::{DiffLineType, FileEntry, GitStatus, GitStatusManager};
use anyhow::Result;
use crossterm::event::{MouseButton, MouseEventKind};
use notify::{RecursiveMode, Watcher};
use ratatui::layout::Rect;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc;
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
    Tree,   // 左侧目录树
    Editor, // 右侧编辑器
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
}

include!("app/watcher.rs");

include!("app/core_impl.rs");
include!("app/tree_impl.rs");
include!("app/editor_impl.rs");
include!("app/interaction_impl.rs");
