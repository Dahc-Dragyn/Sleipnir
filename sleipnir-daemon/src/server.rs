use crate::ui::UiEvent;
use sleipnir_core::models::{ActionStatus, AgentActionFrame, HandshakeResolutionFrame, PayloadType};
use std::borrow::Cow;
use std::path::Path;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc::Sender;
use tracing::{error, info};

#[cfg(unix)]
pub async fn start_uds_server<P: AsRef<Path>>(
    socket_path: P,
    tx: Sender<UiEvent>,
) -> std::io::Result<()> {
    use tokio::net::UnixListener;

    let path = socket_path.as_ref();

    if path.exists() {
        std::fs::remove_file(path)?;
    }

    let listener = UnixListener::bind(path)?;
    info!("Sleipnir daemon bound to UDS at {:?}", path);

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let tx_clone = tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, tx_clone).await {
                        error!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
}

#[cfg(windows)]
pub async fn start_uds_server<P: AsRef<Path>>(
    pipe_name: P,
    tx: Sender<UiEvent>,
) -> std::io::Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;

    // Ensure the path is formatted as a valid named pipe on Windows
    let provided_path = pipe_name.as_ref().to_str().unwrap_or("");
    let path_str = if provided_path.starts_with(r"\\.\pipe\") {
        provided_path.to_string()
    } else {
        format!(r"\\.\pipe\{}", provided_path.replace("\\", "_").replace(":", "_"))
    };

    info!("Sleipnir daemon binding to Named Pipe at {}", path_str);

    let mut first = true;
    loop {
        let server = match ServerOptions::new()
            .first_pipe_instance(first)
            .create(&path_str) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to create pipe instance: {}", e);
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    continue;
                }
            };
        
        first = false;

        // Wait for a client to connect
        match server.connect().await {
            Ok(()) => {
                let tx_clone = tx.clone();
                // Spawn handler and loop around to create the next pipe instance
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(server, tx_clone).await {
                        error!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept pipe connection: {}", e);
            }
        }
    }
}

async fn handle_connection<S>(mut stream: S, tx: Sender<UiEvent>) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut buffer = vec![0; 8192];
    loop {
        let n = match stream.read(&mut buffer).await {
            Ok(0) => break, // Connection closed
            Ok(n) => n,
            Err(e) => return Err(e),
        };

        let data = &buffer[..n];
        match serde_json::from_slice::<AgentActionFrame>(data) {
            Ok(frame) => {
                info!("Successfully parsed AgentActionFrame: transaction_id={}", frame.transaction_id);
                
                let parsed_log = format!("Parsed frame: tx={}, agent={}", frame.transaction_id, frame.agent_id);
                let _ = tx.send(UiEvent::NewLog(parsed_log)).await;

                if let PayloadType::ToolInvocation { ref tool_name, arguments: _ } = frame.payload {
                    // Create oneshot channel for interactive halt
                    let (tx_oneshot, rx_oneshot) = tokio::sync::oneshot::channel();
                    
                    // Signal the UI that we are blocked and waiting for resolution
                    let _ = tx.send(UiEvent::IncomingBlock(frame.transaction_id.to_string(), tx_oneshot)).await;

                    // Await operator decision or default to Denied
                    let (decision, mutated_payload) = rx_oneshot.await.unwrap_or((ActionStatus::Denied, None));

                    let decision_log = match decision {
                        ActionStatus::Approved => format!("Approved transaction: {}", frame.transaction_id),
                        ActionStatus::Denied => format!("Denied transaction: {}", frame.transaction_id),
                        ActionStatus::Mutated => format!("Mutated transaction: {}", frame.transaction_id),
                    };
                    let _ = tx.send(UiEvent::NewLog(decision_log)).await;

                    let final_payload = if decision == ActionStatus::Mutated {
                        if let Some(mutated_args) = mutated_payload {
                            Some(PayloadType::ToolInvocation {
                                tool_name: tool_name.clone(),
                                arguments: Cow::Owned(mutated_args),
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let response = HandshakeResolutionFrame {
                        transaction_id: frame.transaction_id.clone(),
                        status: decision,
                        mutated_payload: final_payload,
                    };

                    let serialized = serde_json::to_vec(&response)?;
                    stream.write_all(&serialized).await?;
                    info!("Sent HandshakeResolutionFrame with status {:?} for transaction_id={}", response.status, frame.transaction_id);
                }
            }
            Err(e) => {
                error!("Failed to parse frame: {}", e);
            }
        }
    }
    Ok(())
}
