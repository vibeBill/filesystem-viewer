use crate::app::{App, AppMode, AppState, DisplayMode, FocusArea};
use crate::git::{DiffLineType, GitStatus};
use ratatui::{prelude::*, widgets::*, Frame};
use std::path::Path;
use std::process::Command;
use unicode_width::UnicodeWidthStr;

/// 颜色定义
mod colors {
    use ratatui::style::Color;

    pub const MODIFIED: Color = Color::Yellow;
    pub const ADDED: Color = Color::Green;
    pub const DELETED: Color = Color::Red;
    pub const UNTRACKED: Color = Color::Gray;
    pub const RENAMED: Color = Color::Magenta;
    pub const IGNORED: Color = Color::Rgb(100, 100, 100);
    pub const CLEAN: Color = Color::White;
    pub const HEADER_BG: Color = Color::Blue;
    pub const HELP_BG: Color = Color::Black;
    pub const EDITOR_BG: Color = Color::Black;
}

include!("ui/layout_render.rs");
include!("ui/tree_status.rs");
include!("ui/editor_preview.rs");
include!("ui/overlays.rs");
