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
async fn connect_client(path: &std::path::Path) -> impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {
    let provided_path = path.to_str().unwrap();
    let path_str = if provided_path.starts_with(r"\\.\pipe\") {
        provided_path.to_string()
    } else {
        format!(r"\\.\pipe\{}", provided_path.replace("\\", "_").replace(":", "_"))
    };

    loop {
        match tokio::net::windows::named_pipe::ClientOptions::new().open(&path_str) {
            Ok(client) => return client,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            Err(e) => panic!("Failed to connect to named pipe: {}", e),
        }
    }
}

#[tokio::test]
async fn test_uds_handshake() {
    #[cfg(unix)]
    let socket_path = std::env::temp_dir().join("sleipnir_test.sock");
    
    #[cfg(windows)]
    let socket_path = std::path::PathBuf::from(r"\\.\pipe\sleipnir_test_sock");

    let path_clone = socket_path.clone();

    // Spawn server in background
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move {
        let _ = start_uds_server(path_clone, tx).await;
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
