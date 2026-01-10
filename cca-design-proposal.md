# CCA - Claude Code Agentic

## Design Proposal & Development Plan

**Date**: 2025-01-10
**Version**: 0.1.0 (Design Phase)
**Status**: Proposal

---

## Executive Summary

CCA (Claude Code Agentic) is a next-generation multi-agent orchestration system written entirely in Rust. It combines the best features from analyzed solutions:

| Source | Feature Adopted |
| ------ | --------------- |
| **CCSwarm** | Rust implementation, native PTY, ACP/WebSocket |
| **Claude-Flow** | Persistent memory, RL learning, token efficiency |
| **TSM-Agent** | True independent instances, manual intervention |

**Key Differentiators:**

- Pure Rust for performance and safety
- True independent Claude Code instances (not simulated agents)
- PostgreSQL + pgvector for enterprise-grade persistence
- Redis for real-time session state and pub/sub messaging
- MCP/ACP protocol for standardized agent communication
- Reinforcement learning for task optimization
- **Command Center Architecture** - Single entry point via Claude Code plugin

---

## Core Feature: Command Center Architecture

**This is the primary and most critical feature of CCA.**

All user interaction flows through a single **Command Center (CC)** - a Claude Code instance with the CCA plugin installed. The user never needs to run separate CLI commands to send tasks.

### Communication Flow

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│   ┌──────────────────────────────────────────────────────────────────────┐  │
│   │                    COMMAND CENTER (CC)                                │  │
│   │              User's Primary Claude Code Instance                      │  │
│   │                                                                       │  │
│   │   User types: "Add authentication to the API"                        │  │
│   │                                                                       │  │
│   │   ┌─────────────────────────────────────────────────────────────┐   │  │
│   │   │                    CCA Plugin (MCP)                          │   │  │
│   │   │   - Analyzes user request                                    │   │  │
│   │   │   - Formats task for Coordinator                             │   │  │
│   │   │   - Receives and displays results                            │   │  │
│   │   └─────────────────────────────────────────────────────────────┘   │  │
│   └──────────────────────────────────────────────────────────────────────┘  │
│                                      │                                       │
│                                      │ MCP/ACP                               │
│                                      ▼                                       │
│   ┌──────────────────────────────────────────────────────────────────────┐  │
│   │                    COORDINATOR AGENT                                  │  │
│   │              Persistent Claude Code Instance                          │  │
│   │                                                                       │  │
│   │   - Receives task from CC                                            │  │
│   │   - Analyzes requirements                                            │  │
│   │   - Routes to appropriate Execution Agents                           │  │
│   │   - Aggregates results                                               │  │
│   │   - Returns summary to CC                                            │  │
│   └──────────────────────────────────────────────────────────────────────┘  │
│                                      │                                       │
│                    ┌─────────────────┼─────────────────┐                    │
│                    │                 │                 │                    │
│                    ▼                 ▼                 ▼                    │
│   ┌────────────────────┐ ┌────────────────────┐ ┌────────────────────┐     │
│   │  EXECUTION AGENT   │ │  EXECUTION AGENT   │ │  EXECUTION AGENT   │     │
│   │    (go-backend)    │ │    (frontend)      │ │    (security)      │     │
│   │                    │ │                    │ │                    │     │
│   │  Persistent CC     │ │  Persistent CC     │ │  Persistent CC     │     │
│   │  with full context │ │  with full context │ │  with full context │     │
│   └────────────────────┘ └────────────────────┘ └────────────────────┘     │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Why This Architecture?

| Approach | Problem | CCA Solution |
| -------- | ------- | ------------ |
| CLI per task | Loses context, manual effort | Single CC session maintains conversation |
| Spawning new instances | No continuity, wasteful | Persistent agents with full history |
| Direct agent commands | User must know routing | CC + Coordinator handle routing |

### User Experience

The user interacts with CCA naturally through the Command Center:

```text
User (in CC): "Add JWT authentication to the projects API endpoint"

CC (via CCA Plugin):
  → Sends to Coordinator
  → Coordinator analyzes: needs go-backend + security
  → go-backend: implements JWT middleware
  → security: reviews for vulnerabilities
  → Coordinator: aggregates results
  → CC: displays summary to user

User (in CC): "Now add unit tests for that"

CC (via CCA Plugin):
  → Coordinator remembers previous task context
  → Routes to qa agent
  → qa: writes tests based on previous implementation
  → Returns to user
```

### Key Principles

1. **Single Entry Point**: User only interacts with Command Center
2. **Context Preservation**: Each agent maintains full conversation history
3. **Intelligent Routing**: Coordinator decides which agents handle what
4. **Result Aggregation**: User sees unified response, not raw agent outputs
5. **Manual Override**: User CAN attach to any agent for direct intervention

### CCA Plugin for Command Center

The CCA plugin exposes MCP tools that CC uses automatically:

```rust
// Tools available to Command Center
pub struct CCAPlugin {
    daemon_connection: DaemonConnection,
}

impl CCAPlugin {
    /// Primary tool - send task through the system
    /// CC calls this automatically when user describes work
    async fn cca_task(&self, description: &str) -> Result<TaskResult> {
        // 1. Send to Coordinator
        // 2. Wait for completion
        // 3. Return formatted result
    }

    /// Check status of running task
    async fn cca_status(&self, task_id: Option<TaskId>) -> Result<Status>;

    /// Get agent activity (for transparency)
    async fn cca_activity(&self) -> Result<Vec<AgentActivity>>;

    /// Manual intervention - attach to specific agent
    async fn cca_attach(&self, agent: &str) -> Result<AttachSession>;

    /// Query learned patterns
    async fn cca_memory(&self, query: &str) -> Result<Vec<Pattern>>;
}
```

### Plugin Configuration

Add to Claude Code's MCP settings:

```json
{
  "mcpServers": {
    "cca": {
      "command": "cca",
      "args": ["mcp-serve"],
      "env": {
        "CCA_DAEMON_URL": "http://localhost:9200"
      }
    }
  }
}
```

### Automatic Tool Selection

The CCA plugin uses Claude's native tool selection. When user describes work:

1. Claude in CC recognizes it's a development task
2. Automatically calls `cca_task` tool
3. Plugin handles all routing internally
4. Returns result to CC conversation

No explicit commands needed - just natural conversation.

### CLI Commands (Testing/Debug Only)

The CLI remains available for debugging and testing:

```bash
# These are for development/debugging, NOT primary usage
cca agent list              # See agent states
cca agent attach backend    # Direct agent access
cca daemon logs             # View system logs
cca task status <id>        # Debug specific task
```

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         CCA - Claude Code Agentic                        │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                        CCA Daemon (Rust)                            │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │ │
│  │  │ Orchestrator │  │  RL Engine   │  │    Memory Manager        │  │ │
│  │  │   (Master)   │  │  (Learning)  │  │  (Redis + Postgres)      │  │ │
│  │  └──────┬───────┘  └──────┬───────┘  └────────────┬─────────────┘  │ │
│  │         │                 │                       │                 │ │
│  │         └─────────────────┼───────────────────────┘                 │ │
│  │                           │                                          │ │
│  │  ┌────────────────────────┴────────────────────────────────────┐   │ │
│  │  │                    Agent Manager                             │   │ │
│  │  │         (PTY Management + Process Supervision)               │   │ │
│  │  └────────────────────────┬────────────────────────────────────┘   │ │
│  └───────────────────────────┼────────────────────────────────────────┘ │
│                              │                                           │
│         ┌────────────────────┼────────────────────────┐                 │
│         │                    │                        │                 │
│         ▼                    ▼                        ▼                 │
│  ┌─────────────┐     ┌─────────────┐          ┌─────────────┐          │
│  │ Claude Code │     │ Claude Code │          │ Claude Code │          │
│  │ Instance 1  │     │ Instance 2  │   ...    │ Instance N  │          │
│  │ (frontend)  │     │ (backend)   │          │ (qa)        │          │
│  │             │     │             │          │             │          │
│  │  [PTY/ACP]  │     │  [PTY/ACP]  │          │  [PTY/ACP]  │          │
│  └──────┬──────┘     └──────┬──────┘          └──────┬──────┘          │
│         │                   │                        │                  │
│         └───────────────────┼────────────────────────┘                  │
│                             │                                           │
│                             ▼                                           │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                    Communication Layer                            │  │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌──────────────────┐  │  │
│  │  │  Redis Pub/Sub  │  │  ACP WebSocket  │  │   MCP Protocol   │  │  │
│  │  │  (Real-time)    │  │  (Agent Comm)   │  │   (Tool Invoke)  │  │  │
│  │  └─────────────────┘  └─────────────────┘  └──────────────────┘  │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                             │                                           │
│                             ▼                                           │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                     Persistence Layer                             │  │
│  │  ┌─────────────────────────┐  ┌────────────────────────────────┐ │  │
│  │  │        Redis            │  │         PostgreSQL             │ │  │
│  │  │  - Session state        │  │  - ReasoningBank (patterns)    │ │  │
│  │  │  - Context cache        │  │  - pgvector (embeddings)       │ │  │
│  │  │  - Pub/Sub channels     │  │  - Task history                │ │  │
│  │  │  - Rate limiting        │  │  - RL training data            │ │  │
│  │  └─────────────────────────┘  └────────────────────────────────┘ │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Core Components

### 1. CCA Daemon (`ccad`)

The main orchestration service written in Rust.

```rust
// Conceptual structure
pub struct CCADaemon {
    orchestrator: Orchestrator,
    agent_manager: AgentManager,
    memory_manager: MemoryManager,
    rl_engine: RLEngine,
    config: CCAConfig,
}

impl CCADaemon {
    pub async fn run(&self) -> Result<()>;
    pub async fn spawn_agent(&self, role: AgentRole) -> Result<AgentId>;
    pub async fn send_task(&self, agent: AgentId, task: Task) -> Result<TaskId>;
    pub async fn broadcast(&self, message: Message) -> Result<()>;
}
```

**Responsibilities:**

- Manage Claude Code subprocess lifecycle
- Route tasks between agents
- Coordinate RL-based optimization
- Handle graceful shutdown and recovery

### 2. Agent Manager

Manages true independent Claude Code instances via PTY.

```rust
pub struct AgentManager {
    agents: HashMap<AgentId, Agent>,
    pty_pool: PtyPool,
    acp_connections: HashMap<AgentId, AcpConnection>,
}

pub struct Agent {
    id: AgentId,
    role: AgentRole,
    pty: PtyHandle,
    process: Child,
    state: AgentState,
    context_hash: String,
}

pub enum AgentRole {
    Coordinator,
    Frontend,
    Backend,
    DBA,
    DevOps,
    Security,
    QA,
    Custom(String),
}
```

**Features:**

- Native PTY management (no tmux dependency)
- Process supervision with automatic restart
- ACP WebSocket connection per agent
- Context state tracking

### 3. Memory Manager

Hybrid memory system using Redis and PostgreSQL.

```rust
pub struct MemoryManager {
    redis: RedisPool,
    postgres: PgPool,
}

impl MemoryManager {
    // Redis operations (fast, ephemeral)
    pub async fn cache_context(&self, agent: AgentId, context: &Context) -> Result<()>;
    pub async fn get_context(&self, agent: AgentId) -> Result<Option<Context>>;
    pub async fn publish(&self, channel: &str, message: &Message) -> Result<()>;
    pub async fn subscribe(&self, channel: &str) -> Result<Subscription>;

    // PostgreSQL operations (persistent, searchable)
    pub async fn store_pattern(&self, pattern: &Pattern) -> Result<PatternId>;
    pub async fn search_patterns(&self, query: &str, k: usize) -> Result<Vec<Pattern>>;
    pub async fn store_embedding(&self, text: &str, embedding: Vec<f32>) -> Result<()>;
    pub async fn similarity_search(&self, embedding: Vec<f32>, k: usize) -> Result<Vec<Match>>;
}
```

### 4. RL Engine

Reinforcement learning for task optimization.

```rust
pub struct RLEngine {
    algorithms: HashMap<String, Box<dyn RLAlgorithm>>,
    training_data: TrainingDataStore,
    active_algorithm: String,
}

pub trait RLAlgorithm: Send + Sync {
    fn name(&self) -> &str;
    fn train(&mut self, experiences: &[Experience]) -> Result<()>;
    fn predict(&self, state: &State) -> Action;
    fn update(&mut self, reward: f64) -> Result<()>;
}

// Implemented algorithms
pub struct QLearning { /* ... */ }
pub struct PPO { /* ... */ }           // Proximal Policy Optimization
pub struct MCTS { /* ... */ }          // Monte Carlo Tree Search
pub struct DQN { /* ... */ }           // Deep Q-Network
pub struct A2C { /* ... */ }           // Advantage Actor-Critic
pub struct SAC { /* ... */ }           // Soft Actor-Critic
pub struct TD3 { /* ... */ }           // Twin Delayed DDPG
pub struct Rainbow { /* ... */ }       // Rainbow DQN
pub struct DecisionTransformer { /* ... */ }
```

**Use Cases:**

- Task routing optimization (which agent handles what)
- Token budget allocation
- Context compression decisions
- Success pattern recognition

### 5. Communication Layer

#### ACP (Agent Client Protocol)

```rust
pub struct AcpConnection {
    websocket: WebSocket,
    agent_id: AgentId,
    session_id: SessionId,
}

impl AcpConnection {
    pub async fn send(&self, message: AcpMessage) -> Result<()>;
    pub async fn receive(&self) -> Result<AcpMessage>;
    pub async fn rpc(&self, method: &str, params: Value) -> Result<Value>;
}

#[derive(Serialize, Deserialize)]
pub struct AcpMessage {
    jsonrpc: String,  // "2.0"
    id: Option<String>,
    method: Option<String>,
    params: Option<Value>,
    result: Option<Value>,
    error: Option<AcpError>,
}
```

#### Redis Pub/Sub

```rust
pub enum Channel {
    AgentStatus,           // cca:status:{agent_id}
    TaskQueue,             // cca:tasks:{agent_id}
    Broadcast,             // cca:broadcast
    Coordination,          // cca:coord
    Learning,              // cca:learning
}

#[derive(Serialize, Deserialize)]
pub struct InterAgentMessage {
    id: Uuid,
    from: AgentId,
    to: AgentId,  // or "broadcast"
    msg_type: MessageType,
    payload: Value,
    timestamp: DateTime<Utc>,
}
```

#### MCP (Model Context Protocol)

Expose CCA as an MCP server for external integration.

```rust
pub struct McpServer {
    tools: HashMap<String, Box<dyn McpTool>>,
    resources: HashMap<String, Box<dyn McpResource>>,
}

// Exposed tools
impl McpServer {
    // Agent management
    fn tool_spawn_agent(&self, role: &str) -> Result<AgentId>;
    fn tool_stop_agent(&self, agent_id: AgentId) -> Result<()>;
    fn tool_list_agents(&self) -> Result<Vec<AgentInfo>>;

    // Task management
    fn tool_send_task(&self, agent: AgentId, task: &str) -> Result<TaskId>;
    fn tool_broadcast(&self, message: &str) -> Result<()>;
    fn tool_get_task_status(&self, task_id: TaskId) -> Result<TaskStatus>;

    // Memory operations
    fn tool_store_pattern(&self, pattern: &str) -> Result<PatternId>;
    fn tool_search_patterns(&self, query: &str) -> Result<Vec<Pattern>>;
    fn tool_get_context(&self, agent: AgentId) -> Result<Context>;
}
```

---

## Database Schema

### PostgreSQL

```sql
-- Extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "vector";

-- Agents table
CREATE TABLE agents (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    role VARCHAR(50) NOT NULL,
    name VARCHAR(100),
    config JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- ReasoningBank: Patterns
CREATE TABLE patterns (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID REFERENCES agents(id),
    pattern_type VARCHAR(50) NOT NULL,
    content TEXT NOT NULL,
    embedding vector(1536),  -- OpenAI ada-002 dimension
    success_count INTEGER DEFAULT 0,
    failure_count INTEGER DEFAULT 0,
    success_rate FLOAT GENERATED ALWAYS AS (
        CASE WHEN success_count + failure_count > 0
        THEN success_count::FLOAT / (success_count + failure_count)
        ELSE 0 END
    ) STORED,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_patterns_embedding ON patterns
    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- Task history
CREATE TABLE tasks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID REFERENCES agents(id),
    description TEXT NOT NULL,
    status VARCHAR(20) DEFAULT 'pending',
    result JSONB,
    tokens_used INTEGER,
    duration_ms INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

-- RL Training data
CREATE TABLE rl_experiences (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    state JSONB NOT NULL,
    action JSONB NOT NULL,
    reward FLOAT NOT NULL,
    next_state JSONB,
    done BOOLEAN DEFAULT FALSE,
    algorithm VARCHAR(50),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Context snapshots (for recovery)
CREATE TABLE context_snapshots (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID REFERENCES agents(id),
    context_hash VARCHAR(64) NOT NULL,
    compressed_context BYTEA,
    token_count INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### Redis Keys

```
# Session state
cca:session:{session_id}           -> JSON session data
cca:agent:{agent_id}:state         -> JSON agent state
cca:agent:{agent_id}:context       -> Compressed context bytes

# Pub/Sub channels
cca:broadcast                      -> Broadcast messages
cca:tasks:{agent_id}               -> Task queue per agent
cca:status                         -> Status updates
cca:coord                          -> Coordination messages

# Rate limiting
cca:ratelimit:{agent_id}           -> Token bucket counter

# Locks
cca:lock:{resource}                -> Distributed locks
```

---

## CLI Interface

### `cca` - Main CLI

```
CCA - Claude Code Agentic v0.1.0

A high-performance multi-agent orchestration system for Claude Code.

USAGE:
    cca <COMMAND>

COMMANDS:
    daemon      Manage the CCA daemon
    agent       Manage agents
    task        Task operations
    memory      Memory operations
    config      Configuration
    status      Show system status

DAEMON:
    cca daemon start              Start the daemon
    cca daemon stop               Stop the daemon
    cca daemon status             Show daemon status
    cca daemon logs               View daemon logs

AGENTS:
    cca agent spawn <role>        Spawn a new agent
    cca agent stop <id>           Stop an agent
    cca agent list                List all agents
    cca agent attach <id>         Attach to agent PTY (manual intervention)
    cca agent send <id> <msg>     Send message to agent
    cca agent logs <id>           View agent logs

TASKS:
    cca task create <desc>        Create a new task
    cca task status <id>          Check task status
    cca task list                 List recent tasks
    cca task cancel <id>          Cancel a task

MEMORY:
    cca memory store <pattern>    Store a pattern
    cca memory search <query>     Search patterns
    cca memory stats              Show memory statistics
    cca memory export             Export patterns to file
    cca memory import <file>      Import patterns from file

CONFIG:
    cca config show               Show current config
    cca config set <key> <value>  Set config value
    cca config init               Initialize config file

EXAMPLES:
    cca daemon start
    cca agent spawn frontend
    cca agent spawn backend
    cca task create "Implement user authentication"
    cca agent attach frontend
    cca memory search "authentication patterns"
```

---

## Configuration

### `cca.toml`

```toml
[daemon]
bind_address = "127.0.0.1:9200"
log_level = "info"
max_agents = 10

[redis]
url = "redis://localhost:6379"
pool_size = 10
context_ttl_seconds = 3600

[postgres]
url = "postgres://cca:cca@localhost:5432/cca"
pool_size = 10
max_connections = 20

[agents]
default_timeout_seconds = 300
context_compression = true
token_budget_per_task = 50000

[agents.roles]
coordinator = { claude_md = "agents/coordinator.md", priority = 1 }
frontend = { claude_md = "agents/frontend.md", priority = 2 }
backend = { claude_md = "agents/backend.md", priority = 2 }
dba = { claude_md = "agents/dba.md", priority = 3 }
devops = { claude_md = "agents/devops.md", priority = 3 }
security = { claude_md = "agents/security.md", priority = 2 }
qa = { claude_md = "agents/qa.md", priority = 3 }

[acp]
websocket_port = 9100
reconnect_interval_ms = 1000
max_reconnect_attempts = 5

[mcp]
enabled = true
bind_address = "127.0.0.1:9201"

[learning]
enabled = true
default_algorithm = "ppo"
training_batch_size = 32
update_interval_seconds = 300

[token_efficiency]
enabled = true
target_reduction = 0.30  # 30% reduction target
compression_algorithm = "context_distillation"
```

---

## Development Plan

### Phase 0: Command Center Plugin MVP (Weeks 1-2) - CRITICAL PATH

**Goal**: Minimal viable Command Center integration

This is the **most important phase** - everything else builds on this foundation.

**Tasks:**

1. MCP Plugin skeleton
   - [ ] Create `cca-mcp` crate with basic MCP server
   - [ ] Implement `cca_task` tool (sends to daemon)
   - [ ] Implement `cca_status` tool
   - [ ] JSON-RPC 2.0 message handling

2. Minimal daemon
   - [ ] Basic daemon that receives tasks from plugin
   - [ ] Single Coordinator agent spawning
   - [ ] Task forwarding to Coordinator
   - [ ] Response collection and return to plugin

3. Integration test
   - [ ] Install plugin in Claude Code
   - [ ] Verify: User prompt → Plugin → Coordinator → Response
   - [ ] End-to-end flow working

**Deliverable**: User can type in CC, task goes to Coordinator, response returns

```text
Week 1-2 Milestone:
  User (CC): "Hello, analyze this codebase"
  → CCA Plugin receives
  → Forwards to Coordinator agent
  → Coordinator responds
  → User sees response in CC
```

### Phase 1: Foundation (Weeks 3-5)

**Goal**: Full daemon with multi-agent spawning and PTY management

**Tasks:**

1. Project setup
   - [ ] Initialize Rust workspace with Cargo
   - [ ] Set up CI/CD (GitHub Actions)
   - [ ] Configure linting (clippy) and formatting (rustfmt)
   - [ ] Set up test infrastructure

2. Core daemon expansion
   - [ ] Implement full `CCADaemon` struct
   - [ ] Process lifecycle management
   - [ ] Signal handling (SIGTERM, SIGINT)
   - [ ] Configuration loading (toml)

3. Agent Manager
   - [ ] PTY creation and management (portable-pty crate)
   - [ ] Multi-agent Claude Code subprocess spawning
   - [ ] Basic send/receive via PTY
   - [ ] Agent state tracking

4. CLI basics (debug only)
   - [ ] `cca daemon start/stop/status`
   - [ ] `cca agent spawn/stop/list`
   - [ ] `cca agent attach` (manual intervention)

**Deliverable**: Coordinator + Execution agents working, CC integration solid

### Phase 2: Communication (Weeks 6-8)

**Goal**: Inter-agent communication via Redis and ACP

**Tasks:**

1. Redis integration
   - [ ] Connection pooling (deadpool-redis)
   - [ ] Session state storage
   - [ ] Context caching
   - [ ] Pub/Sub implementation

2. ACP WebSocket
   - [ ] WebSocket server (tokio-tungstenite)
   - [ ] JSON-RPC 2.0 message handling
   - [ ] Per-agent connections
   - [ ] Reconnection logic

3. Coordinator → Execution Agent routing
   - [ ] Task delegation from Coordinator to specialists
   - [ ] Result aggregation back to Coordinator
   - [ ] Broadcast messages

4. CC Plugin enhancements
   - [ ] `cca_activity` tool - show what agents are doing
   - [ ] Real-time status streaming
   - [ ] Progress indicators for long tasks

**Deliverable**: Full CC → Coordinator → Agents → CC flow

### Phase 3: Persistence (Weeks 9-11)

**Goal**: PostgreSQL integration with vector search

**Tasks:**

1. PostgreSQL setup
   - [ ] Connection pooling (sqlx)
   - [ ] Migration system (sqlx-cli)
   - [ ] Schema implementation

2. ReasoningBank
   - [ ] Pattern storage
   - [ ] pgvector integration
   - [ ] Embedding generation (local or API)
   - [ ] Similarity search

3. Context persistence
   - [ ] Context snapshot storage
   - [ ] Compression (lz4/zstd)
   - [ ] Recovery on restart

4. CC Plugin memory tools
   - [ ] `cca_memory` tool - query patterns from CC
   - [ ] Pattern suggestions during tasks

**Deliverable**: Persistent memory across sessions, accessible from CC

### Phase 4: Learning (Weeks 12-15)

**Goal**: RL engine for optimization

**Tasks:**

1. RL framework
   - [ ] `RLAlgorithm` trait definition
   - [ ] Experience replay buffer
   - [ ] Training loop

2. Algorithm implementations
   - [ ] Q-Learning (tabular)
   - [ ] DQN (neural network)
   - [ ] PPO
   - [ ] MCTS
   - [ ] Additional algorithms (A2C, SAC, TD3, Rainbow, Decision Transformer)

3. Integration with Coordinator
   - [ ] Task routing optimization based on RL
   - [ ] Token budget allocation
   - [ ] Success pattern learning from task outcomes

4. Monitoring
   - [ ] Training metrics exposed via CC plugin
   - [ ] Performance dashboards

**Deliverable**: Adaptive task routing, Coordinator makes smarter decisions

### Phase 5: Token Efficiency (Weeks 16-17)

**Goal**: 30%+ token reduction

**Tasks:**

1. Context analysis
   - [ ] Token counting per agent
   - [ ] Redundancy detection across agents

2. Compression strategies
   - [ ] Context distillation
   - [ ] Summary injection
   - [ ] Selective history pruning
   - [ ] Cross-agent context sharing

3. Monitoring
   - [ ] Token usage tracking in CC
   - [ ] Efficiency metrics and recommendations

**Deliverable**: Measurable token reduction, visible in CC

### Phase 6: Polish & Release (Weeks 18-22)

**Goal**: Production-ready release

**Tasks:**

1. Testing
   - [ ] Integration tests
   - [ ] Load testing
   - [ ] Chaos testing

2. Documentation
   - [ ] User guide
   - [ ] API reference
   - [ ] Architecture docs

3. Packaging
   - [ ] Binary releases
   - [ ] Docker image
   - [ ] Homebrew formula

4. Performance
   - [ ] Profiling
   - [ ] Optimization
   - [ ] Benchmarks

**Deliverable**: v1.0.0 release

---

## Technology Stack

| Component | Technology | Rationale |
| --------- | ---------- | --------- |
| Language | Rust | Performance, safety, no GC |
| Async Runtime | Tokio | Industry standard |
| PTY | portable-pty | Cross-platform |
| WebSocket | tokio-tungstenite | Async WebSocket |
| Redis | deadpool-redis | Connection pooling |
| PostgreSQL | sqlx | Compile-time checked SQL |
| Vector Search | pgvector | Native PostgreSQL |
| Serialization | serde | De facto standard |
| CLI | clap | Full-featured CLI |
| Config | toml + config | Layered configuration |
| Logging | tracing | Structured logging |
| Metrics | prometheus | Observability |

---

## Project Structure

```
cca/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── LICENSE
├── cca.toml.example
├── docker-compose.yml
│
├── crates/
│   ├── cca-daemon/           # Main daemon binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── daemon.rs
│   │       ├── config.rs
│   │       └── ...
│   │
│   ├── cca-cli/              # CLI binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       └── commands/
│   │
│   ├── cca-core/             # Core library
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── agent/
│   │       ├── memory/
│   │       ├── communication/
│   │       └── learning/
│   │
│   ├── cca-acp/              # ACP protocol
│   │   ├── Cargo.toml
│   │   └── src/
│   │
│   ├── cca-mcp/              # MCP server
│   │   ├── Cargo.toml
│   │   └── src/
│   │
│   └── cca-rl/               # RL algorithms
│       ├── Cargo.toml
│       └── src/
│
├── migrations/               # SQL migrations
│   └── ...
│
├── agents/                   # Agent CLAUDE.md files
│   ├── coordinator.md
│   ├── frontend.md
│   ├── backend.md
│   └── ...
│
├── tests/                    # Integration tests
│   └── ...
│
└── docs/                     # Documentation
    ├── architecture.md
    ├── user-guide.md
    └── api-reference.md
```

---

## Success Metrics

| Metric | Target | Measurement |
| ------ | ------ | ----------- |
| Token Efficiency | 30% reduction | Before/after comparison |
| Agent Spawn Time | < 2 seconds | Time to first response |
| Message Latency | < 50ms | Inter-agent P99 |
| Memory Query | < 10ms | Vector search P99 |
| Context Recovery | < 5 seconds | Restart to ready |
| Uptime | 99.9% | Daemon availability |

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
| ---- | ------ | ---------- |
| Claude Code API changes | High | Version pinning, abstraction layer |
| PTY cross-platform issues | Medium | Extensive testing, fallback to tmux |
| RL training instability | Medium | Conservative defaults, human override |
| PostgreSQL scaling | Low | Connection pooling, read replicas |
| Redis memory limits | Low | TTL policies, eviction config |

---

## Timeline Summary

```text
┌─────────────────────────────────────────────────────────────────────────────────┐
│                    CCA Development Timeline (22 Weeks)                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Phase 0: Command Center MVP ████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  W1-2     │
│           (CRITICAL PATH)                                                        │
│                                                                                  │
│  Phase 1: Foundation         ░░░░░░░░████████████░░░░░░░░░░░░░░░░░░░░  W3-5     │
│           Daemon + Agents                                                        │
│                                                                                  │
│  Phase 2: Communication      ░░░░░░░░░░░░░░░░░░░░████████████░░░░░░░░  W6-8     │
│           Redis + ACP                                                            │
│                                                                                  │
│  Phase 3: Persistence        ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░████████  W9-11    │
│           PostgreSQL + pgvector                                                  │
│                                                                                  │
│  Phase 4: Learning           ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░████████░░  W12-15  │
│           RL Algorithms                                                          │
│                                                                                  │
│  Phase 5: Token Efficiency   ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░██  W16-17  │
│           30% reduction                                                          │
│                                                                                  │
│  Phase 6: Polish & Release   ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░████████  W18-22  │
│           v1.0.0                                                                 │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘

Key Milestones:
  Week 2:  CC → Coordinator flow working (MVP)
  Week 5:  Multi-agent spawning operational
  Week 8:  Full agent communication
  Week 11: Persistent memory across sessions
  Week 15: Adaptive task routing via RL
  Week 17: Token efficiency measured
  Week 22: Production release v1.0.0
```

---

## References

- [CCSwarm Analysis](./ccswarm-analysis.md)
- [Claude-Flow Analysis](./claude-flow-analysis.md)
- [ACP Protocol Spec](https://github.com/anthropics/claude-code)
- [MCP Protocol](https://modelcontextprotocol.io/)
- [pgvector](https://github.com/pgvector/pgvector)
- [portable-pty](https://docs.rs/portable-pty/)
