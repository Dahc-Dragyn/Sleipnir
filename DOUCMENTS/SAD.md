System Architecture Document (SAD)
Project Sleipnir: Local High-Speed Agent Orchestration Workbench

Author: Chad Nygard Version: v1.0 Status: Approved / Architectural Baseline Date: June 2026

1. Introduction & Architectural Principles
This document details the high-level design, structural topology, interface definitions, and data pathways for
Project Sleipnir. Sleipnir is an elite, local-first system supervisor engineered to govern autonomous execution
loops across disparate agent processes. The engineering choices outlined herein prioritize absolute
deterministic latency, low compute overhead, and strict sandboxed safety profiles.
The design is driven by three architectural pillars:
Zero Resource Exhaustion: The management layer must remain sub-millisecond in message routing and
operate with a memory footprint below 30MB, preserving available VRAM/RAM for local foundational
models.
Decoupled Isolation: Governance logic is decoupled from execution logic. Agents operate within separate
memory spaces, reporting state out-of-band to a central authority.
Fail-Secure Localism: The entire topology functions without external network listeners. If communication
bounds break, individual node logic immediately fails closed.
2. High-Level Architecture & Component Interactions
Sleipnir is architected as an event-driven, asymmetric runtime broken into an administrative daemon with an
integrated terminal user interface (TUI) and an lightweight operational client embedded into individual target
agents.

Figure 2.1: Structural Topology & Subsystem Communication Matrix
+---------------------------------------------------------------------------------------+ |
SLEIPNIR RUNTIME | | | | +-----------------------+ Local IPC Channel
+--------------------------+ | | | Terminal Frontend | <==========================> | Core
Orchestration | | | | (Ratatui Async Loop) | Internal Tokio Channel | Daemon | | |
+-----------------------+ +--------------------------+ | +--------------
^--------------------------------------------------------^---------------+ | | | (User Input
Override) | (Policy Clearing) v v
+---------------------------------------------------------------------------------------+ |
AGENT EXECUTION ENVIRONMENT (Python / Rust Sandbox Processes) | | | | +-----------------------+
Unix Domain Socket +--------------------------+ | | | Core Agent Logic |
<--------------------------> | Sleipnir Interceptor Client| | | | (Thought/Tool Generation) |
JSON RPC Handshake | (SDK Library) | | | +-----------------------+ +--------------------------+
| +---------------------------------------------------------------------------------------+
•

•

•

Project Sleipnir - System Architecture Document 1

2.1 Subsystem Explanations

Sleipnir Orchestration Daemon: Built using Rust and the Tokio asynchronous runtime. It maintains non-
blocking Unix Domain Socket listeners, multiplexes incoming state streams, tracks target system telemetry,

and serves as the sole authoritative evaluator of policies.
Ratatui Interactive UI: A multi-threaded visual terminal thread that consumes render state changes. It
operates off a localized read-only mirror of the engine state to prevent rendering freezes from bottlenecking
underlying socket buffers.
Sleipnir Interceptor Client (SDK): A pluggable crate (Rust) or module (Python) initialized inside the target
agent's workflow. It overrides or hooks the agent's internal tool executor, blocking action completion
whenever a transaction demands a policy handshake.
3. Data Flow Architecture & Handshake Mechanics
The data exchange patterns within Sleipnir alternate between a high-throughput, non-blocking Reactive State
Stream and a synchronized, blocking Gatekeeper Handshake Loop.

Figure 3.1: Sequence Diagram of Policy Evaluation and Escalation Paths
Agent Process Sleipnir Daemon User Terminal / UI | | | |--- 1. State Update (JSON) ------>| | |
(Thought/Monologue - Stream) |--- 2. Render Frame ------------->| (Displays Mind's Eye) | | |
|--- 3. Proposed Action ---------->| | | (e.g., Tool: bash_exec) |--- 4. Evaluate Policy.toml |
| | | | |==== ESCALATION TRIGGERED =======| | | | | |--- 5. Flash Alert Alert -------->|
(Flashes UI Block) | [BLOCKED & AWAITING IPC IPC] | | | | | [Awaiting Input] | |<-- 6. Keyboard
Command (y/n/e) --| (User Action) | | | |<-- 7. Handshake Resolution ------| | | (Approve /
Deny / Mutate) | | v v v

3.1 Serialization Payloads
Communication across the IPC mechanism utilizes structured, strongly-typed JSON frames to verify schema
safety across foreign languages (e.g., Python agent to Rust engine). There are two critical frames:
Action Submission Frame (Agent → Sleipnir):
•

•

•

{
"version": "1.0",
"transaction_id": "tx_98234-a8f",
"agent_id": "neuroweave-core-01",
"timestamp": 1782121500,
"payload": {
"type": "ToolInvocation",
"tool_name": "execute_bash",
"arguments": {
"command": "rm -rf ./target/debug/crap"
}
},

Project Sleipnir - System Architecture Document 2

Handshake Resolution Frame (Sleipnir → Agent):

4. Detailed Component Technical Specifications
4.1 The Centralized Policy Engine
The system policy router compiles configuration definitions from a hot-reloadable Policy.toml file. It avoids
internal database allocations by building an in-memory execution tree utilizing efficient string matching
algorithms and thread-safe regular expressions compiled via the regex crate.
Architectural Constraint: Structural Synchronization
To avoid race conditions when reloading rules during active execution sequences, the global policy engine is
wrapped inside an asynchronous Read-Write Lock ( tokio::sync::RwLock ). Active evaluations obtain a
brief read guard, while structural disk-reload loops triggered via system file notifications invoke an absolute
write lock, safely blocking incoming evaluations for less than 15 microseconds.

4.2 Disconnected Fallback Circuitry
To prevent blocking deadlocks if the Sleipnir daemon crashes or is killed by the operating system, the client
interceptor integrates an automated heart-beat timeout circuit. If a blocking handshake request does not
receive a response within 2000 milliseconds, or if the socket returns an unrecoverable BrokenPipe or
ConnectionReset error, the local client code intercepts execution, aborts the current tool call loop, and
activates an isolated, ultra-strict fallback rule configuration.
"context_monologue": "Cleaning up debug compilation artifacts to save workspace space."
}

{
"transaction_id": "tx_98234-a8f",
"status": "Mutated",
"action_decision": "Approve",
"mutated_payload": {
"tool_name": "execute_bash",
"arguments": {
"command": "rm -f ./target/debug/crap"
}
},
"audit_reason": "Regex pattern matched 'rm -rf'. Mutated via User Control Console to
remove recursive flag."
}

Project Sleipnir - System Architecture Document 3

5. Technology Stack & Dependency Justification

Component
Layer

Technology / Library
Selected

Architectural Trade-Off / Rationale

Core Engine &
Daemon

Rust (2021 Edition) +
Tokio Async

Guarantees absolute memory safety and data-race elimination
without an active garbage collection cycle. Tokio delivers
predictable, non-blocking asynchronous task execution loops.

User Interface

ratatui +
crossterm crates

Draws text-driven visual blocks directly to terminal memory
buffers. Eliminates the bloat, CPU overhead, and security
vectors associated with running an Electron or web-browser UI
stack.

Inter-Process
Comm.

Unix Domain Sockets
(POSIX) / Named Pipes
(Win)

Bypasses the entire local network stack loop. Guarantees that
communication layers never open external TCP/UDP ports,
preventing network interception.

File Watching notify crate

Hooks into native kernel event paths (inotify, KQueue) to
execute immediate, hot-reloads of system configuration profiles
without polling threads.

6. Architectural Trade-offs & Limitations
IPC Latency vs. Memory Isolation: Marshaling structured JSON frames across a Unix Domain socket
incurs a slight serialization performance penalty compared to linking directly to sharing memory segments.
However, parsing standard JSON creates an absolute memory barrier that protects the Sleipnir process
from memory leaks or unstable crashes originating inside experimental Python agent code blocks.
Regex Processing vs. Semantic Policy Assessment: Phase 1 relies strictly on deterministic string
checking and regex pattern matches for security verification. While this lacks the deep contextual
understanding of using a secondary local model to assess intent risk, it ensures evaluation delays stay
sub-millisecond, preserving CPU and memory resources.