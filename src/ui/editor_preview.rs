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
