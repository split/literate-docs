pub mod output_box;
pub mod render;
pub mod scroll;

#[cfg(test)]
mod tests;

use crossterm::{
    cursor::Show,
    event::{KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use markdown::mdast::Node;
use markdown::{to_mdast, ParseOptions};
use ratatui::style::Stylize;
use ratatui::widgets::Widget;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::HashMap;
use std::io::{self, stdout};
use std::time::Instant;
use tokio::sync::mpsc;

use crate::execute_code_blocks::{is_executable_code_node, spawn_execution_stream, ExecutionEvent};
use crate::with_output_nodes::{is_output_node, with_output_nodes};
use output_box::{OutputBox, OutputState, OutputType};
use render::{build_render_nodes, RenderNode};
use scroll::ScrollState;

struct TerminalGuard;

impl TerminalGuard {
    fn cleanup() {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
        println!();
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        Self::cleanup();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ModalView {
    None,
    Output(usize),
    Logs(usize),
}

pub struct TuiApp {
    ast: Node,
    nodes: Vec<RenderNode>,
    scroll: ScrollState,
    output_types: HashMap<usize, OutputType>,
    running: bool,
    quit: bool,
    aborted: bool,
    modal: ModalView,
    modal_scroll: usize,
}

impl TuiApp {
    pub fn new(input: &str, _previous_content: Option<&str>) -> Self {
        let ast = to_mdast(input, &ParseOptions::default()).expect("Failed to parse markdown");
        let ast = with_output_nodes(&ast);
        let nodes = build_render_nodes(&ast);

        Self {
            ast,
            nodes,
            scroll: ScrollState::new(),
            output_types: HashMap::new(),
            running: false,
            quit: false,
            aborted: false,
            modal: ModalView::None,
            modal_scroll: 0,
        }
    }

    pub async fn run(&mut self) -> Option<Node> {
        if self
            .nodes
            .iter()
            .filter(|n| matches!(n, RenderNode::OutputBlock { .. }))
            .count()
            == 0
        {
            println!("No executable code blocks found.");
            return None;
        }

        let result = self.run_inner().await;

        TerminalGuard::cleanup();

        if let Err(e) = result {
            eprintln!("Error: {}", e);
        }

        if self.aborted {
            None
        } else {
            let empty = Node::Root(markdown::mdast::Root {
                children: vec![],
                position: None,
            });
            Some(std::mem::replace(&mut self.ast, empty))
        }
    }

    async fn run_inner(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen)?;
        let _guard = TerminalGuard;
        let backend = CrosstermBackend::new(out);
        let mut terminal = Terminal::new(backend)?;

        self.running = true;
        let rx = self.spawn_execution();

        let result = self.event_loop(&mut terminal, rx).await;

        result
    }

    fn spawn_execution(&mut self) -> mpsc::Receiver<(usize, ExecutionEvent)> {
        let (tx, rx) = mpsc::channel::<(usize, ExecutionEvent)>(64);

        let exec_codes: Vec<(usize, String, String)> = self
            .nodes
            .iter()
            .filter_map(|n| {
                if let RenderNode::ExecutableCode { index, lang, code } = n {
                    Some((*index, lang.clone(), code.clone()))
                } else {
                    None
                }
            })
            .collect();

        for (idx, lang, code) in &exec_codes {
            if let Some(node) = self.nodes.iter_mut().find(
                |n| matches!(n, RenderNode::OutputBlock { code_index, .. } if *code_index == *idx),
            ) {
                if let RenderNode::OutputBlock { state, .. } = node {
                    *state = OutputState::Running {
                        live_lines: Default::default(),
                        stderr_lines: Default::default(),
                        start: Instant::now(),
                    };
                }
            }
            spawn_execution_stream(lang.clone(), code.clone(), tx.clone(), *idx);
        }

        drop(tx);
        rx
    }

    fn handle_execution_event(&mut self, idx: usize, event: ExecutionEvent) {
        for node in self.nodes.iter_mut() {
            if let RenderNode::OutputBlock { code_index, state } = node {
                if *code_index != idx {
                    continue;
                }
                match event {
                    ExecutionEvent::Started => {}
                    ExecutionEvent::StdoutLine(ref line) => {
                        if let OutputState::Running { live_lines, .. } = state {
                            live_lines.push_back(line.clone());
                        }
                    }
                    ExecutionEvent::StderrLine(ref line) => {
                        if let OutputState::Running { stderr_lines, .. } = state {
                            stderr_lines.push_back(line.clone());
                        }
                    }
                    ExecutionEvent::Completed {
                        ref output,
                        success,
                        duration,
                    } => {
                        let stderr = match &*state {
                            OutputState::Running { stderr_lines, .. } => {
                                stderr_lines.iter().cloned().collect::<Vec<_>>().join("\n")
                            }
                            _ => String::new(),
                        };

                        find_and_update_output_node(&mut self.ast, idx, output);

                        *state = if success {
                            OutputState::Completed {
                                output: output.clone(),
                                previous_output: None,
                                duration,
                                stderr,
                            }
                        } else {
                            OutputState::Failed {
                                error: output.clone(),
                            }
                        };
                    }
                }
            }
        }
    }

    fn output_block_indices(&self) -> Vec<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n, RenderNode::OutputBlock { .. }))
            .map(|(i, _)| i)
            .collect()
    }

    fn current_output_block_index(&self) -> Option<usize> {
        let indices = self.output_block_indices();
        if indices.is_empty() {
            return None;
        }
        let count = indices.len();
        Some(indices[self.scroll.focused_index.min(count - 1)])
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        mut rx: mpsc::Receiver<(usize, ExecutionEvent)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            tokio::select! {
                biased;

                maybe_event = rx.recv(), if self.running => {
                    match maybe_event {
                        Some((idx, event)) => {
                            self.handle_execution_event(idx, event);
                        }
                        None => {
                            self.running = false;
                        }
                    }
                }

                key_event = poll_key() => {
                    if let Some(key) = key_event {
                        if key.kind != KeyEventKind::Press {
                            continue;
                        }

                        if self.modal != ModalView::None {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => {
                                    self.modal = ModalView::None;
                                    self.modal_scroll = 0;
                                }
                                KeyCode::Char('j') | KeyCode::Down => {
                                    self.modal_scroll += 1;
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    self.modal_scroll = self.modal_scroll.saturating_sub(1);
                                }
                                _ => {}
                            }
                            continue;
                        }

                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => {
                                self.quit = true;
                                break;
                            }
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                self.aborted = true;
                                self.quit = true;
                                break;
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                let max_offset = self.max_scroll_offset(terminal.size()?.height as usize);
                                self.scroll.scroll_down(1, max_offset);
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                self.scroll.scroll_up(1);
                            }
                            KeyCode::PageDown => {
                                let max_offset = self.max_scroll_offset(terminal.size()?.height as usize);
                                self.scroll.scroll_down(5, max_offset);
                            }
                            KeyCode::PageUp => {
                                self.scroll.scroll_up(5);
                            }
                            KeyCode::Char('n') => {
                                let output_count = self.output_block_indices().len();
                                if output_count > 0 {
                                    self.scroll.focus_next(output_count);
                                }
                            }
                            KeyCode::Char('p') => {
                                self.scroll.focus_prev();
                            }
                            KeyCode::Char('t') => {
                                if let Some(node_idx) = self.current_output_block_index() {
                                    let output_type = self.output_types.entry(node_idx).or_insert(OutputType::Block);
                                    output_type.toggle();
                                }
                            }
                            KeyCode::Char('m') => {
                                if let Some(node_idx) = self.current_output_block_index() {
                                    if let Some(RenderNode::OutputBlock { code_index, .. }) = self.nodes.get(node_idx) {
                                        self.modal = ModalView::Output(*code_index);
                                        self.modal_scroll = 0;
                                    }
                                }
                            }
                            KeyCode::Char('l') => {
                                if let Some(node_idx) = self.current_output_block_index() {
                                    if let Some(RenderNode::OutputBlock { code_index, .. }) = self.nodes.get(node_idx) {
                                        self.modal = ModalView::Logs(*code_index);
                                        self.modal_scroll = 0;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            if self.quit {
                break;
            }
        }

        Ok(())
    }

    fn render(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        if self.modal != ModalView::None {
            self.render_modal(frame, area);
            return;
        }

        let total_lines = self.total_rendered_lines(area.width as usize);
        let max_offset = total_lines.saturating_sub(area.height as usize);
        let offset = self.scroll.offset.min(max_offset);

        let mut current_line = 0;
        let mut output_box_index = 0;

        for (node_idx, node) in self.nodes.iter().enumerate() {
            let node_lines = self.node_rendered_lines(node, area.width as usize);

            if current_line + node_lines <= offset {
                current_line += node_lines;
                if matches!(node, RenderNode::OutputBlock { .. }) {
                    output_box_index += 1;
                }
                continue;
            }

            let skip = if offset > current_line {
                offset - current_line
            } else {
                0
            };
            let remaining_height = area
                .height
                .saturating_sub((current_line.saturating_sub(offset)) as u16)
                as usize;

            if remaining_height == 0 {
                current_line += node_lines;
                if matches!(node, RenderNode::OutputBlock { .. }) {
                    output_box_index += 1;
                }
                continue;
            }

            match node {
                RenderNode::Text { content, kind } => {
                    let prefix = match kind {
                        render::TextKind::Heading(level) => {
                            format!("{} ", "#".repeat(*level as usize))
                        }
                        _ => String::new(),
                    };
                    let style = match kind {
                        render::TextKind::Heading(_) => ratatui::style::Style::default().bold(),
                        _ => ratatui::style::Style::default(),
                    };

                    let wrapped = wrap_text(&format!("{}{}", prefix, content), area.width as usize);
                    for (i, line) in wrapped.iter().enumerate().skip(skip) {
                        if i - skip >= remaining_height {
                            break;
                        }
                        let y = current_line
                            .saturating_sub(offset)
                            .saturating_add(i)
                            .saturating_sub(skip) as u16;
                        if y < area.height {
                            frame.render_widget(
                                ratatui::widgets::Paragraph::new(line.clone()).style(style),
                                ratatui::layout::Rect::new(0, y, area.width, 1),
                            );
                        }
                    }
                }
                RenderNode::CodeBlock { lang, code } => {
                    let block_text = format!("```{}\n{}\n```", lang, code);
                    let lines: Vec<_> = block_text.lines().collect();
                    for (i, line) in lines.iter().enumerate().skip(skip) {
                        if i - skip >= remaining_height {
                            break;
                        }
                        let y = current_line
                            .saturating_sub(offset)
                            .saturating_add(i)
                            .saturating_sub(skip) as u16;
                        if y < area.height {
                            frame.render_widget(
                                ratatui::widgets::Paragraph::new(line.to_string()).style(
                                    ratatui::style::Style::default()
                                        .fg(ratatui::style::Color::DarkGray),
                                ),
                                ratatui::layout::Rect::new(0, y, area.width, 1),
                            );
                        }
                    }
                }
                RenderNode::ExecutableCode { lang, code, index } => {
                    let is_focused = self.scroll.focused_index == output_box_index;
                    let status = self.get_code_status(*index);

                    let header = format!(" {} │ {} ", lang, status);
                    let code_lines: Vec<_> = code
                        .lines()
                        .map(|l| {
                            ratatui::text::Line::from(ratatui::text::Span::styled(
                                l.to_string(),
                                ratatui::style::Style::default().fg(ratatui::style::Color::Cyan),
                            ))
                        })
                        .collect();
                    let visible_lines = node_lines.saturating_sub(skip);
                    let box_height = visible_lines.min(remaining_height) as u16;
                    if box_height > 0 && current_line < offset + area.height as usize {
                        let y = (current_line.saturating_sub(offset)) as u16;
                        if y < area.height {
                            output_box::ScrollableBox {
                                header,
                                content: code_lines,
                                is_focused,
                                skip_lines: skip,
                            }
                            .render(
                                ratatui::layout::Rect::new(0, y, area.width, box_height),
                                frame.buffer_mut(),
                            );
                        }
                    }
                }
                RenderNode::OutputBlock {
                    code_index: _,
                    state,
                } => {
                    let is_focused = self.scroll.focused_index == output_box_index;
                    let output_type = self
                        .output_types
                        .get(&node_idx)
                        .copied()
                        .unwrap_or(OutputType::Block);

                    let visible_lines = node_lines.saturating_sub(skip);
                    let box_height = visible_lines.min(remaining_height) as u16;
                    if box_height > 0 && current_line < offset + area.height as usize {
                        let y = (current_line.saturating_sub(offset)) as u16;
                        if y < area.height {
                            let box_widget = OutputBox {
                                state,
                                is_focused,
                                output_type,
                                skip_lines: skip,
                            };

                            frame.render_widget(
                                box_widget,
                                ratatui::layout::Rect::new(0, y, area.width, box_height),
                            );
                        }
                    }

                    output_box_index += 1;
                }
            }

            current_line += node_lines;
        }

        let status_bar = if self.running {
            let completed = self
                .nodes
                .iter()
                .filter(|n| {
                    matches!(
                        n,
                        RenderNode::OutputBlock {
                            state: OutputState::Completed { .. } | OutputState::Failed { .. },
                            ..
                        }
                    )
                })
                .count();
            let total = self
                .nodes
                .iter()
                .filter(|n| matches!(n, RenderNode::OutputBlock { .. }))
                .count();
            format!(
                " [{}/{}] running... | ↑/↓:scroll  n/p:focus  t:type  m:modal  l:logs  q:quit",
                completed, total
            )
        } else {
            " done | ↑/↓:scroll  n/p:focus  t:type  m:modal  l:logs  q:quit".to_string()
        };

        frame.render_widget(
            ratatui::widgets::Paragraph::new(status_bar)
                .style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray)),
            ratatui::layout::Rect::new(0, area.height - 1, area.width, 1),
        );
    }

    fn get_code_status(&self, index: usize) -> String {
        if let Some(RenderNode::OutputBlock { state, .. }) = self.nodes.iter().find(
            |n| matches!(n, RenderNode::OutputBlock { code_index, .. } if *code_index == index),
        ) {
            match state {
                OutputState::Running { start, .. } => {
                    let elapsed = start.elapsed();
                    let secs = elapsed.as_secs();
                    let ms = elapsed.subsec_millis();
                    if secs > 0 {
                        format!("▶ {:.1}s", secs as f64 + ms as f64 / 1000.0)
                    } else {
                        format!("▶ {}ms", ms)
                    }
                }
                OutputState::Completed { .. } => "✓ done".to_string(),
                OutputState::Failed { .. } => "✗ error".to_string(),
                OutputState::Pending => "○ pending".to_string(),
            }
        } else {
            "○ pending".to_string()
        }
    }

    fn render_modal(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let (title, sections) = match self.modal {
            ModalView::Output(code_idx) => {
                let state = self.nodes.iter().find_map(|n| {
                    if let RenderNode::OutputBlock { code_index, state } = n {
                        if *code_index == code_idx {
                            Some(state)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });
                if let Some(state) = state {
                    let text = state.all_output();
                    if text.is_empty() {
                        (
                            "Output",
                            vec![("output", vec!["(no output yet)".to_string()])],
                        )
                    } else {
                        (
                            "Output",
                            vec![("output", text.lines().map(|l| l.to_string()).collect())],
                        )
                    }
                } else {
                    ("Output", vec![("output", vec!["(not found)".to_string()])])
                }
            }
            ModalView::Logs(code_idx) => {
                let state = self.nodes.iter().find_map(|n| {
                    if let RenderNode::OutputBlock { code_index, state } = n {
                        if *code_index == code_idx {
                            Some(state)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });
                if let Some(state) = state {
                    match state {
                        OutputState::Running {
                            live_lines,
                            stderr_lines,
                            start,
                        } => {
                            let elapsed = start.elapsed();
                            let mut sections = vec![(
                                "logs",
                                vec![
                                    format!("running for {:.1}s", elapsed.as_secs_f64()),
                                    String::new(),
                                ],
                            )];
                            sections[0].1.extend(live_lines.iter().cloned());
                            if !stderr_lines.is_empty() {
                                sections.push((
                                    "stderr",
                                    vec![String::new(), "--- stderr ---".to_string()],
                                ));
                                sections[1].1.extend(stderr_lines.iter().cloned());
                            }
                            ("Build Logs", sections)
                        }
                        OutputState::Completed {
                            output,
                            duration,
                            stderr,
                            ..
                        } => {
                            let mut sections = vec![(
                                "output",
                                vec![
                                    format!("completed in {:.1}s", duration.as_secs_f64()),
                                    String::new(),
                                ],
                            )];
                            sections[0].1.extend(output.lines().map(|l| l.to_string()));
                            if !stderr.is_empty() {
                                sections.push((
                                    "stderr",
                                    vec![String::new(), "--- stderr ---".to_string()],
                                ));
                                sections[1].1.extend(stderr.lines().map(|l| l.to_string()));
                            }
                            ("Output", sections)
                        }
                        OutputState::Failed { error } => {
                            let sections =
                                vec![("error", vec!["failed".to_string(), String::new()])];
                            let mut sections = sections;
                            sections[0].1.extend(error.lines().map(|l| l.to_string()));
                            ("Error", sections)
                        }
                        OutputState::Pending => (
                            "Build Logs",
                            vec![("logs", vec!["(waiting...)".to_string()])],
                        ),
                    }
                } else {
                    (
                        "Build Logs",
                        vec![("logs", vec!["(not found)".to_string()])],
                    )
                }
            }
            ModalView::None => unreachable!(),
        };

        let inner_width = area.width.saturating_sub(4);
        let inner_height = area.height.saturating_sub(4);

        if inner_width < 10 || inner_height < 5 {
            return;
        }

        let modal_x = 2;
        let modal_y = 2;

        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .title(format!(" {} (↑/↓ scroll, q/Esc close) ", title))
            .style(ratatui::style::Style::default().fg(ratatui::style::Color::White));

        let modal_area = ratatui::layout::Rect::new(modal_x, modal_y, inner_width, inner_height);
        let inner = block.inner(modal_area);
        block.render(modal_area, frame.buffer_mut());

        let scroll_offset = self.modal_scroll;

        let mut all_lines: Vec<(String, String)> = Vec::new();
        for (section_name, lines) in &sections {
            all_lines.push((
                section_name.to_string(),
                format!("─── {} ───", section_name),
            ));
            for line in lines {
                all_lines.push((section_name.to_string(), line.clone()));
            }
        }

        let total_lines = all_lines.len();
        let max_scroll = total_lines.saturating_sub(inner.height as usize);
        let scroll = scroll_offset.min(max_scroll);

        for (i, (_, line)) in all_lines.iter().enumerate().skip(scroll) {
            let row = i - scroll;
            if row >= inner.height as usize {
                break;
            }
            let y = inner.y + row as u16;
            let truncated = if line.len() > inner.width as usize {
                &line[..inner.width as usize]
            } else {
                line.as_str()
            };
            for (j, ch) in truncated.chars().enumerate() {
                let x = inner.x + j as u16;
                if x < inner.right() {
                    if let Some(cell) = frame
                        .buffer_mut()
                        .cell_mut(ratatui::layout::Position::new(x, y))
                    {
                        cell.set_char(ch);
                    }
                }
            }
        }
    }

    fn max_scroll_offset(&self, viewport_height: usize) -> usize {
        let total = self.total_rendered_lines(80);
        total.saturating_sub(viewport_height)
    }

    fn total_rendered_lines(&self, terminal_width: usize) -> usize {
        self.nodes
            .iter()
            .map(|n| self.node_rendered_lines(n, terminal_width))
            .sum::<usize>()
            .max(1)
    }

    fn node_rendered_lines(&self, node: &RenderNode, terminal_width: usize) -> usize {
        match node {
            RenderNode::Text { content, kind } => {
                let prefix = match kind {
                    render::TextKind::Heading(level) => "#".repeat(*level as usize) + " ",
                    _ => String::new(),
                };
                let wrapped = wrap_text(&format!("{}{}", prefix, content), terminal_width);
                wrapped.len().max(1) + 1
            }
            RenderNode::CodeBlock { code, .. } => code.lines().count() + 2,
            RenderNode::ExecutableCode { code, .. } => code.lines().count() + 2,
            RenderNode::OutputBlock { state, .. } => match state {
                OutputState::Pending => 3,
                OutputState::Running {
                    live_lines,
                    stderr_lines,
                    ..
                } => {
                    let output_lines = live_lines.len()
                        + if stderr_lines.is_empty() {
                            0
                        } else {
                            stderr_lines.len() + 1
                        };
                    (output_lines + 2).max(3)
                }
                OutputState::Completed { output, .. } => (output.lines().count() + 2).max(3),
                OutputState::Failed { error } => (error.lines().count() + 2).max(3),
            },
        }
    }
}

fn find_and_update_output_node(ast: &mut Node, target_code_index: usize, output: &str) {
    fn walk(node: &mut Node, target: usize, output: &str, code_index: &mut usize) -> bool {
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut() {
                if is_output_node(child) {
                    if code_index.saturating_sub(1) == target {
                        if let Node::Code(code) = child {
                            code.value = output.to_string();
                            return true;
                        }
                    }
                }

                if is_executable_code_node(child) {
                    *code_index += 1;
                }

                if walk(child, target, output, code_index) {
                    return true;
                }
            }
        }
        false
    }

    let mut code_index = 0;
    walk(ast, target_code_index, output, &mut code_index);
}

async fn poll_key() -> Option<crossterm::event::KeyEvent> {
    if crossterm::event::poll(std::time::Duration::from_millis(100)).ok()? {
        if let crossterm::event::Event::Key(key) = crossterm::event::read().ok()? {
            if key.kind == KeyEventKind::Press {
                return Some(key);
            }
        }
    }
    None
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut start = 0;
        while start < line.len() {
            let end = (start + width).min(line.len());
            if end >= line.len() {
                lines.push(line[start..].to_string());
                break;
            }
            let chunk_end = line[start..end]
                .char_indices()
                .last()
                .map(|(i, c)| start + i + c.len_utf8())
                .unwrap_or(end);
            lines.push(line[start..chunk_end].to_string());
            start = chunk_end;
        }
    }
    lines
}
