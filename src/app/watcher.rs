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
