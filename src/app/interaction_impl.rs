impl App {
    /// 整页向上滚动
    pub fn page_up(&mut self) {
        if self.scroll_offset >= self.list_height {
            self.scroll_offset -= self.list_height;
        } else {
            self.scroll_offset = 0;
        }
        if self.selected > self.scroll_offset {
            self.selected = self.scroll_offset;
        }
    }

    /// 整页向下滚动
    pub fn page_down(&mut self) {
        let filtered_len = self.cached_filtered.len();
        if self.scroll_offset + self.list_height * 2 < filtered_len {
            self.scroll_offset += self.list_height;
        } else {
            self.scroll_offset = filtered_len.saturating_sub(self.list_height);
        }
        let new_selected = (self.selected + self.list_height).min(filtered_len - 1);
        if new_selected > self.selected {
            self.selected = new_selected;
        }
    }

    /// 向上滚动 (逐行)
    pub fn scroll_up(&mut self, lines: usize) {
        for _ in 0..lines {
            if self.scroll_offset > 0 {
                self.scroll_offset -= 1;
            }
            if self.selected > self.scroll_offset + self.list_height - 1 {
                self.selected = self.scroll_offset + self.list_height - 1;
            }
        }
    }

    /// 向下滚动 (逐行)
    pub fn scroll_down(&mut self, lines: usize) {
        let filtered_len = self.cached_filtered.len();
        for _ in 0..lines {
            if self.scroll_offset + self.list_height < filtered_len {
                self.scroll_offset += 1;
            }
            if self.selected < self.scroll_offset {
                self.selected = self.scroll_offset;
            }
        }
    }

    /// 编辑器向上滚动 (逐行)
    pub fn editor_scroll_up(&mut self, lines: usize) {
        if self.editor_scroll >= lines {
            self.editor_scroll -= lines;
        } else {
            self.editor_scroll = 0;
        }
    }

    /// 编辑器向下滚动 (逐行)
    pub fn editor_scroll_down(&mut self, lines: usize) {
        let visible_lines = self.list_height.max(10);
        let max_scroll = self.editor_content.len().saturating_sub(visible_lines);
        if self.editor_scroll + lines <= max_scroll {
            self.editor_scroll += lines;
        } else {
            self.editor_scroll = max_scroll;
        }
    }

    /// 处理鼠标点击事件
    pub fn handle_mouse_click(&mut self, row: u16, column: u16, kind: MouseEventKind) -> bool {
        // 更新 hover 位置
        self.hover_row = Some(row);
        self.hover_col = Some(column);

        match kind {
            // 左键点击
            MouseEventKind::Down(MouseButton::Left) => {
                // 检查是否点击在编辑器区域
                if let Some(editor_area) = self.editor_area {
                    if column >= editor_area.x
                        && column < editor_area.x + editor_area.width
                        && row >= editor_area.y
                        && row < editor_area.y + editor_area.height
                    {
                        // 点击在编辑器上
                        self.focus = FocusArea::Editor;
                        self.editor_mouse_click(row, column);
                        return false;
                    }
                }

                // 检查是否点击在目录树区域（左侧）
                if column < (self.tree_width as u16) {
                    // 使用 list_height 来判断有效行范围
                    // 目录树从 y=4 (header 3 + border 1) 开始到 4+list_height
                    if row >= 4 && row < 4 + (self.list_height as u16) {
                        let item_rel_index = (row - 4) as usize;
                        let actual_index = self.scroll_offset + item_rel_index;

                        if actual_index < self.cached_filtered.len() {
                            // 如果点击的是已经选中的项，或是文件，则尝试进入编辑模式
                            if self.selected == actual_index {
                                if let Some(file) = self.get_file_by_index(actual_index) {
                                    if file.is_dir {
                                        self.toggle_collapse();
                                    } else {
                                        let _ = self.open_editor();
                                    }
                                }
                            } else {
                                self.selected = actual_index;
                            }
                            self.focus = FocusArea::Tree;
                            return true;
                        }
                    }
                }
            }
            // 中键按下 - 记录起始位置，开始拖拽模式
            MouseEventKind::Down(MouseButton::Middle) => {
                self.middle_drag_origin = Some(row);
                self.is_middle_dragging = true;
            }
            _ => {}
        }
        false
    }

    /// 处理中键拖拽滚动
    pub fn handle_middle_drag(&mut self, row: u16, column: u16) {
        if !self.is_middle_dragging {
            return;
        }

        if let Some(origin_y) = self.middle_drag_origin {
            let delta = row as i32 - origin_y as i32;

            if delta.abs() >= 1 {
                let lines = delta.unsigned_abs() as usize;

                if column < self.tree_width as u16 {
                    // 拖拽目录树区域
                    if delta < 0 {
                        self.scroll_up(lines);
                    } else {
                        self.scroll_down(lines);
                    }
                } else {
                    // 拖拽编辑器区域
                    if delta < 0 {
                        self.editor_scroll_up(lines);
                    } else {
                        self.editor_scroll_down(lines);
                    }
                }

                // 更新起始位置
                self.middle_drag_origin = Some(row);
            }
        }
    }

    /// 停止中键拖拽
    pub fn stop_middle_drag(&mut self) {
        self.is_middle_dragging = false;
        self.middle_drag_origin = None;
    }

    /// 处理编辑器鼠标点击（设置光标位置）
    fn editor_mouse_click(&mut self, row: u16, column: u16) {
        if let Some(editor_area) = self.editor_area {
            // 检查行
            if row >= editor_area.y + 1 && row < editor_area.y + editor_area.height - 1 {
                let line_offset = (row - (editor_area.y + 1)) as usize;
                let new_line = (self.editor_scroll + line_offset)
                    .min(self.editor_content.len().saturating_sub(1));
                self.editor_cursor.0 = new_line;

                // 计算列：减去边框(1) + gutter(1) + 行号区(4) + 空格(1)
                let text_start_x = editor_area.x + 7;
                if column >= text_start_x {
                    let col_offset = (column - text_start_x) as usize;
                    let line_len = self
                        .editor_content
                        .get(new_line)
                        .map(|s| s.chars().count())
                        .unwrap_or(0);
                    self.editor_cursor.1 = col_offset.min(line_len);
                } else {
                    self.editor_cursor.1 = 0;
                }
            }
        }
    }
}
