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
        FocusArea::Editor => "Ctrl+S 保存 | Esc 返回 | q 保存返回",
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
