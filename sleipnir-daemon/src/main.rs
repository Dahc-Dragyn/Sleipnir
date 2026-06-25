mod server;
mod ui;
#[cfg(test)]
mod server_test;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use sleipnir_core::models::ActionStatus;
use std::time::Duration;
use tracing::info;
use tracing_subscriber::FmtSubscriber;
use std::env;
use tui_textarea::TextArea;
use ui::TerminalGuard;

fn export_to_chandrian(tx_id: &str) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("chandrian_fuzz_pool.jsonl")?;

    writeln!(
        file,
        "{{\"timestamp\": {}, \"transaction_id\": \"{}\", \"flag\": \"ADVERSARIAL_FUZZ_TARGET\"}}",
        epoch, tx_id
    )?;

    Ok(())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let log_file = std::fs::File::create("sleipnir.log").expect("failed to create log file");
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::sync::Mutex::new(log_file))
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    // Establish non-blocking channel link
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ui::UiEvent>(32);

    // Initialize UI using TerminalGuard to ensure cleanup on drop
    let mut ui_guard = TerminalGuard::setup()?;
    let mut state = ui::AppState::new();

    info!("Sleipnir Daemon initializing...");
    
    // Use temp_dir for Windows compatibility
    let socket_path = env::temp_dir().join("sleipnir.sock");
    
    let tx_for_server = tx.clone();
    // Background the socket server
    tokio::spawn(async move {
        if let Err(e) = server::start_uds_server(socket_path, tx_for_server).await {
            tracing::error!("Server error: {}", e);
        }
    });
    
    // The Event Pump
    loop {
        // Draw terminal layout based on the current state tracking variable
        ui_guard.terminal.draw(|f| {
            ui::draw(f, &mut state);
        })?;

        // Non-blocking sweep on the receiver channel to pull down telemetry transitions
        while let Ok(event) = rx.try_recv() {
            match event {
                ui::UiEvent::IncomingBlock(tx_id, tx_oneshot) => {
                    if let ui::InteractionMode::Streaming = state.mode {
                        state.mode = ui::InteractionMode::Blocked(tx_id, tx_oneshot);
                    } else {
                        state.pending_blocks.push_back((tx_id, tx_oneshot));
                    }
                }
                ui::UiEvent::NewLog(log_msg) => {
                    state.push_log(log_msg);
                }
            }
        }

        let mut advance_queue = || {
            if let Some((id, tx_chan)) = state.pending_blocks.pop_front() {
                ui::InteractionMode::Blocked(id, tx_chan)
            } else {
                ui::InteractionMode::Streaming
            }
        };

        // Poll keyboard events
        if event::poll(Duration::from_secs(0))? {
            let raw_event = event::read()?;
            
            // Global exit hook for Ctrl+C
            if let Event::Key(key) = &raw_event {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }
            }

            let mut temp_mode = ui::InteractionMode::Streaming;
            std::mem::swap(&mut state.mode, &mut temp_mode);

            match temp_mode {
                ui::InteractionMode::Blocked(tx_id, tx_oneshot) => {
                    if let Event::Key(key) = &raw_event {
                        match key.code {
                            KeyCode::Char('y') => {
                                let _ = tx_oneshot.send((ActionStatus::Approved, None));
                                state.mode = advance_queue();
                            }
                            KeyCode::Char('n') => {
                                let _ = tx_oneshot.send((ActionStatus::Denied, None));
                                state.mode = advance_queue();
                            }
                            KeyCode::Char('m') => {
                                let mut textarea = TextArea::default();
                                textarea.insert_str("{\"query\": \"EDIT ME\"}");
                                state.mode = ui::InteractionMode::Mutating(tx_id, tx_oneshot, textarea);
                            }
                            KeyCode::F(8) => {
                                match export_to_chandrian(&tx_id) {
                                    Ok(()) => {
                                        let _ = tx.try_send(ui::UiEvent::NewLog(format!("EXPORTED TO CHANDRIAN: {}", tx_id)));
                                    }
                                    Err(e) => {
                                        let _ = tx.try_send(ui::UiEvent::NewLog(format!("EXPORTED TO CHANDRIAN FAILURE: {}", e)));
                                    }
                                }
                                let _ = tx_oneshot.send((ActionStatus::Denied, None));
                                state.mode = advance_queue();
                            }
                            _ => {
                                // Restore state if irrelevent key pressed
                                state.mode = ui::InteractionMode::Blocked(tx_id, tx_oneshot);
                            }
                        }
                    } else {
                        state.mode = ui::InteractionMode::Blocked(tx_id, tx_oneshot);
                    }
                }
                ui::InteractionMode::Mutating(tx_id, tx_oneshot, mut textarea) => {
                    if let Event::Key(key) = &raw_event {
                        if key.code == KeyCode::F(9) {
                            let text = textarea.lines().join("\n");
                            let _ = tx_oneshot.send((ActionStatus::Mutated, Some(text)));
                            state.mode = advance_queue();
                            continue;
                        } else if key.code == KeyCode::Esc {
                            let _ = tx_oneshot.send((ActionStatus::Denied, None));
                            state.mode = advance_queue();
                            continue;
                        }
                    }
                    
                    // Route all other crossterm events to textarea
                    textarea.input(raw_event.clone());
                    state.mode = ui::InteractionMode::Mutating(tx_id, tx_oneshot, textarea);
                }
                ui::InteractionMode::Streaming => {
                    if let Event::Key(key) = &raw_event {
                        if key.code == KeyCode::Char('q') {
                            break;
                        }
                    }
                    state.mode = ui::InteractionMode::Streaming;
                }
            }
        }

        // Clock out at ~60Hz to prevent CPU throttling
        tokio::time::sleep(std::time::Duration::from_millis(16)).await;
    }
    
    Ok(())
}
