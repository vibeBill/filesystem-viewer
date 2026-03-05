impl App {
    pub fn open_editor(&mut self) -> Result<()> {
        if let Some(file) = self.selected_file() {
            if file.is_dir {
                // 如果是目录，切换折叠状态
                self.toggle_collapse();
                return Ok(());
            }

            let file_path = file.path.clone();
            let full_path = Path::new(&self.working_dir).join(&file_path);
            let content = std::fs::read_to_string(&full_path).unwrap_or_else(|_| String::new());

            self.editor_path = full_path.to_string_lossy().to_string();
            self.editor_content = content.lines().map(|s| s.to_string()).collect();
            // 保存原始内容用于 diff 对比
            self.editor_original_content = self.editor_content.clone();
            self.editor_cursor = (0, 0);
            self.editor_scroll = 0;
            self.editor_h_scroll = 0;
            self.editor_modified = false;
            self.editor_undo_stack.clear();
            // 加载 git diff 行信息
            self.editor_diff_lines = self.git_manager.get_file_diff_lines(&file_path);
            self.focus = FocusArea::Editor;
        }
        Ok(())
    }

    /// 编辑器中向上移动光标
    pub fn editor_up(&mut self) {
        if self.editor_cursor.0 > 0 {
            self.editor_cursor.0 -= 1;
            // 调整列位置
            if self.editor_cursor.1 > self.editor_content[self.editor_cursor.0].chars().count() {
                self.editor_cursor.1 = self.editor_content[self.editor_cursor.0].chars().count();
            }
            // 调整滚动
            if self.editor_scroll > self.editor_cursor.0 {
                self.editor_scroll = self.editor_cursor.0;
            }
        }
    }

    /// 编辑器中向下移动光标
    pub fn editor_down(&mut self) {
        if self.editor_cursor.0 < self.editor_content.len().saturating_sub(1) {
            self.editor_cursor.0 += 1;
            // 调整列位置
            if self.editor_cursor.1 > self.editor_content[self.editor_cursor.0].chars().count() {
                self.editor_cursor.1 = self.editor_content[self.editor_cursor.0].chars().count();
            }
            // 调整滚动
            let visible_lines = self.list_height.max(10);
            if self.editor_cursor.0 >= self.editor_scroll + visible_lines {
                self.editor_scroll = self.editor_cursor.0 - visible_lines + 1;
            }
        }
    }

    /// 编辑器整页向上滚动
    pub fn editor_page_up(&mut self) {
        let visible_lines = self.list_height.max(10);
        if self.editor_scroll >= visible_lines {
            self.editor_scroll -= visible_lines;
        } else {
            self.editor_scroll = 0;
        }
        if self.editor_cursor.0 < self.editor_scroll {
            self.editor_cursor.0 = self.editor_scroll;
        }
    }

    /// 编辑器整页向下滚动
    pub fn editor_page_down(&mut self) {
        let visible_lines = self.list_height.max(10);
        let max_scroll = self.editor_content.len().saturating_sub(visible_lines);
        if self.editor_scroll + visible_lines * 2 <= self.editor_content.len() {
            self.editor_scroll += visible_lines;
        } else {
            self.editor_scroll = max_scroll;
        }
        let max_cursor = self.editor_scroll + visible_lines - 1;
        if self.editor_cursor.0 > max_cursor.min(self.editor_content.len().saturating_sub(1)) {
            self.editor_cursor.0 = max_cursor.min(self.editor_content.len().saturating_sub(1));
        }
    }

    /// 编辑器中向左移动光标
    pub fn editor_left(&mut self) {
        if self.editor_cursor.1 > 0 {
            self.editor_cursor.1 -= 1;
        }
    }

    /// 编辑器中向右移动光标
    pub fn editor_right(&mut self) {
        let line_len = self.editor_content[self.editor_cursor.0].chars().count();
        if self.editor_cursor.1 < line_len {
            self.editor_cursor.1 += 1;
        }
    }

    /// 编辑器中删除字符（Backspace）
    pub fn editor_backspace(&mut self) {
        self.push_undo();
        if self.editor_cursor.1 > 0 {
            let line = &mut self.editor_content[self.editor_cursor.0];
            let mut chars: Vec<char> = line.chars().collect();
            chars.remove(self.editor_cursor.1 - 1);
            *line = chars.iter().collect();
            self.editor_cursor.1 -= 1;
            self.editor_modified = true;
        } else if self.editor_cursor.0 > 0 {
            // 合并到上一行
            let prev_len = self.editor_content[self.editor_cursor.0 - 1].len();
            let current_line = self.editor_content.remove(self.editor_cursor.0);
            self.editor_content[self.editor_cursor.0 - 1].push_str(&current_line);
            self.editor_cursor.0 -= 1;
            self.editor_cursor.1 = prev_len;
            self.editor_modified = true;
        }
    }

    /// 编辑器中删除字符（Delete）
    pub fn editor_delete(&mut self) {
        self.push_undo();
        let line_len = self.editor_content[self.editor_cursor.0].chars().count();
        if self.editor_cursor.1 < line_len {
            let line = &mut self.editor_content[self.editor_cursor.0];
            let mut chars: Vec<char> = line.chars().collect();
            chars.remove(self.editor_cursor.1);
            *line = chars.iter().collect();
            self.editor_modified = true;
        } else if self.editor_cursor.0 < self.editor_content.len() - 1 {
            // 合并下一行
            let next_line = self.editor_content.remove(self.editor_cursor.0 + 1);
            self.editor_content[self.editor_cursor.0].push_str(&next_line);
            self.editor_modified = true;
        }
    }

    /// 编辑器中插入字符
    pub fn editor_insert(&mut self, c: char) {
        self.push_undo();
        let line = &mut self.editor_content[self.editor_cursor.0];
        let mut chars: Vec<char> = line.chars().collect();
        chars.insert(self.editor_cursor.1, c);
        *line = chars.iter().collect();
        self.editor_cursor.1 += 1;
        self.editor_modified = true;
    }

    /// 编辑器中插入新行
    pub fn editor_insert_newline(&mut self) {
        self.push_undo();
        let current_line = self.editor_content[self.editor_cursor.0].clone();
        let before: String = current_line.chars().take(self.editor_cursor.1).collect();
        let after: String = current_line.chars().skip(self.editor_cursor.1).collect();

        self.editor_content[self.editor_cursor.0] = before;
        self.editor_content.insert(self.editor_cursor.0 + 1, after);
        self.editor_cursor.0 += 1;
        self.editor_cursor.1 = 0;
        self.editor_modified = true;
    }

    /// 推送撤销历史
    fn push_undo(&mut self) {
        // 限制撤销栈大小为 100
        if self.editor_undo_stack.len() >= 100 {
            self.editor_undo_stack.remove(0);
        }
        self.editor_undo_stack.push(self.editor_content.clone());
    }

    /// 撤销（Ctrl+Z）
    pub fn editor_undo(&mut self) {
        if let Some(prev_state) = self.editor_undo_stack.pop() {
            self.editor_content = prev_state;
            self.editor_modified = true;
            // 调整光标
            if self.editor_cursor.0 >= self.editor_content.len() {
                self.editor_cursor.0 = self.editor_content.len().saturating_sub(1);
            }
        }
    }

    /// 保存文件
    pub fn editor_save(&mut self) -> Result<()> {
        let content = self.editor_content.join("\n");
        std::fs::write(&self.editor_path, content)?;
        self.editor_modified = false;
        // 显示保存成功提示
        self.status_message = Some("✓ 文件已保存".to_string());
        self.status_message_time = Some(std::time::Instant::now());
        Ok(())
    }

    /// 退出编辑器（切换到目录树聚焦）
    pub fn exit_editor(&mut self) {
        // 如果有未保存的修改，提示用户
        if self.editor_modified {
            self.status_message = Some("⚠ 未保存的修改已丢弃".to_string());
            self.status_message_time = Some(std::time::Instant::now());
        }
        self.focus = FocusArea::Tree;
    }

    /// 切换聚焦区域
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            FocusArea::Tree => FocusArea::Editor,
            FocusArea::Editor => FocusArea::Tree,
        };
    }

    /// 设置编辑器区域
    pub fn set_editor_area(&mut self, area: Rect) {
        self.editor_area = Some(area);
    }

}
