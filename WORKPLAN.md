# CCA - Work Plan

## Project Overview

**CCA (Claude Code Agentic)** - A Rust-based multi-agent orchestration system for Claude Code instances.

**Source Document**: `cca-design-proposal.md`

---

## Phase 0: Command Center Plugin MVP (CRITICAL PATH) ✅ COMPLETE

**Goal**: Minimal viable Command Center integration - User → Plugin → Coordinator → Response

### 0.1 MCP Plugin Skeleton
- [x] Create `cca-mcp` crate with basic MCP server implementation
- [x] Implement `cca_task` tool (sends tasks to daemon)
- [x] Implement `cca_status` tool (check task status)
- [x] JSON-RPC 2.0 message handling

### 0.2 Minimal Daemon
- [x] Basic daemon that receives tasks from plugin
- [x] Single Coordinator agent spawning
- [x] Task forwarding to Coordinator
- [x] Response collection and return to plugin

### 0.3 Integration Test
- [x] Install plugin in Claude Code
- [x] Verify: User prompt → Plugin → Coordinator → Response
- [x] End-to-end flow working

**Milestone**: User types in CC, task goes to Coordinator, response returns ✅

---

## Phase 1: Foundation ✅ COMPLETE

**Goal**: Full daemon with multi-agent spawning and PTY management

### 1.1 Project Setup
- [x] Initialize Rust workspace with Cargo
- [x] Set up CI/CD (GitHub Actions)
- [x] Configure linting (clippy) and formatting (rustfmt)
- [x] Set up test infrastructure (42 unit tests across crates)

### 1.2 Core Daemon
- [x] Implement full `CCADaemon` struct
- [x] Process lifecycle management
- [x] Signal handling (SIGTERM, SIGINT)
- [x] Configuration loading (toml)

### 1.3 Agent Manager
- [x] PTY creation and management (portable-pty crate)
- [x] Multi-agent Claude Code subprocess spawning
- [x] Basic send/receive via PTY
- [x] Agent state tracking

### 1.4 CLI Basics (Debug Only)
- [x] `cca daemon start/stop/status`
- [x] `cca agent spawn/stop/list`
- [x] `cca agent attach` (manual intervention)

**Milestone**: Coordinator + Execution agents working, CC integration solid ✅

---

## Phase 2: Communication ✅ COMPLETE

**Goal**: Inter-agent communication via Redis and ACP

### 2.1 Redis Integration ✅
- [x] Connection pooling (deadpool-redis)
- [x] Session state storage
- [x] Context caching
- [x] Pub/Sub implementation

### 2.2 ACP WebSocket ✅
- [x] WebSocket server (tokio-tungstenite)
- [x] JSON-RPC 2.0 message handling
- [x] Per-agent connections
- [x] Reconnection logic

### 2.3 Coordinator Routing ✅
- [x] Task delegation from Coordinator to specialists
- [x] Result aggregation back to Coordinator
- [x] Broadcast messages

### 2.4 CC Plugin Enhancements ✅
- [x] `cca_activity` tool - show what agents are doing
- [x] `cca_acp_status` tool - ACP WebSocket connection status
- [x] `cca_broadcast` tool - broadcast messages to all agents
- [x] `cca_workloads` tool - workload distribution across agents
- [x] Real-time status streaming (via ACP WebSocket)
- [x] Progress indicators for long tasks

**Milestone**: Full CC → Coordinator → Agents → CC flow ✅

---

## Phase 3: Persistence ✅ COMPLETE

**Goal**: PostgreSQL integration with vector search

### 3.1 PostgreSQL Setup
- [x] Connection pooling (sqlx)
- [x] Migration system (sqlx-cli)
- [x] Schema implementation

### 3.2 ReasoningBank
- [x] Pattern storage
- [x] pgvector integration
- [x] Embedding generation (local or API) - placeholder ready
- [x] Similarity search

### 3.3 Context Persistence
- [x] Context snapshot storage
- [x] Compression (lz4/zstd) - placeholder ready
- [x] Recovery on restart

### 3.4 CC Plugin Memory Tools
- [x] `cca_memory` tool - query patterns from CC
- [x] `/api/v1/postgres/status` endpoint
- [x] `/api/v1/memory/search` endpoint

**Milestone**: Persistent memory across sessions, accessible from CC ✅

---

## Phase 4: Learning ✅ COMPLETE

**Goal**: RL engine for optimization

### 4.1 RL Framework ✅
- [x] `RLAlgorithm` trait definition
- [x] Experience replay buffer (`ExperienceBuffer`)
- [x] Training loop (`RLEngine.train()`)

### 4.2 Algorithm Implementations ✅
- [x] Q-Learning (tabular) - fully working
- [x] DQN (neural network) - placeholder ready
- [x] PPO (Proximal Policy Optimization) - placeholder ready
- [ ] MCTS (Monte Carlo Tree Search) - future enhancement
- [ ] Additional: A2C, SAC, TD3, Rainbow, Decision Transformer - future enhancements

### 4.3 Daemon Integration ✅
- [x] RLService async wrapper with PostgreSQL persistence
- [x] RLConfig for configuration (batch_size, train_interval, buffer_capacity)
- [x] StateBuilder helper for task/agent states
- [x] compute_reward function for task outcomes
- [x] API endpoints: `/api/v1/rl/stats`, `/api/v1/rl/train`, `/api/v1/rl/algorithm`, `/api/v1/rl/params`

### 4.4 CC Plugin Tools ✅
- [x] `cca_rl_status` tool - get RL engine stats
- [x] `cca_rl_train` tool - trigger training
- [x] `cca_rl_algorithm` tool - switch algorithms

### 4.5 Orchestrator Integration ✅
- [x] Task routing optimization based on RL predictions (`find_best_agent_rl`)
- [x] Agent workload stats tracking (success_rate, avg_completion_time)
- [x] Experience recording on task completion (`process_result`)
- [x] Reward computation based on success, tokens, and duration
- [x] Orchestrator wired with RLService in daemon.rs

**Milestone**: Adaptive task routing, smarter Coordinator decisions ✅

---

## Phase 5: Token Efficiency ✅ COMPLETE

**Goal**: 30%+ token reduction

### 5.1 Context Analysis ✅
- [x] Token counting per agent (`TokenCounter` with BPE-like estimation)
- [x] Redundancy detection across agents (`ContextAnalyzer` with n-gram similarity)

### 5.2 Compression Strategies ✅
- [x] Context distillation (code comment removal)
- [x] Summary injection (message summarization)
- [x] Selective history pruning (keep recent + important)
- [x] Cross-agent context sharing (deduplication)

### 5.3 Monitoring ✅
- [x] Token usage tracking via `TokenMetrics` with per-agent breakdown
- [x] Efficiency metrics and recommendations API endpoints
- [x] MCP tools: `cca_tokens_analyze`, `cca_tokens_compress`, `cca_tokens_metrics`, `cca_tokens_recommendations`

**Milestone**: Token efficiency infrastructure complete ✅

---

## Phase 6: Polish & Release

**Goal**: Production-ready v1.0.0

### 6.1 Testing
- [x] Integration tests (63 tests across 4 test files)
  - `crates/cca-daemon/tests/api_integration.rs` (20 tests) - API endpoints using axum-test
  - `crates/cca-daemon/tests/token_service_integration.rs` (12 tests) - Token service components
  - `crates/cca-mcp/tests/mcp_tools_integration.rs` (16 tests) - MCP tools using wiremock
  - `crates/cca-rl/tests/rl_integration.rs` (15 tests) - RL engine algorithms
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

## Phase 7: Linting & Refactoring

**Goal**: Clean, idiomatic Rust code with zero warnings

### 7.1 Clippy Linting
- [ ] Fix all clippy warnings (pedantic level)
- [ ] Address dead code warnings
- [ ] Fix unused imports and variables
- [ ] Resolve type complexity warnings

### 7.2 Code Refactoring
- [ ] Remove duplicate code patterns
- [ ] Simplify complex functions
- [ ] Improve error handling consistency
- [ ] Standardize naming conventions

### 7.3 API Cleanup
- [ ] Remove unused public APIs
- [ ] Consolidate similar endpoints
- [ ] Improve request/response types
- [ ] Add proper documentation comments

### 7.4 Dependency Audit
- [ ] Remove unused dependencies
- [ ] Update outdated crates
- [ ] Check for security advisories
- [ ] Optimize feature flags

**Milestone**: Zero warnings, clean `cargo clippy -- -W clippy::pedantic`

---

## Phase 8: QA & Security Review

**Goal**: Production-grade security and quality

### 8.1 Code Review
- [ ] Review all crates for bugs and logic errors
- [ ] Check for race conditions in async code
- [ ] Verify error handling paths
- [ ] Review API input validation

### 8.2 Security Audit
- [ ] OWASP Top 10 vulnerability check
- [ ] Input sanitization review
- [ ] Authentication/authorization audit
- [ ] Secrets handling review

### 8.3 Dependency Security
- [ ] Run `cargo audit` for CVEs
- [ ] Review transitive dependencies
- [ ] Check for supply chain risks
- [ ] Verify dependency licenses

### 8.4 Hardening
- [ ] Add rate limiting
- [ ] Implement request validation
- [ ] Add security headers
- [ ] Configure proper timeouts

**Milestone**: Security-reviewed, production-hardened codebase

---

## Technology Stack

| Component | Technology | Crate/Version |
|-----------|------------|---------------|
| Async Runtime | Tokio (async/await) | `tokio` |
| PTY | portable-pty | `portable-pty` |
| WebSocket | Tungstenite | `tokio-tungstenite` |
| Redis | deadpool-redis | `deadpool-redis` |
| PostgreSQL | sqlx + pg17 | `sqlx` (pg17 via pgvector) |
| Vector Search | pgvector | `pgvector` (1536-dim embeddings) |
| Serialization | serde | `serde`, `serde_json` |
| CLI | clap | `clap` |
| Config | toml + config | `config`, `toml` |
| Logging | tracing | `tracing`, `tracing-subscriber` |
| Metrics | prometheus | `prometheus` |

**Note**: Uses PostgreSQL 17 (latest stable with pgvector). When pg18 becomes stable with pgvector support, update `docker-compose.yml`.

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

**Phase**: 6 - Polish & Release
**Status**: 🏗️ IN PROGRESS (Integration Tests Complete)

**Completed (Phase 0)**:
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

**Completed (Phase 1)**:
- ✅ **1.1 CI/CD**: GitHub Actions workflow (`.github/workflows/ci.yml`)
- ✅ **1.1 Linting**: clippy and rustfmt configuration
- ✅ **1.1 Testing**: 65 unit tests across all crates
- ✅ **1.2 Signal Handling**: SIGINT and SIGTERM with graceful shutdown
- ✅ **1.4 CLI**: Full `cca daemon start/stop/status/logs` commands
- ✅ **1.4 CLI**: Full `cca agent spawn/stop/list/attach/send/logs` commands
- ✅ **1.4 CLI**: PID file management for daemon lifecycle

**Completed (Phase 2)**:
- ✅ **2.1 Redis**: Connection pooling, session storage, context caching, Pub/Sub
- ✅ **2.2 ACP WebSocket**: Server with JSON-RPC 2.0, per-agent connections, reconnection logic
- ✅ **2.3 Orchestrator**: Task delegation, result aggregation, broadcast messaging
- ✅ **2.4 MCP Plugin**: `cca_acp_status`, `cca_broadcast`, `cca_workloads` tools

**Completed (Phase 3)**:
- ✅ **3.1 PostgreSQL**: Connection pooling with sqlx, full repository pattern
- ✅ **3.1 PostgreSQL**: AgentRepository, PatternRepository, TaskRepository
- ✅ **3.1 PostgreSQL**: ContextSnapshotRepository, RLExperienceRepository
- ✅ **3.2 ReasoningBank**: pgvector similarity search (cosine distance)
- ✅ **3.2 ReasoningBank**: Text search fallback, pattern success/failure tracking
- ✅ **3.3 Context Persistence**: Snapshot storage with compression support
- ✅ **3.4 MCP Plugin**: `cca_memory` tool wired to PatternRepository
- ✅ **3.4 API Endpoints**: `/api/v1/postgres/status`, `/api/v1/memory/search`

**Completed (Phase 4)**:
- ✅ **4.1 RL Framework**: RLAlgorithm trait, ExperienceBuffer, RLEngine training loop
- ✅ **4.2 Algorithms**: Q-Learning working, DQN/PPO placeholders
- ✅ **4.3 Daemon Integration**: RLService with PostgreSQL persistence
- ✅ **4.3 API Endpoints**: `/api/v1/rl/stats`, `/api/v1/rl/train`, `/api/v1/rl/algorithm`, `/api/v1/rl/params`
- ✅ **4.4 MCP Plugin**: `cca_rl_status`, `cca_rl_train`, `cca_rl_algorithm` tools
- ✅ **4.5 Orchestrator Integration**: RL-based task routing with `find_best_agent_rl`
- ✅ **4.5 Stats Tracking**: Agent success_rate, avg_completion_time tracking
- ✅ **4.5 Experience Recording**: Reward computation and experience storage on task completion

**Completed (Phase 5)**:
- ✅ **5.1 Context Analysis**: `TokenCounter` with BPE-like estimation, redundancy detection
- ✅ **5.2 Compression**: Code comment removal, history pruning, summarization, deduplication
- ✅ **5.3 Monitoring**: `TokenMetrics` with per-agent tracking, recommendations API
- ✅ **5.3 API Endpoints**: `/api/v1/tokens/analyze`, `/api/v1/tokens/compress`, `/api/v1/tokens/metrics`, `/api/v1/tokens/recommendations`
- ✅ **5.3 MCP Plugin**: `cca_tokens_analyze`, `cca_tokens_compress`, `cca_tokens_metrics`, `cca_tokens_recommendations` tools

**To Run**:
```bash
# 1. Start infrastructure
docker-compose up -d

# 2. Start daemon (background)
cca daemon start

# 3. Or start in foreground
cca daemon start --foreground

# 4. Check status
cca daemon status

# 5. List agents
cca agent list

# 6. Stop daemon
cca daemon stop
```

**In Progress (Phase 6)**:
- ✅ **6.1 Integration Tests**: 63 tests across 4 test files
  - API integration tests (20 tests) with axum-test mock handlers
  - Token service tests (12 tests) for token counting and compression
  - MCP tools tests (16 tests) with wiremock HTTP mocking
  - RL engine tests (15 tests) for algorithms and experience buffer

**Next Step**: Phase 6 - Load testing, Chaos testing, Documentation, Packaging, Performance
