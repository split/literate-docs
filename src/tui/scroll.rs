pub struct ScrollState {
    pub offset: usize,
    pub focused_index: usize,
}

impl ScrollState {
    pub fn new() -> Self {
        Self {
            offset: 0,
            focused_index: 0,
        }
    }

    pub fn scroll_down(&mut self, lines: usize, max_offset: usize) {
        self.offset = (self.offset + lines).min(max_offset);
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.offset = self.offset.saturating_sub(lines);
    }

    pub fn focus_next(&mut self, total: usize) {
        if self.focused_index + 1 < total {
            self.focused_index += 1;
        }
    }

    pub fn focus_prev(&mut self) {
        self.focused_index = self.focused_index.saturating_sub(1);
    }

    pub fn reset_focus(&mut self) {
        self.focused_index = 0;
    }
}
