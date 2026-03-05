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
            Constraint::Length(3), // 头部
            Constraint::Min(0),    // 主内容
            Constraint::Length(3), // 状态栏
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

    render_file_list(frame, main_chunks[0], app);
    render_editor_pane(frame, main_chunks[1], app); // 直接渲染编辑器窗格
    render_status_bar(frame, chunks[2], app);

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
