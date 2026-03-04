mod app;
mod git;
mod ui;

use anyhow::Result;
use app::{App, AppMode, AppState, FocusArea};
use crossterm::{
    event::{
        self, Event, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::time::Duration;

/// 主函数
fn main() -> Result<()> {
    // 获取工作目录，默认为当前目录
    let args: Vec<String> = std::env::args().collect();
    let working_dir = if args.len() > 1 { &args[1] } else { "." };

    // 初始化终端
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    // 启用鼠标支持
    execute!(stdout, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 创建应用
    let mut app = App::new(working_dir)?;

    // 获取文件变更接收器
    let change_rx = app.get_change_receiver();

    // 启动文件监听
    let mut file_watcher = app::FileWatcher::new(app.tx.take().unwrap())?;
    if let Err(e) = file_watcher.start(&app.working_dir) {
        app.error_message = Some(format!("文件监听失败：{}", e));
    }

    // 主循环
    let result = run(&mut terminal, &mut app, change_rx);

    // 恢复终端
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    execute!(
        terminal.backend_mut(),
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("错误：{}", e);
        std::process::exit(1);
    }

    Ok(())
}

/// 主循环
fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    change_rx: Option<std::sync::mpsc::Receiver<()>>,
) -> Result<()> {
    // 响应时间（约 60 FPS），提升鼠标 hover 和滚动的丝滑度
    let refresh_timeout = Duration::from_millis(16);

    loop {
        app.poll_terminal_output();

        // 绘制界面
        terminal.draw(|frame| ui::render(frame, app))?;

        // 检查是否需要刷新
        if app.should_refresh() && app.state == AppState::Running {
            app.refresh_files()?;
        }

        // 等待事件
        if let Some(rx) = &change_rx {
            // 有文件监听通道
            if event::poll(refresh_timeout)? {
                match event::read()? {
                    Event::Key(key) => {
                        // 只处理按键按下事件，避免重复
                        if key.kind == KeyEventKind::Press {
                            if handle_key_event(app, key) {
                                break;
                            }
                        }
                    }
                    Event::Mouse(mouse) => {
                        if handle_mouse_event(app, mouse) {
                            break;
                        }
                    }
                    _ => {}
                }
            } else {
                // 超时，检查通道
                if rx.try_recv().is_ok() && app.state == AppState::Running {
                    app.refresh_files()?;
                }
            }
        } else {
            // 无文件监听通道，轮询事件
            if event::poll(refresh_timeout)? {
                match event::read()? {
                    Event::Key(key) => {
                        // 只处理按键按下事件，避免重复
                        if key.kind == KeyEventKind::Press {
                            if handle_key_event(app, key) {
                                break;
                            }
                        }
                    }
                    Event::Mouse(mouse) => {
                        if handle_mouse_event(app, mouse) {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

/// 处理鼠标事件
/// 返回 true 表示退出
fn handle_mouse_event(app: &mut App, mouse: MouseEvent) -> bool {
    // 更新 hover 位置
    app.hover_row = Some(mouse.row);
    app.hover_col = Some(mouse.column);

    // 如果显示帮助，点击关闭
    if app.show_help {
        app.show_help = false;
        return false;
    }

    // 如果有错误，点击继续
    if app.error_message.is_some() {
        app.error_message = None;
        return false;
    }

    match mouse.kind {
        // 鼠标滚动 - 根据聚焦区域决定滚动目标 (逐行滚动提升手感)
        MouseEventKind::ScrollUp => {
            if app.focus == FocusArea::Editor && !app.editor_content.is_empty() {
                app.editor_scroll_up(2);
            } else if app.focus == FocusArea::Terminal {
                app.terminal_scroll_up(2);
            } else {
                app.scroll_up(2);
            }
        }
        MouseEventKind::ScrollDown => {
            if app.focus == FocusArea::Editor && !app.editor_content.is_empty() {
                app.editor_scroll_down(2);
            } else if app.focus == FocusArea::Terminal {
                app.terminal_scroll_down(2);
            } else {
                app.scroll_down(2);
            }
        }
        // 鼠标左键点击
        MouseEventKind::Down(MouseButton::Left) => {
            app.handle_mouse_click(
                mouse.row,
                mouse.column,
                MouseEventKind::Down(MouseButton::Left),
            );
        }
        // 鼠标中键点击 - 开始拖拽滚动
        MouseEventKind::Down(MouseButton::Middle) => {
            app.handle_mouse_click(
                mouse.row,
                mouse.column,
                MouseEventKind::Down(MouseButton::Middle),
            );
        }
        // 鼠标中键拖拽 - 自由滚动
        MouseEventKind::Drag(MouseButton::Middle) => {
            app.handle_middle_drag(mouse.row, mouse.column);
        }
        // 鼠标中键释放 - 停止拖拽
        MouseEventKind::Up(MouseButton::Middle) => {
            app.stop_middle_drag();
        }
        _ => {}
    }

    false
}

/// 处理键盘事件
/// 返回 true 表示退出
fn handle_key_event(app: &mut App, key: KeyEvent) -> bool {
    // 如果显示帮助，按任意键关闭
    if app.show_help {
        app.show_help = false;
        return false;
    }

    // 如果有错误，按任意键继续
    if app.error_message.is_some() {
        app.error_message = None;
        return false;
    }

    // 如果在搜索模式，处理搜索输入
    if app.mode == AppMode::Search {
        return handle_search_event(app, key);
    }

    // 根据聚焦区域处理事件
    match app.focus {
        FocusArea::Editor => {
            return handle_editor_event(app, key);
        }
        FocusArea::Tree => {
            return handle_tree_event(app, key);
        }
        FocusArea::Terminal => {
            return handle_terminal_event(app, key);
        }
    }
}

/// 处理搜索模式的事件
fn handle_search_event(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        // Esc 或 Enter 退出搜索
        KeyCode::Esc | KeyCode::Enter => {
            app.mode = AppMode::Normal;
        }
        // Backspace 删除
        KeyCode::Backspace => {
            app.search_backspace();
        }
        // 普通字符输入
        KeyCode::Char(c) if c.is_ascii() => {
            app.search_input(c);
        }
        _ => {}
    }
    false
}

/// 处理目录树区域的键盘事件
fn handle_tree_event(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        // 退出
        KeyCode::Char('q') => {
            app.quit();
            return true;
        }

        // 上下导航
        KeyCode::Up => {
            app.select_previous();
        }

        KeyCode::Down => {
            app.select_next();
        }

        // 折叠目录（左箭头）
        KeyCode::Left => {
            app.toggle_collapse();
        }

        // 展开目录（右箭头）
        KeyCode::Right => {
            app.toggle_collapse();
        }

        // 空格键切换折叠
        KeyCode::Char(' ') => {
            app.toggle_collapse();
        }

        // 切换聚焦区域
        KeyCode::Tab => {
            app.toggle_focus();
        }

        // 切换显示模式
        KeyCode::Char('m') => {
            app.toggle_display_mode();
        }

        // 手动刷新
        KeyCode::Char('r') => {
            let _ = app.refresh_files();
        }

        // Enter 键：文件打开编辑器，文件夹展开/收起
        KeyCode::Enter => {
            let _ = app.open_editor();
        }

        // 切换帮助
        KeyCode::Char('?') => {
            app.toggle_help();
        }

        // 设置刷新间隔 (0-9)
        KeyCode::Char(c) if c.is_ascii_digit() => {
            app.refresh_interval = c.to_digit(10).unwrap() as u64;
        }

        // Page Up / Page Down - 整页滚动
        KeyCode::PageUp => {
            app.page_up();
        }

        KeyCode::PageDown => {
            app.page_down();
        }

        // Home / End
        KeyCode::Home => {
            app.selected = 0;
            app.scroll_offset = 0;
        }

        KeyCode::End => {
            let filtered = app.get_filtered_paths();
            app.selected = filtered.len().saturating_sub(1);
        }

        // 搜索文件（按 / 键或 Ctrl+P - VSCode 风格）
        KeyCode::Char('/') | KeyCode::Char('p')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.toggle_search();
        }

        // Ctrl+O - 打开文件（VSCode 风格）
        KeyCode::Char('o')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            if let Some(file) = app.selected_file() {
                if !file.is_dir {
                    let _ = app.open_editor();
                }
            }
        }

        _ => {}
    }

    false
}

/// 处理终端模式的事件
fn handle_terminal_event(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => app.focus = FocusArea::Tree,
        KeyCode::Enter => app.terminal_execute(),
        KeyCode::Backspace => app.terminal_backspace(),
        KeyCode::Tab => app.toggle_focus(),
        KeyCode::Up => app.terminal_scroll_up(1),
        KeyCode::Down => app.terminal_scroll_down(1),
        KeyCode::Left
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.prev_terminal_tab();
        }
        KeyCode::Right
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.next_terminal_tab();
        }
        KeyCode::Char('t')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.create_terminal_tab();
        }
        KeyCode::Char(c) => app.terminal_input_char(c),
        _ => {}
    }

    false
}

/// 处理编辑器模式的事件
fn handle_editor_event(app: &mut App, key: KeyEvent) -> bool {
    // 先处理特殊按键
    match key.code {
        // 切换到目录树聚焦（Esc 或 Ctrl+K 然后 Ctrl+S 类似 VSCode）
        KeyCode::Esc => {
            app.exit_editor();
        }

        // 保存（Ctrl+S - VSCode 风格）
        KeyCode::Char('s')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            if let Err(e) = app.editor_save() {
                app.error_message = Some(format!("保存失败：{}", e));
            }
        }

        // 撤销（Ctrl+Z - VSCode 风格）
        KeyCode::Char('z')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.editor_undo();
        }

        // 全选（Ctrl+A - VSCode 风格）- 跳转到文件开头
        KeyCode::Char('a')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.editor_cursor = (0, 0);
            app.editor_scroll = 0;
        }

        // 跳到文件开头（Ctrl+Home - VSCode 风格）
        KeyCode::Home
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.editor_cursor = (0, 0);
            app.editor_scroll = 0;
        }

        // 跳到文件末尾（Ctrl+End - VSCode 风格）
        KeyCode::End
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            if let Some(last_line) = app.editor_content.last() {
                app.editor_cursor = (app.editor_content.len() - 1, last_line.chars().count());
            }
            app.editor_scroll = app.editor_content.len().saturating_sub(app.list_height);
        }

        // 查找（Ctrl+F - VSCode 风格）- 暂时同搜索功能
        KeyCode::Char('f')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            // 暂时实现为搜索文件，后续可实现文件内查找
            app.exit_editor();
            app.toggle_search();
        }

        // 关闭编辑器（Ctrl+W - VSCode 风格）
        KeyCode::Char('w')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            let _ = app.editor_save();
            app.exit_editor();
        }

        // 退出并保存
        KeyCode::Char('q') => {
            let _ = app.editor_save();
            app.exit_editor();
        }

        // 上下左右移动
        KeyCode::Up => {
            app.editor_up();
        }

        KeyCode::Down => {
            app.editor_down();
        }

        KeyCode::Left => {
            app.editor_left();
        }

        KeyCode::Right => {
            app.editor_right();
        }

        // Page Up / Page Down
        KeyCode::PageUp => {
            app.editor_page_up();
        }

        KeyCode::PageDown => {
            app.editor_page_down();
        }

        // Home / End
        KeyCode::Home => {
            app.editor_cursor.1 = 0;
        }

        KeyCode::End => {
            if let Some(line) = app.editor_content.get(app.editor_cursor.0) {
                app.editor_cursor.1 = line.chars().count();
            }
        }

        // Backspace
        KeyCode::Backspace => {
            app.editor_backspace();
        }

        // Delete
        KeyCode::Delete => {
            app.editor_delete();
        }

        // Enter - 插入新行
        KeyCode::Enter => {
            app.editor_insert_newline();
        }

        // Tab - 切换聚焦区域（编辑器 -> 终端 -> 目录树）
        KeyCode::Tab => {
            app.toggle_focus();
        }

        // 普通字符输入
        KeyCode::Char(c) => {
            app.editor_insert(c);
        }

        _ => {}
    }

    false
}
