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

/// 渲染主界面
pub fn render(frame: &mut Frame, app: &mut App) {
    match app.state {
        AppState::Running => {
            render_main_view(frame, app);
        }
        AppState::Quit => {}
    }
}

/// 渲染主视图
fn render_main_view(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // 头部
            Constraint::Min(0),     // 文件/编辑器
            Constraint::Length(10), // 终端
            Constraint::Length(3),  // 状态栏
        ])
        .split(frame.area());

    // 计算列表高度
    let list_height = chunks[1].height as usize;
    app.set_list_height(list_height);

    render_header(frame, chunks[0], app);

    // 左右分栏布局
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(app.tree_width as u16), // 左侧目录树
            Constraint::Min(0),                        // 右侧预览/编辑器
        ])
        .split(chunks[1]);

    // 设置编辑器区域边界
    app.set_editor_area(main_chunks[1]);
    app.set_terminal_area(chunks[2]);

    render_file_list(frame, main_chunks[0], app);
    render_editor_pane(frame, main_chunks[1], app);
    render_terminal_pane(frame, chunks[2], app);
    render_status_bar(frame, chunks[3], app);

    // 如果显示搜索，覆盖显示搜索框
    if app.mode == AppMode::Search {
        render_search_box(frame, app);
    }

    // 如果显示帮助，覆盖显示
    if app.show_help {
        render_help(frame);
    }

    // 如果有错误，显示错误
    if let Some(_) = &app.error_message {
        render_error(frame, app);
    }

    // 如果有临时消息，显示消息
    render_status_message(frame, app);
}

/// 渲染头部
fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    // 检查 Git 仓库状态
    let git_status = if app.git_manager.is_git_repo() {
        "⎇ Git"
    } else {
        "📁 文件"
    };

    let header = Paragraph::new(format!(" {} - {}", git_status, app.working_dir))
        .style(Style::default().fg(Color::White).bg(colors::HEADER_BG))
        .alignment(Alignment::Center);

    frame.render_widget(header, area);
}

/// 渲染文件列表
fn render_file_list(frame: &mut Frame, area: Rect, app: &App) {
    let filtered = app.get_filtered_paths();

    if filtered.is_empty() {
        let msg = if app.git_manager.is_git_repo() {
            "当前目录没有文件"
        } else {
            "当前目录没有文件"
        };

        let empty_msg = Paragraph::new(msg)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center);
        frame.render_widget(empty_msg, area);
        return;
    }

    // 计算可见区域高度（减去上下边框）
    let inner_height = area.height.saturating_sub(2) as usize;

    // 只处理可见的文件项，大幅提升高性能大数据量下的响应速度
    let visible_items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(inner_height)
        .map(|(idx, path)| {
            let is_selected = idx == app.selected;

            // 检查是否是 hover 的行
            let is_hovered = if let (Some(h_row), Some(h_col)) = (app.hover_row, app.hover_col) {
                h_col >= area.x
                    && h_col < area.x + area.width
                    && h_row == area.y + 1 + (idx as i16 - app.scroll_offset as i16) as u16
            } else {
                false
            };

            // 获取文件信息
            let file = app.get_file_by_path(path);
            let (status, is_dir, depth) =
                file.map(|f| (f.status, f.is_dir, f.depth))
                    .unwrap_or((GitStatus::Clean, false, 0));

            let indent = "  ".repeat(depth);

            // 状态颜色
            let status_color = match status {
                GitStatus::Modified => colors::MODIFIED,
                GitStatus::Added => colors::ADDED,
                GitStatus::Deleted => colors::DELETED,
                GitStatus::Untracked => colors::UNTRACKED,
                GitStatus::Renamed => colors::RENAMED,
                GitStatus::Ignored => colors::IGNORED,
                GitStatus::Clean => colors::CLEAN,
                _ => colors::CLEAN,
            };

            let status_symbol = status.symbol();

            // 目录折叠/展开图标 (VSCode 风格)
            let fold_icon = if is_dir {
                if app.is_collapsed(path) {
                    " " // chevron right
                } else {
                    " " // chevron down
                }
            } else {
                "  "
            };

            // 如果没有图标字体支持，回退到普通字符
            let fold_icon = if fold_icon.chars().any(|c| c as u32 > 0x7F) {
                if is_dir {
                    if app.is_collapsed(path) {
                        "▶ "
                    } else {
                        "▼ "
                    }
                } else {
                    "  "
                }
            } else {
                fold_icon
            };

            let file_name = path
                .rsplit(|c| c == '/' || c == '\\')
                .next()
                .unwrap_or(path);
            let display_name = if is_dir {
                format!("{}/", file_name)
            } else {
                file_name.to_string()
            };

            let content = format!(
                "{}{} {:<2} {}",
                indent, fold_icon, status_symbol, display_name
            );

            // 样式优化
            let mut style = Style::default().fg(status_color);

            if is_selected {
                if app.focus == FocusArea::Tree {
                    style = style
                        .bg(Color::Rgb(50, 50, 80))
                        .add_modifier(Modifier::BOLD);
                } else {
                    style = style.bg(Color::Rgb(40, 40, 40));
                }
            } else if is_hovered {
                style = style.bg(Color::Rgb(35, 35, 35));
            }

            ListItem::new(content).style(style)
        })
        .collect();

    // 目录树标题
    let tree_title = format!(" EXPLORER: {} ", app.working_dir.to_uppercase());
    let border_style = if app.focus == FocusArea::Tree {
        Style::default().fg(Color::Blue)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let list = List::new(visible_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                tree_title,
                Style::default().add_modifier(Modifier::BOLD),
            ))
            .border_style(border_style),
    );

    frame.render_widget(list, area);
}

fn render_terminal_pane(frame: &mut Frame, area: Rect, app: &App) {
    let Some(tab) = app.terminal_tabs.get(app.active_terminal_tab) else {
        return;
    };

    let tabs = app
        .terminal_tabs
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            if idx == app.active_terminal_tab {
                format!("[{}]", t.name)
            } else {
                t.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" | ");

    let inner_height = area.height.saturating_sub(3) as usize;
    let visible = tab
        .output
        .iter()
        .skip(
            app.terminal_scroll
                .saturating_sub(inner_height.saturating_sub(1)),
        )
        .take(inner_height.saturating_sub(1))
        .map(|line| Line::from(line.as_str()))
        .collect::<Vec<_>>();

    let mut lines = visible;
    lines.push(Line::from(format!("> {}", tab.input)).style(Style::default().fg(Color::Yellow)));

    let border_color = if app.focus == FocusArea::Terminal {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    let terminal = Paragraph::new(lines).block(
        Block::default()
            .title(format!(" 终端 {} ", tabs))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)),
    );

    frame.render_widget(terminal, area);

    if app.focus == FocusArea::Terminal {
        let prompt_width = UnicodeWidthStr::width("> ") as u16;
        let input_width = UnicodeWidthStr::width(tab.input.as_str()) as u16;
        let max_x = area.x + area.width.saturating_sub(2);
        let cursor_x = (area.x + 1 + prompt_width + input_width).min(max_x);
        let cursor_y = area.y + area.height.saturating_sub(2);
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

/// 渲染状态栏
fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let stats = app.get_stats();

    let mode_text = match app.display_mode {
        DisplayMode::All => "全部",
        DisplayMode::Changed => "变更",
        DisplayMode::Tracked => "已跟踪",
    };

    // 获取当前选中文件的简要信息
    let file_info = app
        .selected_file()
        .map(|f| {
            let name = f
                .path
                .rsplit(|c| c == '/' || c == '\\')
                .next()
                .unwrap_or(&f.path);
            if f.is_dir {
                format!("📁 {}/", name)
            } else {
                format!("📄 {}", name)
            }
        })
        .unwrap_or_else(|| "无文件".to_string());

    // 状态栏分三部分：左侧 Git 统计，中间当前文件，右侧快捷键提示
    let stats_text = format!(
        " M:{} A:{} D:{} U:{} | 模式：{} | 刷新：{}s ",
        stats.modified,
        stats.added,
        stats.deleted,
        stats.untracked,
        mode_text,
        app.refresh_interval
    );

    let shortcuts_text = match app.focus {
        FocusArea::Tree => "Tab 切换 | Enter 编辑 | q 退出 | ? 帮助",
        FocusArea::Editor => "Ctrl+S 保存 | Esc 返回 | Tab 到终端",
        FocusArea::Terminal => "Enter 执行 | Ctrl+C 中断 | Ctrl+点击链接 打开浏览器",
    };

    // 计算各部分位置
    let stats_width = stats_text.len() as u16;
    let shortcuts_width = shortcuts_text.len() as u16;

    // 中间文件信息
    let max_file_width = area.width.saturating_sub(stats_width + shortcuts_width + 4);
    let file_display = if file_info.len() > max_file_width as usize {
        format!(
            "{}...",
            &file_info[..max_file_width.saturating_sub(3) as usize]
        )
    } else {
        file_info
    };

    let status_line = format!("{} {} {}", stats_text, file_display, shortcuts_text);

    let status_bar = Paragraph::new(status_line)
        .style(Style::default().fg(Color::White).bg(Color::DarkGray))
        .alignment(Alignment::Left);

    frame.render_widget(status_bar, area);
}

/// 渲染编辑器窗格（右侧）
fn render_editor_pane(frame: &mut Frame, area: Rect, app: &App) {
    // 检查是否有打开的文件
    if app.editor_path.is_empty() {
        // 没有打开的文件，显示预览
        render_preview_pane_inner(frame, area, app);
        return;
    }

    // 有打开的文件，显示编辑器
    let modified_flag = if app.editor_modified { " *" } else { "" };
    let save_hint = if app.editor_modified {
        " (Ctrl+S 保存)"
    } else {
        ""
    };

    // 编辑器标题 - 显示聚焦状态
    let editor_title = if app.focus == FocusArea::Editor {
        format!(
            " 编辑器：{}{}{} ",
            app.editor_path, modified_flag, save_hint
        )
    } else {
        format!(" 编辑器：{}{} ", app.editor_path, modified_flag)
    };

    let border_color = if app.focus == FocusArea::Editor {
        if app.editor_modified {
            Color::Red
        } else {
            Color::Yellow
        }
    } else {
        Color::Green
    };

    // 计算可见行数（减去上下边框）
    let inner_height = area.height.saturating_sub(2) as usize;

    // 使用 app 的 editor_scroll，不强制跟随光标
    // 光标移动时的滚动调整已在 editor_up/editor_down 等方法中处理
    let mut editor_scroll = app.editor_scroll;
    // 仅确保不超过底部
    if editor_scroll + inner_height > app.editor_content.len() {
        editor_scroll = app.editor_content.len().saturating_sub(inner_height);
    }

    // 渲染编辑器内容（带光标、diff 高亮和 VSCode 风格 gutter 标记）
    let lines: Vec<Line> = app
        .editor_content
        .iter()
        .skip(editor_scroll)
        .take(inner_height)
        .enumerate()
        .map(|(i, line)| {
            let line_num = i + editor_scroll + 1;
            let actual_line = i + editor_scroll;
            let is_cursor_line = actual_line == app.editor_cursor.0;

            // 检查该行是否有 git diff 标记
            let diff_type = app.editor_diff_lines.get(&line_num);

            // 检查该行是否被编辑修改（对比原始内容）
            let is_edited = app.editor_original_content.get(actual_line) != Some(&line.to_string());

            // Gutter 标记（VSCode 风格）
            let (gutter_char, gutter_color) = match diff_type {
                Some(DiffLineType::Added) => ("┃", Color::Green),
                Some(DiffLineType::Modified) => ("┃", Color::Rgb(80, 130, 220)),
                Some(DiffLineType::Deleted) => ("▸", Color::Red),
                None => {
                    if is_edited {
                        ("┃", Color::Yellow)
                    } else {
                        (" ", Color::DarkGray)
                    }
                }
            };

            // 行号颜色
            let line_num_color = if diff_type.is_some() || is_edited {
                gutter_color
            } else if is_cursor_line && app.focus == FocusArea::Editor {
                Color::White
            } else {
                Color::DarkGray
            };

            let gutter_span =
                Span::styled(gutter_char.to_string(), Style::default().fg(gutter_color));

            let line_num_span = Span::styled(
                format!("{:<4} ", line_num),
                Style::default().fg(line_num_color),
            );

            // 行背景色
            let line_bg = if is_cursor_line && app.focus == FocusArea::Editor {
                Color::Rgb(40, 40, 60)
            } else if diff_type == Some(&DiffLineType::Added) {
                Color::Rgb(15, 30, 15)
            } else if diff_type == Some(&DiffLineType::Modified) || is_edited {
                Color::Rgb(20, 25, 35)
            } else {
                Color::Reset
            };

            let text_color = if is_cursor_line && app.focus == FocusArea::Editor {
                Color::White
            } else if line.is_empty() {
                Color::DarkGray
            } else {
                Color::Gray
            };

            Line::from(vec![
                gutter_span,
                line_num_span,
                Span::styled(line.to_string(), Style::default().fg(text_color)),
            ])
            .style(Style::default().bg(line_bg))
        })
        .collect();

    let editor = Paragraph::new(lines)
        .style(Style::default().fg(Color::Gray).bg(colors::EDITOR_BG))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(editor_title)
                .border_style(Style::default().fg(border_color)),
        );

    frame.render_widget(editor, area);

    // 如果聚焦在编辑器，显示光标
    if app.focus == FocusArea::Editor {
        // 计算光标前文本的实际显示宽度（支持中文等宽字符）
        let cursor_display_offset = if let Some(line) = app.editor_content.get(app.editor_cursor.0)
        {
            let prefix: String = line.chars().take(app.editor_cursor.1).collect();
            UnicodeWidthStr::width(prefix.as_str()) as u16
        } else {
            0
        };
        frame.set_cursor_position((
            area.x + 1 + 1 + 5 + cursor_display_offset, // 边框 1 + gutter 1 + 行号 4 + 空格 1 = 7
            area.y + 1 + (app.editor_cursor.0 - editor_scroll) as u16,
        ));
    }
}

/// 渲染预览窗格（右侧）- 内部函数
fn render_preview_pane_inner(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(file) = app.selected_file() {
        if file.is_dir {
            // 目录 - 显示目录信息
            let dir_children = app
                .files
                .iter()
                .filter(|f| f.path.starts_with(&file.path) && f.path != file.path)
                .count();

            let dir_info = vec![
                Line::from(""),
                Line::from(format!("  📁 {}", file.path)),
                Line::from(""),
                Line::from(format!("  子项数量：{}", dir_children)),
                Line::from(""),
                Line::from("  按 Enter 展开目录"),
            ];
            let preview = Paragraph::new(dir_info)
                .style(Style::default().fg(Color::Gray))
                .block(
                    Block::default()
                        .title(" 目录预览 ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan)),
                );
            frame.render_widget(preview, area);
        } else {
            // 文件 - 显示文件内容预览
            let full_path = Path::new(&app.working_dir).join(&file.path);
            let content = std::fs::read_to_string(&full_path)
                .unwrap_or_else(|_| String::from("无法读取文件（可能是二进制文件）"));

            // 获取文件状态描述
            let status_desc = match file.status {
                GitStatus::Modified => "已修改",
                GitStatus::Added => "新增",
                GitStatus::Deleted => "删除",
                GitStatus::Untracked => "未跟踪",
                GitStatus::Renamed => "重命名",
                GitStatus::Clean => "无变更",
                _ => "未知",
            };

            // 获取 Git diff 来显示修改
            let mut diff_lines: Vec<i32> = Vec::new();
            if file.status == GitStatus::Modified {
                // 获取 git diff 来识别修改的行
                diff_lines = get_git_diff_lines(&app.working_dir, &file.path);
            }

            // 构建预览内容（带文件信息头部）
            let mut lines: Vec<Line> = vec![
                Line::from(
                    format!(" 📄 {} ", file.path)
                        .bg(Color::DarkGray)
                        .fg(Color::White),
                ),
                Line::from(format!(
                    " 状态：{} | 大小：{} 行",
                    status_desc,
                    content.lines().count()
                )),
                Line::from(""),
            ];

            // 添加文件内容预览（限制行数）
            let max_preview_lines = area.height.saturating_sub(4) as usize; // 减去标题和信息行
            let content_lines: Vec<&str> = content.lines().collect();

            for (i, line) in content_lines.iter().take(max_preview_lines).enumerate() {
                let line_num = i + 1;
                // 检查该行是否在 diff 中（被修改）
                let is_in_diff = diff_lines.contains(&(line_num as i32));

                if is_in_diff {
                    // 修改的行用黄色高亮
                    lines.push(
                        Line::from(format!("{:<4} {}", line_num, line))
                            .style(Style::default().bg(Color::Rgb(40, 35, 0)).fg(Color::Yellow)),
                    );
                } else {
                    lines.push(
                        Line::from(format!("{:<4} {}", line_num, line))
                            .style(Style::default().fg(Color::Gray)),
                    );
                }
            }

            let preview = Paragraph::new(lines)
                .style(Style::default().bg(colors::EDITOR_BG))
                .block(
                    Block::default()
                        .title(format!(" 预览：{} ", file.path))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Green)),
                );
            frame.render_widget(preview, area);
        }
    } else {
        let empty_msg = vec![
            Line::from(""),
            Line::from(""),
            Line::from("  没有选中的文件"),
            Line::from(""),
            Line::from("  使用 ↑↓ 或鼠标选择文件"),
            Line::from("  按 Enter 或 e 打开编辑器"),
        ];
        let empty = Paragraph::new(empty_msg)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Left);
        frame.render_widget(empty, area);
    }
}

/// 渲染帮助窗口
fn render_help(frame: &mut Frame) {
    let area = centered_rect(65, 80, frame.area());

    let help_text = vec![
        Line::from(""),
        Line::from(" │ 目录树导航 (左侧) "),
        Line::from(" │ "),
        Line::from(" │  ↑        上一个文件        ↓         下一个文件"),
        Line::from(" │  PageUp   向上翻页          PageDown  向下翻页"),
        Line::from(" │  Home     跳到开头          End       跳到末尾"),
        Line::from(" │  ←        折叠目录          →         展开目录"),
        Line::from(" │  Space    切换折叠状态"),
        Line::from(""),
        Line::from(" │ 文件操作 "),
        Line::from(" │ "),
        Line::from(" │  Enter    打开文件/折叠目录  m         切换显示模式"),
        Line::from(" │  r        手动刷新          0-9       设置刷新间隔 (秒)"),
        Line::from(" │  Ctrl+P   搜索文件          Tab       循环切换焦点"),
        Line::from(""),
        Line::from(" │ 编辑器操作 (右侧) - VSCode 风格 "),
        Line::from(" │ "),
        Line::from(" │  ↑↓←→     移动光标          Home/End  行首/行尾"),
        Line::from(" │  PageUp   向上翻页          PageDown  向下翻页"),
        Line::from(" │  Ctrl+S   保存文件          Ctrl+Z    撤销"),
        Line::from(" │  Ctrl+A   全选/跳到开头     Ctrl+Home 跳到文件开头"),
        Line::from(" │  Ctrl+End 跳到文件末尾      Ctrl+F    查找"),
        Line::from(" │  Ctrl+W   关闭编辑器        Esc       返回目录树"),
        Line::from(" │  Backspace 删除前字符       Delete    删除后字符"),
        Line::from(""),
        Line::from(" │ 鼠标操作 "),
        Line::from(" │ "),
        Line::from(" │  滚轮     滚动              左键点击  选择文件/定位光标"),
        Line::from(" │  中键点击 整页翻页"),
        Line::from(""),
        Line::from(" │ Git 状态 "),
        Line::from(" │ "),
        Line::from(" │  M (黄)   已修改     A (绿)   新增        D (红)   删除"),
        Line::from(" │  ?? (灰)  未跟踪     R (紫)   重命名"),
        Line::from(""),
        Line::from(" │ Diff 高亮 "),
        Line::from(" │ "),
        Line::from(" │  绿色行号/背景  表示修改的行"),
        Line::from(""),
        Line::from(" │ 终端操作 (底部) "),
        Line::from(" │ "),
        Line::from(" │  Enter    执行命令          Ctrl+T    新建终端 Tab"),
        Line::from(" │  Ctrl+←   上一个 Tab        Ctrl+→    下一个 Tab"),
        Line::from(" │  Ctrl+C   中断当前命令      Ctrl+点击  打开链接"),
        Line::from(" │  Esc      返回目录树"),
        Line::from(""),
        Line::from(" │ 其他 "),
        Line::from(" │ "),
        Line::from(" │  ?        切换帮助          q         退出应用"),
        Line::from(""),
    ];

    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::White).bg(colors::HELP_BG))
        .block(
            Block::default()
                .title(" 帮助 - 按 ? 关闭 ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(Clear, area);
    frame.render_widget(help, area);
}

/// 渲染错误窗口
fn render_error(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 30, frame.area());

    let error_msg = app.error_message.clone().unwrap_or_default();
    let error_text = vec![
        Line::from(""),
        Line::from(format!("  {}", error_msg)),
        Line::from(""),
        Line::from(" 按任意键继续..."),
    ];

    let error = Paragraph::new(error_text)
        .style(Style::default().fg(Color::Red).bg(Color::Black))
        .block(
            Block::default()
                .title(" 错误 ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(Clear, area);
    frame.render_widget(error, area);
}

/// 渲染临时消息（保存成功等提示）
fn render_status_message(frame: &mut Frame, app: &App) {
    if let Some(msg) = &app.status_message {
        // 检查消息是否过期（2 秒后消失）
        if let Some(time) = app.status_message_time {
            if time.elapsed().as_secs() >= 2 {
                return;
            }
        }

        let area = centered_rect(40, 15, frame.area());

        let msg_text = vec![
            Line::from(""),
            Line::from(format!("  {}", msg)),
            Line::from(""),
        ];

        let msg_widget = Paragraph::new(msg_text)
            .style(Style::default().fg(Color::Green).bg(Color::Black))
            .block(
                Block::default()
                    .title(" 提示 ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green)),
            )
            .alignment(Alignment::Center);

        frame.render_widget(Clear, area);
        frame.render_widget(msg_widget, area);
    }
}

/// 渲染搜索框
fn render_search_box(frame: &mut Frame, app: &App) {
    let area = centered_rect(50, 20, frame.area());

    let search_text = vec![
        Line::from(""),
        Line::from(format!("  搜索：{}", app.search_query)),
        Line::from(""),
        Line::from("  按 Enter 确认，Esc 取消"),
    ];

    let search_widget = Paragraph::new(search_text)
        .style(Style::default().fg(Color::Cyan).bg(Color::Black))
        .block(
            Block::default()
                .title(" 搜索文件 ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(Clear, area);
    frame.render_widget(search_widget, area);

    // 设置光标位置
    frame.set_cursor_position((area.x + 5 + app.search_query.len() as u16, area.y + 2));
}

/// 获取 git diff 中修改的行号
fn get_git_diff_lines(working_dir: &str, file_path: &str) -> Vec<i32> {
    let mut modified_lines = Vec::new();

    // 使用 git diff --unified=0 来获取修改的行号
    let output = Command::new("git")
        .args(["diff", "--unified=0", "--no-color", "--", file_path])
        .current_dir(working_dir)
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            // 解析 @@ -X,Y +A,B @@ 格式的 hunk 头
            if line.starts_with("@@") {
                // 提取添加的行信息（+ 后面的部分）
                if let Some(plus_part) = line.split('+').nth(1) {
                    if let Some(line_num_str) = plus_part.split(',').next() {
                        if let Ok(line_num) = line_num_str.trim().parse::<i32>() {
                            modified_lines.push(line_num);
                        }
                    }
                }
            }
        }
    }

    modified_lines
}

/// 创建居中矩形
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
