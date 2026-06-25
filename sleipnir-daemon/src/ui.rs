use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use sleipnir_core::models::ActionStatus;
use std::collections::VecDeque;
use std::io::{self, Stdout};
use tui_textarea::TextArea;

#[derive(Debug)]
pub enum InteractionMode<'a> {
    Streaming,
    Blocked(String, tokio::sync::oneshot::Sender<(ActionStatus, Option<String>)>),
    Mutating(String, tokio::sync::oneshot::Sender<(ActionStatus, Option<String>)>, TextArea<'a>),
}

#[derive(Debug)]
pub enum UiEvent {
    IncomingBlock(String, tokio::sync::oneshot::Sender<(ActionStatus, Option<String>)>),
    NewLog(String),
}

pub struct AppState<'a> {
    pub mode: InteractionMode<'a>,
    pub logs: VecDeque<String>,
    pub pending_blocks: VecDeque<(String, tokio::sync::oneshot::Sender<(ActionStatus, Option<String>)>)>,
}

impl<'a> AppState<'a> {
    pub fn new() -> Self {
        Self {
            mode: InteractionMode::Streaming,
            logs: VecDeque::with_capacity(50),
            pending_blocks: VecDeque::new(),
        }
    }

    pub fn push_log(&mut self, log: String) {
        if self.logs.len() >= 50 {
            self.logs.pop_front();
        }
        self.logs.push_back(log);
    }
}

pub struct TerminalGuard {
    pub terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    pub fn setup() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

pub fn draw(frame: &mut Frame, state: &mut AppState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(5)])
        .split(area);

    let top_chunk = chunks[0];
    let bottom_chunk = chunks[1];

    let pending_suffix = if state.pending_blocks.is_empty() {
        "".to_string()
    } else {
        format!(" ({} Pending)", state.pending_blocks.len())
    };

    let title = match &state.mode {
        InteractionMode::Streaming => " SLEIPNIR: STREAMING ".to_string(),
        InteractionMode::Blocked(_, _) => format!(" SLEIPNIR: BLOCKED{} ", pending_suffix),
        InteractionMode::Mutating(..) => format!(" SLEIPNIR: MUTATING{} (F9 to Submit, ESC to Cancel) ", pending_suffix),
    };

    let color = match &state.mode {
        InteractionMode::Streaming => ratatui::style::Color::Green,
        InteractionMode::Blocked(_, _) => ratatui::style::Color::Red,
        InteractionMode::Mutating(..) => ratatui::style::Color::Yellow,
    };

    let block = Block::default()
        .title(title.as_str())
        .borders(Borders::ALL)
        .border_style(ratatui::style::Style::default().fg(color));

    match &mut state.mode {
        InteractionMode::Streaming => {
            let p = Paragraph::new(vec![Line::from("Agent telemetry flowing. System monitoring active.")])
                .block(block)
                .alignment(Alignment::Center);
            frame.render_widget(p, top_chunk);
        }
        InteractionMode::Blocked(ref tx_id, _) => {
            let p = Paragraph::new(vec![
                Line::from(vec![
                    Span::raw("Agent Blocked. Awaiting operator resolution for Transaction ID: "),
                    Span::raw(tx_id.as_str()),
                ]),
                Line::from("Press 'y' to Approve, 'n' to Deny, 'm' to Mutate"),
            ])
            .block(block)
            .alignment(Alignment::Center);
            frame.render_widget(p, top_chunk);
        }
        InteractionMode::Mutating(_, _, ref mut textarea) => {
            textarea.set_block(Block::default());
            let inner_area = block.inner(top_chunk);
            frame.render_widget(block, top_chunk);
            frame.render_widget(&*textarea, inner_area);
        }
    }

    // Render telemetry log in bottom chunk
    let log_items: Vec<ListItem> = state.logs
        .iter()
        .map(|log| ListItem::new(Line::from(log.as_str())))
        .collect();

    let logs_block = Block::default()
        .title(" TELEMETRY LOG ")
        .borders(Borders::ALL)
        .border_style(ratatui::style::Style::default().fg(ratatui::style::Color::Cyan));

    let list = List::new(log_items).block(logs_block);
    frame.render_widget(list, bottom_chunk);
}
