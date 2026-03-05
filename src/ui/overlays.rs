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
        Line::from(" │  Ctrl+P   搜索文件          Tab       切换到编辑器"),
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
