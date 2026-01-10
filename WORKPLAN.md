# CCA - Work Plan

## Project Overview

**CCA (Claude Code Agentic)** - A Rust-based multi-agent orchestration system for Claude Code instances.

**Source Document**: `cca-design-proposal.md`

---

## Phase 0: Command Center Plugin MVP (CRITICAL PATH)

**Goal**: Minimal viable Command Center integration - User → Plugin → Coordinator → Response

### 0.1 MCP Plugin Skeleton
- [ ] Create `cca-mcp` crate with basic MCP server implementation
- [ ] Implement `cca_task` tool (sends tasks to daemon)
- [ ] Implement `cca_status` tool (check task status)
- [ ] JSON-RPC 2.0 message handling

### 0.2 Minimal Daemon
- [ ] Basic daemon that receives tasks from plugin
- [ ] Single Coordinator agent spawning
- [ ] Task forwarding to Coordinator
- [ ] Response collection and return to plugin

### 0.3 Integration Test
- [ ] Install plugin in Claude Code
- [ ] Verify: User prompt → Plugin → Coordinator → Response
- [ ] End-to-end flow working

**Milestone**: User types in CC, task goes to Coordinator, response returns

---

## Phase 1: Foundation

**Goal**: Full daemon with multi-agent spawning and PTY management

### 1.1 Project Setup
- [ ] Initialize Rust workspace with Cargo
- [ ] Set up CI/CD (GitHub Actions)
- [ ] Configure linting (clippy) and formatting (rustfmt)
- [ ] Set up test infrastructure

### 1.2 Core Daemon
- [ ] Implement full `CCADaemon` struct
- [ ] Process lifecycle management
- [ ] Signal handling (SIGTERM, SIGINT)
- [ ] Configuration loading (toml)

### 1.3 Agent Manager
- [ ] PTY creation and management (portable-pty crate)
- [ ] Multi-agent Claude Code subprocess spawning
- [ ] Basic send/receive via PTY
- [ ] Agent state tracking

### 1.4 CLI Basics (Debug Only)
- [ ] `cca daemon start/stop/status`
- [ ] `cca agent spawn/stop/list`
- [ ] `cca agent attach` (manual intervention)

**Milestone**: Coordinator + Execution agents working, CC integration solid

---

## Phase 2: Communication

**Goal**: Inter-agent communication via Redis and ACP

### 2.1 Redis Integration
- [ ] Connection pooling (deadpool-redis)
- [ ] Session state storage
- [ ] Context caching
- [ ] Pub/Sub implementation

### 2.2 ACP WebSocket
- [ ] WebSocket server (tokio-tungstenite)
- [ ] JSON-RPC 2.0 message handling
- [ ] Per-agent connections
- [ ] Reconnection logic

### 2.3 Coordinator Routing
- [ ] Task delegation from Coordinator to specialists
- [ ] Result aggregation back to Coordinator
- [ ] Broadcast messages

### 2.4 CC Plugin Enhancements
- [ ] `cca_activity` tool - show what agents are doing
- [ ] Real-time status streaming
- [ ] Progress indicators for long tasks

**Milestone**: Full CC → Coordinator → Agents → CC flow

---

## Phase 3: Persistence

**Goal**: PostgreSQL integration with vector search

### 3.1 PostgreSQL Setup
- [ ] Connection pooling (sqlx)
- [ ] Migration system (sqlx-cli)
- [ ] Schema implementation

### 3.2 ReasoningBank
- [ ] Pattern storage
- [ ] pgvector integration
- [ ] Embedding generation (local or API)
- [ ] Similarity search

### 3.3 Context Persistence
- [ ] Context snapshot storage
- [ ] Compression (lz4/zstd)
- [ ] Recovery on restart

### 3.4 CC Plugin Memory Tools
- [ ] `cca_memory` tool - query patterns from CC
- [ ] Pattern suggestions during tasks

**Milestone**: Persistent memory across sessions, accessible from CC

---

## Phase 4: Learning

**Goal**: RL engine for optimization

### 4.1 RL Framework
- [ ] `RLAlgorithm` trait definition
- [ ] Experience replay buffer
- [ ] Training loop

### 4.2 Algorithm Implementations
- [ ] Q-Learning (tabular)
- [ ] DQN (neural network)
- [ ] PPO (Proximal Policy Optimization)
- [ ] MCTS (Monte Carlo Tree Search)
- [ ] Additional: A2C, SAC, TD3, Rainbow, Decision Transformer

### 4.3 Coordinator Integration
- [ ] Task routing optimization based on RL
- [ ] Token budget allocation
- [ ] Success pattern learning

### 4.4 Monitoring
- [ ] Training metrics exposed via CC plugin
- [ ] Performance dashboards

**Milestone**: Adaptive task routing, smarter Coordinator decisions

---

## Phase 5: Token Efficiency

**Goal**: 30%+ token reduction

### 5.1 Context Analysis
- [ ] Token counting per agent
- [ ] Redundancy detection across agents

### 5.2 Compression Strategies
- [ ] Context distillation
- [ ] Summary injection
- [ ] Selective history pruning
- [ ] Cross-agent context sharing

### 5.3 Monitoring
- [ ] Token usage tracking in CC
- [ ] Efficiency metrics and recommendations

**Milestone**: Measurable 30% token reduction

---

## Phase 6: Polish & Release

**Goal**: Production-ready v1.0.0

### 6.1 Testing
- [ ] Integration tests
- [ ] Load testing
- [ ] Chaos testing

### 6.2 Documentation
- [ ] User guide
- [ ] API reference
- [ ] Architecture docs

### 6.3 Packaging
- [ ] Binary releases
- [ ] Docker image
- [ ] Homebrew formula

### 6.4 Performance
- [ ] Profiling
- [ ] Optimization
- [ ] Benchmarks

**Milestone**: v1.0.0 Release

---

## Technology Stack

| Component | Technology | Crate |
|-----------|------------|-------|
| Async Runtime | Tokio | `tokio` |
| PTY | portable-pty | `portable-pty` |
| WebSocket | Tungstenite | `tokio-tungstenite` |
| Redis | deadpool-redis | `deadpool-redis` |
| PostgreSQL | sqlx | `sqlx` |
| Vector Search | pgvector | `pgvector` |
| Serialization | serde | `serde`, `serde_json` |
| CLI | clap | `clap` |
| Config | toml + config | `config`, `toml` |
| Logging | tracing | `tracing`, `tracing-subscriber` |
| Metrics | prometheus | `prometheus` |

---

## Project Structure

```
cca/
├── Cargo.toml              # Workspace manifest
├── Cargo.lock
├── README.md
├── LICENSE
├── WORKPLAN.md             # This file
├── cca.toml.example        # Example configuration
├── docker-compose.yml      # Redis + PostgreSQL
│
├── crates/
│   ├── cca-core/           # Core library (types, traits)
│   ├── cca-daemon/         # Main daemon binary
│   ├── cca-cli/            # CLI binary
│   ├── cca-mcp/            # MCP server plugin
│   ├── cca-acp/            # ACP protocol
│   └── cca-rl/             # RL algorithms
│
├── migrations/             # SQL migrations
├── agents/                 # Agent CLAUDE.md files
├── tests/                  # Integration tests
└── docs/                   # Documentation
```

---

## Success Metrics

| Metric | Target |
|--------|--------|
| Token Efficiency | 30% reduction |
| Agent Spawn Time | < 2 seconds |
| Message Latency | < 50ms P99 |
| Memory Query | < 10ms P99 |
| Context Recovery | < 5 seconds |
| Uptime | 99.9% |

---

## Current Status

**Phase**: 0 - Command Center Plugin MVP
**Status**: ✅ PHASE 0 COMPLETE

**Completed**:
- ✅ Rust workspace with 6 crates
- ✅ Docker infrastructure (PostgreSQL:5433, Redis:6380)
- ✅ Daemon running at 127.0.0.1:9200 with full API
- ✅ All crate skeletons with proper dependencies
- ✅ Agent CLAUDE.md files for all 7 roles
- ✅ **0.1 MCP Plugin**: HTTP client (`client.rs`) with daemon communication
- ✅ **0.1 MCP Plugin**: Tools wired to call daemon endpoints
- ✅ **0.2 Daemon**: Full API endpoints (tasks, agents, status, activity)
- ✅ **0.2 Daemon**: PTY-based agent spawning via `portable-pty`
- ✅ **0.2 Daemon**: Task routing to Coordinator agent
- ✅ **0.2 Daemon**: PTY stdin/stdout communication
- ✅ **0.3 MCP Binary**: `cca-mcp` binary with CLI args
- ✅ **0.3 MCP Config**: `.claude/mcp_servers.json` for Claude Code
- ✅ **0.3 End-to-End**: MCP → Daemon → Agent spawning flow tested

**To Run**:
```bash
# 1. Start infrastructure
docker-compose up -d

# 2. Start daemon
./target/release/ccad

# 3. Configure Claude Code (copy MCP config)
cp .claude/mcp_servers.json ~/.config/claude-code/mcp_servers.json

# 4. Use cca_task, cca_status, etc. from Claude Code
```

**Next Step**: Phase 1 - Full daemon with multi-agent spawning and CI/CD
