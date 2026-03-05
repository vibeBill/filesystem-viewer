mod app;
mod git;
mod runtime;
mod ui;

use anyhow::Result;
use app::App;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;

/// 主函数
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let working_dir = if args.len() > 1 { &args[1] } else { "." };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    execute!(stdout, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(working_dir)?;
    let change_rx = app.get_change_receiver();

    let mut file_watcher = app::FileWatcher::new(app.tx.take().unwrap())?;
    if let Err(e) = file_watcher.start(&app.working_dir) {
        app.error_message = Some(format!("文件监听失败：{}", e));
    }

    let result = runtime::run(&mut terminal, &mut app, change_rx);

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
