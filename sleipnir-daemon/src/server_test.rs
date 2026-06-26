use crate::server::start_uds_server;
use crate::ui::UiEvent;
use sleipnir_core::models::{ActionStatus, AgentActionFrame, HandshakeResolutionFrame, PayloadType};
use std::borrow::Cow;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[cfg(unix)]
async fn connect_client(path: &std::path::Path) -> impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {
    tokio::net::UnixStream::connect(path).await.expect("Failed to connect")
}

#[cfg(windows)]
async fn connect_client(addr: &str) -> impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {
    loop {
        match tokio::net::TcpStream::connect(addr).await {
            Ok(client) => return client,
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            Err(e) => panic!("Failed to connect to TCP socket: {}", e),
        }
    }
}

#[tokio::test]
async fn test_uds_handshake() {
    #[cfg(unix)]
    let socket_path = std::env::temp_dir().join("sleipnir_test.sock");
    
    #[cfg(windows)]
    let socket_path = "127.0.0.1:47777".to_string();

    let path_clone = socket_path.clone();

    // Spawn server in background
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let policy = std::sync::Arc::new(tokio::sync::RwLock::new(crate::policy::PolicyEngine::default()));
    tokio::spawn(async move {
        let _ = start_uds_server(path_clone, tx, policy).await;
    });

    // Mock Operator loop
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let UiEvent::IncomingBlock(_, tx_oneshot) = event {
                let _ = tx_oneshot.send((ActionStatus::Approved, None));
            }
        }
    });

    // Give server a tiny bit of time to bind
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut stream = connect_client(&socket_path).await;

    let action = AgentActionFrame {
        transaction_id: Cow::Borrowed("tx_123"),
        agent_id: Cow::Borrowed("test_agent"),
        timestamp: 123456,
        payload: PayloadType::ToolInvocation {
            tool_name: Cow::Borrowed("execute_bash"),
            arguments: Cow::Borrowed("{\"cmd\":\"ls\"}"),
        },
        context_monologue: Some(Cow::Borrowed("Testing tool invocation")),
    };

    let serialized_action = serde_json::to_vec(&action).unwrap();
    
    let start_time = Instant::now();
    stream.write_all(&serialized_action).await.unwrap();

    let mut buf = vec![0; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let duration = start_time.elapsed();

    assert!(n > 0);
    let resolution: HandshakeResolutionFrame = serde_json::from_slice(&buf[..n]).unwrap();

    assert_eq!(resolution.transaction_id, "tx_123");
    assert_eq!(resolution.status, ActionStatus::Approved);
    assert!(resolution.mutated_payload.is_none());

    // Verify latency loop
    println!("Handshake roundtrip took: {:?}", duration);
}

#[tokio::test]
async fn test_swarm_concurrent_blocks() {
    use crate::server::handle_connection;
    use crate::policy::{PolicyEngine, CompiledToolPolicy};
    use sleipnir_core::models::RiskTier;

    let (tx_ui, mut rx_ui) = tokio::sync::mpsc::channel(32);
    let mut engine = PolicyEngine::default();
    engine.tools.insert("execute_sql".into(), CompiledToolPolicy {
        risk_tier: RiskTier::Verify,
        patterns: vec![],
    });
    let policy_arc = std::sync::Arc::new(tokio::sync::RwLock::new(engine));
    // Simulate 5 swarm concurrent connections
    for i in 0..5 {
        let (mut client_io, server_io) = tokio::io::duplex(64 * 1024);
        let tx_clone = tx_ui.clone();
        let policy_clone = policy_arc.clone();
        
        tokio::spawn(async move {
            let _ = handle_connection(server_io, tx_clone, policy_clone).await;
        });
        
        tokio::spawn(async move {
            let frame = AgentActionFrame {
                transaction_id: Cow::Owned(format!("tx_{}", i)),
                agent_id: Cow::Borrowed("agent_alpha"),
                timestamp: 1000,
                payload: PayloadType::ToolInvocation {
                    tool_name: Cow::Borrowed("execute_sql"),
                    arguments: Cow::Borrowed("{\"query\": \"SELECT * FROM test\"}"),
                },
                context_monologue: None,
            };
            
            let serialized = serde_json::to_vec(&frame).unwrap();
            client_io.write_all(&serialized).await.unwrap();
        });
    }
    
    let mut block_count = 0;
    
    for _ in 0..15 {
        if let Ok(event) = tokio::time::timeout(std::time::Duration::from_millis(50), rx_ui.recv()).await {
            if let Some(e) = event {
                if let UiEvent::IncomingBlock(tx_id, tx_oneshot) = e {
                    assert!(tx_id.starts_with("tx_"));
                    block_count += 1;
                    let _ = tx_oneshot.send((sleipnir_core::models::ActionStatus::Approved, None));
                }
            }
        }
    }
    
    assert_eq!(block_count, 5, "Server dropped frames during concurrency spike!");
}
