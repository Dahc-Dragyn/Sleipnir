# Project Sleipnir

Sleipnir is a high-performance "Human-on-the-Loop" supervisor daemon built in Rust. It serves as an interception and auditing layer for autonomous AI agents, ensuring that high-risk actions (like destructive database queries or sensitive API calls) are paused, reviewed, and explicitly approved by a human operator before execution.

## Architecture

The project is split into the following components:
- **sleipnir-core**: Shared data models and serialization logic (`AgentActionFrame`, `HandshakeResolutionFrame`, etc.).
- **sleipnir-daemon**: The Rust-based supervisor daemon. Features a synchronous terminal dashboard (powered by `ratatui` and `crossterm`) connected to an asynchronous backend. 
- **sleipnir-client** / **test_agent.py**: Agent-side client logic. The Python harness connects cross-platform using Unix Domain Sockets (POSIX) or Named Pipes (Windows) to transmit `ToolInvocation` frames.

## Key Features
- **Cross-Platform IPC**: Seamlessly handles Windows Named Pipes (`\\.\pipe\sleipnir.sock`) and POSIX UDS.
- **Terminal UI (TUI)**: A non-blocking, responsive dashboard detailing telemetry and blocked frames.
- **Interactive Gates**: 
  - `y`: Approve the tool invocation.
  - `n`: Deny the tool invocation.
  - `m`: Mutate the payload. Opens an inline `tui-textarea` to edit the arguments safely before submission (press `F9` to submit).
- **Asynchronous Decoupling**: Uses `tokio::sync::mpsc` for state updates and non-cloneable `tokio::sync::oneshot` channels for single-use transaction resolutions.

## Getting Started

1. **Run the Daemon**:
   ```powershell
   cd sleipnir
   cargo run --bin sleipnir-daemon
   ```
2. **Run the Agent (in a new terminal)**:
   ```powershell
   cd sleipnir
   python test_agent.py
   ```
3. **Interact**: When the agent sends a payload, the daemon will halt in the `Blocked` state. Use the keyboard bindings to resolve it.
