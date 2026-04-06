use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};
use similar::{ChangeTag, TextDiff};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputType {
    Block,
    Comment,
}

impl OutputType {
    pub fn toggle(&mut self) {
        *self = match self {
            OutputType::Block => OutputType::Comment,
            OutputType::Comment => OutputType::Block,
        };
    }

    pub fn label(&self) -> &'static str {
        match self {
            OutputType::Block => "output",
            OutputType::Comment => "comment",
        }
    }
}

#[derive(Debug)]
pub enum OutputState {
    Pending,
    Running {
        live_lines: VecDeque<String>,
        stderr_lines: VecDeque<String>,
        start: Instant,
    },
    Completed {
        output: String,
        previous_output: Option<String>,
        duration: Duration,
        stderr: String,
    },
    Failed {
        error: String,
    },
    Orphaned {
        content: String,
    },
}

impl OutputState {
    pub fn status_label(&self) -> &'static str {
        match self {
            OutputState::Pending => "waiting",
            OutputState::Running { .. } => "running",
            OutputState::Completed { .. } => "done",
            OutputState::Failed { .. } => "error",
            OutputState::Orphaned { .. } => "orphan",
        }
    }

    pub fn is_done(&self) -> bool {
        matches!(self, OutputState::Completed { .. })
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, OutputState::Failed { .. })
    }

    pub fn all_output(&self) -> String {
        match self {
            OutputState::Completed { output, .. } => output.clone(),
            OutputState::Failed { error } => error.clone(),
            OutputState::Running { live_lines, .. } => {
                live_lines.iter().cloned().collect::<Vec<_>>().join("\n")
            }
            OutputState::Pending => String::new(),
            OutputState::Orphaned { content } => content.clone(),
        }
    }

    pub fn all_logs(&self) -> String {
        match self {
            OutputState::Completed { output, stderr, .. } => {
                if stderr.is_empty() {
                    output.clone()
                } else {
                    format!("{}\n\n--- stderr ---\n{}", output, stderr)
                }
            }
            OutputState::Failed { error } => error.clone(),
            OutputState::Running {
                live_lines,
                stderr_lines,
                ..
            } => {
                let mut lines: Vec<String> = live_lines.iter().cloned().collect();
                if !stderr_lines.is_empty() {
                    lines.push(String::new());
                    lines.push("--- stderr ---".to_string());
                    lines.extend(stderr_lines.iter().cloned());
                }
                lines.join("\n")
            }
            OutputState::Pending => String::new(),
            OutputState::Orphaned { content } => content.clone(),
        }
    }
}

pub struct ScrollableBox {
    pub header: String,
    pub content: Vec<Line<'static>>,
    pub is_focused: bool,
    pub skip_lines: usize,
}

impl ScrollableBox {
    pub fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let style = if self.is_focused {
            Style::default().fg(Color::White)
        } else {
            Style::default()
        };

        if self.skip_lines > 0 && area.height == 1 {
            let width = area.width as usize;
            if width >= 2 {
                let line = "└".to_string() + &"─".repeat(width.saturating_sub(2)) + "┘";
                for (j, ch) in line.chars().enumerate() {
                    if let Some(cell) =
                        buf.cell_mut(ratatui::layout::Position::new(area.x + j as u16, area.y))
                    {
                        cell.set_char(ch).set_fg(style.fg.unwrap_or(Color::Reset));
                    }
                }
            }
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.header)
            .style(style);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let start = self.skip_lines.min(self.content.len());
        let visible = self.content.iter().skip(start).take(inner.height as usize);

        for (i, line) in visible.enumerate() {
            let y = inner.y + i as u16;
            if y >= area.bottom() {
                break;
            }
            for (j, span) in line.spans.iter().enumerate() {
                let x = inner.x + j as u16;
                if x < inner.right() {
                    buf.set_span(x, y, span, inner.width);
                }
            }
        }
    }
}

pub struct OutputBox<'a> {
    pub state: &'a OutputState,
    pub is_focused: bool,
    pub output_type: OutputType,
    pub skip_lines: usize,
}

impl OutputBox<'_> {
    fn status_indicator(&self) -> Span<'_> {
        match self.state {
            OutputState::Pending => Span::styled("○ pending", Style::default().fg(Color::DarkGray)),
            OutputState::Running { start, .. } => {
                let elapsed = start.elapsed();
                let secs = elapsed.as_secs();
                let ms = elapsed.subsec_millis();
                if secs > 0 {
                    Span::styled(
                        format!("▶ {:.1}s", secs as f64 + ms as f64 / 1000.0),
                        Style::default().fg(Color::Yellow),
                    )
                } else {
                    Span::styled(format!("▶ {}ms", ms), Style::default().fg(Color::Yellow))
                }
            }
            OutputState::Completed {
                previous_output,
                duration,
                ..
            } => {
                if previous_output.is_some() {
                    Span::styled(
                        format!("~ changed {:.1}s", duration.as_secs_f64()),
                        Style::default().fg(Color::LightMagenta),
                    )
                } else {
                    Span::styled(
                        format!("✓ {:.1}s", duration.as_secs_f64()),
                        Style::default().fg(Color::Green),
                    )
                }
            }
            OutputState::Failed { .. } => Span::styled("✗ error", Style::default().fg(Color::Red)),
            OutputState::Orphaned { .. } => Span::styled(
                "○ orphan",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(ratatui::style::Modifier::CROSSED_OUT),
            ),
        }
    }

    fn render_lines(&self, max_width: usize) -> Vec<Line<'static>> {
        match self.state {
            OutputState::Pending => {
                vec![Line::from(Span::styled(
                    "  (waiting...)",
                    Style::default().fg(Color::DarkGray),
                ))]
            }
            OutputState::Running {
                live_lines,
                stderr_lines,
                ..
            } => {
                let mut lines: Vec<Line> = live_lines
                    .iter()
                    .map(|l| {
                        Line::from(Span::raw(format!(
                            "  {}",
                            truncate(l, max_width.saturating_sub(2))
                        )))
                    })
                    .collect();

                if !stderr_lines.is_empty() {
                    lines.push(Line::from(""));
                    lines.extend(stderr_lines.iter().map(|l| {
                        Line::from(Span::styled(
                            format!("  {}", truncate(l, max_width.saturating_sub(2))),
                            Style::default().fg(Color::Yellow),
                        ))
                    }));
                }

                if lines.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "  ⠋ running...",
                        Style::default().fg(Color::DarkGray),
                    )));
                }

                lines
            }
            OutputState::Completed {
                output,
                previous_output,
                ..
            } => {
                if let (Some(prev), false) =
                    (previous_output, self.output_type == OutputType::Comment)
                {
                    self.render_diff_lines(prev, output, max_width)
                } else {
                    output
                        .lines()
                        .map(|l| {
                            Line::from(format!("  {}", truncate(l, max_width.saturating_sub(2))))
                        })
                        .collect()
                }
            }
            OutputState::Failed { error } => error
                .lines()
                .map(|l| {
                    Line::from(Span::styled(
                        format!("  {}", truncate(l, max_width.saturating_sub(2))),
                        Style::default().fg(Color::Red),
                    ))
                })
                .collect(),
            OutputState::Orphaned { content } => content
                .lines()
                .map(|l| {
                    Line::from(Span::styled(
                        format!("  {}", truncate(l, max_width.saturating_sub(2))),
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(ratatui::style::Modifier::CROSSED_OUT),
                    ))
                })
                .collect(),
        }
    }

    fn render_diff_lines(&self, prev: &str, curr: &str, max_width: usize) -> Vec<Line<'static>> {
        let diff = TextDiff::from_lines(prev, curr);
        let mut lines = Vec::new();

        for change in diff.iter_all_changes() {
            let (prefix, style) = match change.tag() {
                ChangeTag::Delete => ("-", Style::default().fg(Color::Red)),
                ChangeTag::Insert => ("+", Style::default().fg(Color::Green)),
                ChangeTag::Equal => (" ", Style::default().fg(Color::DarkGray)),
            };
            let text = change.value().trim_end_matches('\n');
            if !text.is_empty() || change.tag() != ChangeTag::Equal {
                lines.push(Line::from(Span::styled(
                    format!(
                        "  {} {}",
                        prefix,
                        truncate(text, max_width.saturating_sub(4))
                    ),
                    style,
                )));
            }
        }

        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (no changes)",
                Style::default().fg(Color::DarkGray),
            )));
        }

        lines
    }

    fn content_lines(&self, max_width: usize) -> Vec<Line<'static>> {
        self.render_lines(max_width)
    }
}

impl Widget for OutputBox<'_> {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let status = self.status_indicator();
        let type_label = self.output_type.label();
        let header = format!(" {} │ {} ", status.content, type_label);

        let content = self.content_lines(area.width as usize);

        ScrollableBox {
            header,
            content,
            is_focused: self.is_focused,
            skip_lines: self.skip_lines,
        }
        .render(area, buf);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max.saturating_sub(1);
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}
