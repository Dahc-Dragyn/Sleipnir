# Project Sleipnir

Sleipnir is a high-performance "Human-on-the-Loop" supervisor daemon built in Rust. It serves as a strict interception and auditing layer for autonomous AI agents, ensuring that high-risk actions (like destructive database queries or sensitive API calls) are paused, reviewed, and explicitly approved by a human operator before execution.

## Architecture

The project is split into the following components:
- **sleipnir-core**: Shared data models and Serde serialization logic for payloads (`AgentActionFrame`, `HandshakeResolutionFrame`, `PolicyDecision`, etc.).
- **sleipnir-daemon**: The Rust-based supervisor daemon. Features a synchronous terminal dashboard (powered by `ratatui` and `crossterm`) connected to an asynchronous `tokio` backend. Includes a hot-reloadable `PolicyEngine` to match incoming payloads against regex-defined threat patterns.
- **sleipnir-client**: A suite of agent-side python harnesses (`mock_agent.py`, `stress_test.py`, `test_agent.py`) that simulate AI interactions with the gatekeeper. 

## Key Features
- **Cross-Platform IPC**: Seamlessly handles sandboxed local telemetry via **Local TCP Loopback (`127.0.0.1:47777`)** on Windows (to bypass named pipe instance exhaustion), and POSIX Unix Domain Sockets (`UDS`) on Linux/macOS.
- **Regex Policy Engine**: Hot-reloadable `Policy.toml` configuration to automatically categorize tools into `Allow`, `Verify`, or `Deny` tiers. `Allow` tools execute silently in the background, while `Verify` or `Deny` tools freeze the daemon for operator review.
- **Terminal UI (TUI)**: A non-blocking, responsive dashboard detailing telemetry and blocked frames.
- **Interactive Gates**: 
  - `y`: Approve the tool invocation.
  - `n`: Deny the tool invocation.
  - `m`: Mutate the payload. Opens an inline `tui-textarea` to edit the arguments safely before submission (press `F9` to submit).
  - `F8`: Forward the payload to the Chandrian adversarial fuzz pool.
- **Disconnected Fallback Circuitry**: Client scripts implement localized timeouts and catch blocks (`Local Safe Mode`) to gracefully abort execution if the Rust daemon crashes or drops the connection.

## Getting Started

1. **Run the Daemon**:
   ```powershell
   cd sleipnir
   cargo run --bin sleipnir-daemon
   ```
2. **Run the UAT Mock Agent (in a new terminal)**:
   ```powershell
   cd sleipnir
   python sleipnir-client/mock_agent.py
   ```
3. **Interact**: The Mock Agent simulates a full autonomous workflow. It will auto-approve low-risk directory scans, and the daemon will instantly halt and wait for your keystrokes (`y`/`n`/`m`) when it attempts destructive commands (e.g. `rm -rf /` or `DROP DATABASE`).
