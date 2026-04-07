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
use crate::render_markdown::parse_markdown;
use markdown::mdast::Node;
use ratatui::style::Stylize;
use ratatui::text::Span;
use ratatui::widgets::Widget;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::HashMap;
use std::io::{self, stdout};
use std::time::Instant;
use tokio::sync::mpsc;

use crate::execute_code_blocks::{is_executable_code_node, spawn_execution_stream, ExecutionEvent};
use crate::output_node::{is_output_node, update_output_value};
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
        use crate::with_output_nodes::with_output_nodes;

        let ast = parse_markdown(input);

        let info = with_output_nodes(&ast);
        let orphans = info.orphans;
        let ast = info.ast;

        let nodes = build_render_nodes(&ast, &orphans);

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
            let ast = std::mem::replace(&mut self.ast, empty);
            Some(crate::output_node::clean_orphans(ast))
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
                if let RenderNode::ExecutableCode {
                    index, lang, code, ..
                } = n
                {
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
        let mut update_info = None;

        for node in self.nodes.iter_mut() {
            if let RenderNode::OutputBlock {
                code_index,
                state,
                is_orphan,
                ..
            } = node
            {
                if *code_index != idx || *is_orphan {
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

                        let new_state = if success {
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

                        update_info = Some((*code_index, output.clone(), new_state));
                    }
                }
            }
        }

        if let Some((code_index, output, new_state)) = update_info {
            self.update_ast_output(code_index, &output);
            for node in self.nodes.iter_mut() {
                if let RenderNode::OutputBlock {
                    code_index: ci,
                    state,
                    ..
                } = node
                {
                    if *ci == code_index {
                        *state = new_state;
                        break;
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
                            vec![ratatui::text::Span::styled(
                                format!("{} ", "#".repeat(*level as usize)),
                                ratatui::style::Style::default().bold(),
                            )]
                        }
                        _ => vec![],
                    };
                    let style = match kind {
                        render::TextKind::Heading(_) => ratatui::style::Style::default().bold(),
                        _ => ratatui::style::Style::default(),
                    };

                    let mut spans = prefix;
                    spans.extend(content.clone());

                    let mut lines: Vec<ratatui::text::Line> = split_spans_on_newlines(&spans);

                    // Add blank line before headings that aren't the first node
                    if matches!(kind, render::TextKind::Heading(_)) && node_idx > 0 {
                        lines.insert(0, ratatui::text::Line::from(""));
                    }

                    let paragraph =
                        ratatui::widgets::Paragraph::new(ratatui::text::Text::from(lines))
                            .style(style)
                            .wrap(ratatui::widgets::Wrap { trim: true });

                    let node_height = self.node_rendered_lines(node, area.width as usize);
                    let visible_lines = node_height.saturating_sub(skip);
                    let render_height = visible_lines.min(remaining_height) as u16;
                    if render_height > 0 && current_line < offset + area.height as usize {
                        let y = (current_line.saturating_sub(offset)) as u16;
                        if y < area.height {
                            frame.render_widget(
                                paragraph,
                                ratatui::layout::Rect::new(0, y, area.width, render_height),
                            );
                        }
                    }
                }
                RenderNode::Table { rows } => {
                    use ratatui::widgets::{Cell, Row, Table};

                    let header_cells: Vec<Cell> = rows[0]
                        .iter()
                        .map(|cell| {
                            let text: String = cell.iter().map(|s| s.content.as_ref()).collect();
                            Cell::from(text)
                        })
                        .collect();
                    let header = Row::new(header_cells)
                        .style(ratatui::style::Style::default().bold())
                        .bottom_margin(1);

                    let data_rows: Vec<Row> = rows[1..]
                        .iter()
                        .map(|row| {
                            let cells: Vec<Cell> = row
                                .iter()
                                .map(|cell| {
                                    let text: String =
                                        cell.iter().map(|s| s.content.as_ref()).collect();
                                    Cell::from(text)
                                })
                                .collect();
                            Row::new(cells)
                        })
                        .collect();

                    let constraints: Vec<ratatui::layout::Constraint> =
                        vec![ratatui::layout::Constraint::Fill(1); rows[0].len()];

                    let table = Table::new(data_rows, constraints)
                        .header(header)
                        .block(
                            ratatui::widgets::Block::default()
                                .borders(ratatui::widgets::Borders::ALL),
                        );

                    let node_height = self.node_rendered_lines(node, area.width as usize);
                    let visible_lines = node_height.saturating_sub(skip);
                    let render_height = visible_lines.min(remaining_height) as u16;
                    if render_height > 0 && current_line < offset + area.height as usize {
                        let y = (current_line.saturating_sub(offset)) as u16;
                        if y < area.height {
                            frame.render_widget(
                                table,
                                ratatui::layout::Rect::new(0, y, area.width, render_height),
                            );
                        }
                    }
                }
                RenderNode::CodeBlock {
                    lang,
                    code,
                    executable,
                } => {
                    let is_focused = self.scroll.focused_index == output_box_index;
                    let status = if *executable {
                        self.get_code_status(0)
                    } else {
                        "no exec".to_string()
                    };

                    let header = format!(" {} │ {} ", lang, status);
                    let code_lines: Vec<_> = code
                        .lines()
                        .map(|l| {
                            ratatui::text::Line::from(ratatui::text::Span::styled(
                                l.to_string(),
                                ratatui::style::Style::default()
                                    .fg(ratatui::style::Color::DarkGray),
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
                RenderNode::ExecutableCode {
                    lang,
                    code,
                    index,
                    hidden,
                } => {
                    let is_focused = self.scroll.focused_index == output_box_index;
                    let status = self.get_code_status(*index);

                    let hidden_label = if *hidden { " (hidden)" } else { "" };
                    let header = format!(" {}{} │ {} ", lang, hidden_label, status);
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
                    ..
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
                OutputState::Orphaned { .. } => "○ orphan".to_string(),
            }
        } else {
            "○ pending".to_string()
        }
    }

    fn render_modal(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let (title, sections) = match self.modal {
            ModalView::Output(code_idx) => {
                let state = self.nodes.iter().find_map(|n| {
                    if let RenderNode::OutputBlock {
                        code_index, state, ..
                    } = n
                    {
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
                    if let RenderNode::OutputBlock {
                        code_index, state, ..
                    } = n
                    {
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
                        OutputState::Orphaned { content } => (
                            "Orphaned Output",
                            vec![
                                (
                                    "note",
                                    vec!["(will be removed on quit)".to_string(), String::new()],
                                ),
                                ("content", content.lines().map(|l| l.to_string()).collect()),
                            ],
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
                let prefix_len = match kind {
                    render::TextKind::Heading(level) => *level as usize + 1,
                    _ => 0,
                };
                let total_len: usize =
                    content.iter().map(|s| s.content.len()).sum::<usize>() + prefix_len;
                let newline_count: usize = content
                    .iter()
                    .map(|s| s.content.matches('\n').count())
                    .sum();
                let wrapped = total_len / terminal_width.max(1);
                let base = (wrapped.max(1) + 1) + newline_count;
                // Headings get an extra blank line above them
                if matches!(kind, render::TextKind::Heading(_)) {
                    base + 1
                } else {
                    base
                }
            }
            RenderNode::Table { rows } => rows.len() + 2,
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
                OutputState::Orphaned { content } => (content.lines().count() + 2).max(3),
            },
        }
    }
    fn update_ast_output(&mut self, target_code_index: usize, output: &str) {
        fn walk(node: &mut Node, target: usize, output: &str, idx: &mut usize) -> bool {
            if let Some(children) = node.children_mut() {
                for child in children.iter_mut() {
                    if is_output_node(child) {
                        if idx.saturating_sub(1) == target {
                            return update_output_value(child, output);
                        }
                    }

                    if is_executable_code_node(child) {
                        *idx += 1;
                    }

                    if walk(child, target, output, idx) {
                        return true;
                    }
                }
            }
            false
        }

        let mut idx = 0;
        walk(&mut self.ast, target_code_index, output, &mut idx);
    }
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

fn split_spans_on_newlines(spans: &[Span<'static>]) -> Vec<ratatui::text::Line<'static>> {
    let mut lines: Vec<Vec<Span<'static>>> = vec![vec![]];
    for span in spans {
        let parts: Vec<&str> = span.content.split('\n').collect();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                lines.push(vec![]);
            }
            if !part.is_empty() || (i == 0 && parts.len() == 1) {
                lines
                    .last_mut()
                    .unwrap()
                    .push(Span::styled(part.to_string(), span.style));
            }
        }
    }
    lines
        .into_iter()
        .map(|spans| {
            if spans.is_empty() {
                ratatui::text::Line::from("")
            } else {
                ratatui::text::Line::from(spans)
            }
        })
        .collect()
}
