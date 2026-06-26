use crate::ui::UiEvent;
use crate::policy::{PolicyEngine, PolicyDecision};
use std::sync::Arc;
use tokio::sync::RwLock;
use sleipnir_core::models::{ActionStatus, AgentActionFrame, HandshakeResolutionFrame, PayloadType};
use std::borrow::Cow;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc::Sender;
use tracing::{error, info};

#[cfg(unix)]
pub async fn start_uds_server<P: AsRef<std::path::Path>>(
    socket_path: P,
    tx: Sender<UiEvent>,
    policy: Arc<RwLock<PolicyEngine>>,
) -> std::io::Result<()> {
    use tokio::net::UnixListener;

    let path = socket_path.as_ref();

    if path.exists() {
        std::fs::remove_file(path)?;
    }

    let listener = UnixListener::bind(path)?;
    
    // Set 0600 permissions
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;

    info!("Sleipnir daemon bound to UDS at {:?}", path);

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let tx_clone = tx.clone();
                let policy_clone = policy.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, tx_clone, policy_clone).await {
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
pub async fn start_uds_server<P: AsRef<str>>(
    address: P,
    tx: Sender<UiEvent>,
    policy: Arc<RwLock<PolicyEngine>>,
) -> std::io::Result<()> {
    use tokio::net::TcpListener;
    
    let addr_str = address.as_ref();
    info!("Sleipnir daemon binding to TCP at {}", addr_str);

    let listener = TcpListener::bind(addr_str).await?;

    loop {
        let (stream, addr) = match listener.accept().await {
            Ok(res) => res,
            Err(e) => {
                error!("Failed to accept TCP connection: {}", e);
                continue;
            }
        };

        let tx_clone = tx.clone();
        let policy_clone = policy.clone();
        
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, tx_clone, policy_clone).await {
                error!("Connection error from {}: {}", addr, e);
            }
        });
    }
}

pub(crate) async fn handle_connection<S>(mut stream: S, tx: Sender<UiEvent>, policy: Arc<RwLock<PolicyEngine>>) -> std::io::Result<()>
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

                if let PayloadType::ToolInvocation { ref tool_name, ref arguments } = frame.payload {
                    let decision = {
                        let lock = policy.read().await;
                        lock.evaluate(tool_name, arguments)
                    };

                    match decision {
                        PolicyDecision::Pass => {
                            let _ = tx.send(UiEvent::NewLog(format!("Auto-Approved transaction: {}", frame.transaction_id))).await;
                            let response = HandshakeResolutionFrame {
                                transaction_id: frame.transaction_id.clone(),
                                status: ActionStatus::Approved,
                                mutated_payload: None,
                            };
                            let serialized = serde_json::to_vec(&response)?;
                            stream.write_all(&serialized).await?;
                            info!("Auto-Approved tool invocation: {}", tool_name);
                        }
                        PolicyDecision::Escalate => {
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
                }
            }
            Err(e) => {
                error!("Failed to parse frame: {}", e);
            }
        }
    }
    Ok(())
}
