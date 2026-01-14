# CCA Architecture

This document provides a comprehensive overview of the CCA (Claude Code Agentic) system architecture, including system design, component interactions, data flow, technology stack, and deployment architecture.

> **Related Documentation:**
> - [Data Flow](./data-flow.md) - Detailed data flow diagrams
> - [Communication Protocols](./protocols.md) - ACP and MCP protocol specifications
> - [Deployment Guide](./deployment.md) - Setup and deployment instructions
> - [Security Hardening](./security-hardening.md) - Security best practices
> - Component documentation in [docs/components/](./components/)

---

## Table of Contents

1. [System Design Overview](#system-design-overview)
2. [High-Level Architecture](#high-level-architecture)
3. [Core Components](#core-components)
4. [Component Interactions](#component-interactions)
5. [Data Architecture](#data-architecture)
6. [Communication Protocols](#communication-protocols)
7. [Reinforcement Learning System](#reinforcement-learning-system)
8. [Token Efficiency System](#token-efficiency-system)
9. [Technology Stack](#technology-stack)
10. [Deployment Architecture](#deployment-architecture)
11. [Security Architecture](#security-architecture)
12. [Performance & Scalability](#performance--scalability)
13. [Architectural Patterns](#architectural-patterns)

---

## System Design Overview

CCA is a **next-generation multi-agent orchestration system** for Claude Code, designed to manage multiple real Claude Code instances (not simulated agents) through a unified **Command Center architecture**.

### Design Principles

1. **Real Agent Instances**: Each agent is a genuine Claude Code process, not a simulation
2. **Separation of Concerns**: Clear boundaries between components via distinct crates
3. **Async-First**: Built on Tokio for high-performance concurrent operations
4. **Learning System**: Integrated RL for continuous improvement in task routing
5. **Enterprise Storage**: PostgreSQL with pgvector for semantic search capabilities
6. **Real-Time Communication**: WebSocket-based Agent Communication Protocol (ACP)
7. **Security by Default**: Granular permission controls for agent capabilities

### Key Capabilities

| Capability | Description |
|------------|-------------|
| **Multi-Agent Orchestration** | Spawn and manage specialized Claude Code instances |
| **ReasoningBank** | Semantic pattern storage and retrieval with pgvector |
| **RL-Based Routing** | Learn optimal task-to-agent routing over time |
| **Token Efficiency** | Monitor and optimize context token usage |
| **Real-Time Coordination** | WebSocket-based agent communication |

---

## High-Level Architecture

```mermaid
graph TB
    subgraph "User Layer"
        CC[Command Center<br/>Claude Code + CCA Plugin]
    end

    subgraph "Protocol Layer"
        MCP[MCP Server<br/>cca-mcp<br/>JSON-RPC over stdio]
    end

    subgraph "CCA System Core"
        Daemon[CCA Daemon<br/>cca-daemon<br/>HTTP API :9200]
        ACP[ACP Server<br/>cca-acp<br/>WebSocket :9100]
        RL[RL Engine<br/>cca-rl]
        Core[Core Types<br/>cca-core]
    end

    subgraph "Agent Layer"
        Coord[Coordinator Agent<br/>Task Analysis & Routing]
        FE[Frontend Agent<br/>UI/UX Specialist]
        BE[Backend Agent<br/>API/Services Specialist]
        Sec[Security Agent<br/>Security Review]
        QA[QA Agent<br/>Testing Specialist]
    end

    subgraph "Data Layer"
        Redis[(Redis<br/>Session State<br/>Pub/Sub<br/>:6380)]
        PG[(PostgreSQL<br/>pgvector<br/>ReasoningBank<br/>:5433)]
    end

    subgraph "External Services"
        Ollama[Ollama<br/>nomic-embed-text<br/>:11434]
    end

    CC -->|MCP Tools| MCP
    MCP -->|HTTP API| Daemon

    Daemon -->|Spawn/Manage via PTY| Coord
    Daemon -->|Spawn/Manage via PTY| FE
    Daemon -->|Spawn/Manage via PTY| BE
    Daemon -->|Spawn/Manage via PTY| Sec
    Daemon -->|Spawn/Manage via PTY| QA

    Coord <-->|WebSocket| ACP
    FE <-->|WebSocket| ACP
    BE <-->|WebSocket| ACP
    Sec <-->|WebSocket| ACP
    QA <-->|WebSocket| ACP

    Daemon -->|Cache/Pub-Sub| Redis
    Daemon -->|Patterns/Tasks/RL| PG

    Daemon -->|Embeddings| Ollama
    RL --> Daemon
    Core --> Daemon
    Core --> ACP
    Core --> RL
```

### Crate Dependency Graph

```mermaid
graph BT
    Core[cca-core<br/>Core types & traits]
    ACP[cca-acp<br/>WebSocket server/client]
    RL[cca-rl<br/>RL algorithms]
    MCP[cca-mcp<br/>MCP protocol server]
    Daemon[cca-daemon<br/>Main orchestration]
    CLI[cca-cli<br/>Command line tool]

    ACP --> Core
    RL --> Core
    MCP --> Core
    Daemon --> Core
    Daemon --> ACP
    Daemon --> RL
    CLI --> Core

    style Core fill:#e1f5fe
    style Daemon fill:#fff3e0
```

---

## Core Components

### Component Overview

| Crate | Purpose | Key Responsibilities | Documentation |
|-------|---------|---------------------|---------------|
| **cca-core** | Foundation | Types, traits, error handling | [cca-core.md](./components/cca-core.md) |
| **cca-daemon** | Orchestration | HTTP API, agent management, services | [cca-daemon.md](./components/cca-daemon.md) |
| **cca-acp** | Communication | WebSocket server/client, messaging | [cca-acp.md](./components/cca-acp.md) |
| **cca-mcp** | Integration | Claude Code plugin, tool exposure | [cca-mcp.md](./components/cca-mcp.md) |
| **cca-rl** | Learning | RL algorithms, experience management | [cca-rl.md](./components/cca-rl.md) |
| **cca-cli** | CLI | Terminal interface for management | [cca-cli.md](./components/cca-cli.md) |

### CCA Daemon Architecture

The daemon is the central orchestration service coordinating all CCA operations.

```mermaid
graph TB
    subgraph "CCA Daemon (cca-daemon)"
        HTTP[HTTP API Server<br/>Axum + Tower]

        subgraph "Core Services"
            AM[Agent Manager<br/>PTY/Process Control]
            Orch[Orchestrator<br/>Task Routing & Delegation]
            TS[Token Service<br/>Efficiency Optimization]
            ES[Embedding Service<br/>Vector Generation]
            Auth[Auth Middleware<br/>API Key Validation]
        end

        subgraph "External Connections"
            ACPS[ACP Server<br/>WebSocket :9100]
            RLS[RL Service<br/>Learning Engine]
            RDS[Redis Services<br/>State & Pub/Sub]
            PGS[PostgreSQL Services<br/>Persistence]
            OLL[Ollama Client<br/>Embeddings API]
        end
    end

    HTTP --> AM
    HTTP --> Orch
    HTTP --> TS
    HTTP --> Auth

    AM -->|PTY Management| Agents[Claude Code Instances]
    Orch -->|Task Routing| AM
    Orch --> ACPS
    Orch --> RLS
    Orch --> RDS

    TS --> RDS
    RLS --> PGS
    ES -->|nomic-embed-text| OLL
    ES --> PGS
```

### Agent Manager

The Agent Manager handles spawning and lifecycle management of Claude Code instances.

```mermaid
sequenceDiagram
    participant D as Daemon
    participant AM as Agent Manager
    participant PTY as PTY System
    participant CC as Claude Code

    Note over D,CC: Agent Spawning
    D->>AM: spawn(role: Backend)
    AM->>PTY: openpty()
    PTY-->>AM: PtyPair (master/slave)
    AM->>AM: Build command with permissions
    AM->>CC: spawn_command(claude)
    CC-->>AM: Child Process
    AM->>AM: Create PtyHandle
    AM-->>D: AgentId (UUID)

    Note over D,CC: Task Communication
    D->>AM: send(agent_id, task_message)
    AM->>PTY: write to master
    PTY->>CC: stdin
    CC->>PTY: stdout
    PTY-->>AM: response
    AM-->>D: task_result
```

### Orchestrator

The Orchestrator handles intelligent task routing with RL integration.

```mermaid
graph TB
    subgraph "Orchestrator"
        TR[Task Router]
        WM[Workload Manager]
        RA[Result Aggregator]

        subgraph "Routing Strategies"
            RLR[RL-Based Routing<br/>State → Action]
            HEU[Heuristic Routing<br/>Load Balancing]
        end
    end

    Task[Incoming Task] --> TR
    TR -->|Primary| RLR
    RLR -->|Fallback| HEU
    TR --> WM
    WM -->|Select Agent| Agent[Best Agent]
    Agent --> RA
    RA --> Result[Aggregated Result]
```

---

## Component Interactions

### Task Execution Flow

```mermaid
sequenceDiagram
    participant User
    participant CC as Command Center
    participant MCP as MCP Server
    participant D as Daemon API
    participant O as Orchestrator
    participant C as Coordinator
    participant A as Specialist Agent
    participant RL as RL Engine
    participant PG as PostgreSQL

    Note over User,PG: 1. Task Creation
    User->>CC: "Add authentication to API"
    CC->>MCP: cca_task(description, priority)
    MCP->>D: POST /api/v1/tasks

    Note over D,C: 2. Task Assignment
    D->>D: Create TaskState
    D->>O: Find/spawn Coordinator
    O->>C: Send task via PTY

    Note over C,A: 3. Task Delegation
    C->>C: Analyze requirements
    C->>O: Request delegation to Backend
    O->>RL: Build state, get routing prediction
    RL-->>O: Action: RouteToAgent(Backend)
    O->>A: Delegate via ACP WebSocket

    Note over A,PG: 4. Task Execution
    A->>A: Execute task
    A-->>O: Task result via ACP

    Note over O,PG: 5. Result Processing
    O->>RL: Record experience (state, action, reward)
    O->>PG: Store successful pattern
    O-->>D: Aggregated TaskResult

    Note over D,User: 6. Response
    D-->>MCP: TaskResponse JSON
    MCP-->>CC: Result
    CC-->>User: Display result
```

### Agent Communication Pathways

```mermaid
graph TB
    subgraph "Communication Channels"
        HTTP[HTTP REST API<br/>:9200<br/>MCP ↔ Daemon]
        WS[WebSocket ACP<br/>:9100<br/>Daemon ↔ Agents]
        STDIO[PTY/Stdio<br/>Agent Process Control]
        PUBSUB[Redis Pub/Sub<br/>Event Broadcasting]
    end

    subgraph "Use Cases"
        UC1[Task Creation/Status]
        UC2[Real-time Task Assignment]
        UC3[Agent Spawning/Control]
        UC4[System-wide Notifications]
    end

    UC1 --> HTTP
    UC2 --> WS
    UC3 --> STDIO
    UC4 --> PUBSUB
```

---

## Data Architecture

### Storage Strategy

| Data Type | Storage | Rationale |
|-----------|---------|-----------|
| Session State | Redis | Fast access, TTL support |
| Agent Context | Redis (compressed) | Quick retrieval, 1hr TTL |
| Patterns | PostgreSQL + pgvector | Semantic search, persistence |
| Tasks | PostgreSQL | Audit trail, reporting |
| RL Experiences | PostgreSQL | Training data, analysis |
| Broadcasts | Redis Pub/Sub | Real-time, ephemeral |

### Redis Data Model

```mermaid
erDiagram
    SESSION ||--o{ AGENT_STATE : contains
    AGENT_STATE ||--o{ CONTEXT : has

    SESSION {
        string session_id PK
        json data
        timestamp created_at
    }

    AGENT_STATE {
        uuid agent_id PK
        string role
        string state
        uuid current_task
        int tokens_used
        int tasks_completed
        timestamp last_heartbeat
    }

    CONTEXT {
        uuid agent_id FK
        bytes compressed_context "LZ4"
        int ttl_seconds "3600"
    }
```

**Redis Key Patterns:**

| Pattern | Purpose | TTL |
|---------|---------|-----|
| `cca:session:{id}` | Session data | Session lifetime |
| `cca:agent:{id}:state` | Agent state | Heartbeat-based |
| `cca:agent:{id}:context` | Compressed context | 1 hour |
| `cca:broadcast` | Broadcast channel | N/A (pub/sub) |
| `cca:tasks:{agent_id}` | Task queue | Task lifetime |
| `cca:status` | Status updates | N/A (pub/sub) |
| `cca:coord` | Coordination messages | N/A (pub/sub) |

### PostgreSQL Schema

```mermaid
erDiagram
    AGENTS ||--o{ PATTERNS : creates
    AGENTS ||--o{ TASKS : executes
    AGENTS ||--o{ CONTEXT_SNAPSHOTS : has

    AGENTS {
        uuid id PK
        varchar role
        varchar name
        jsonb config
        timestamp created_at
    }

    PATTERNS {
        uuid id PK
        uuid agent_id FK
        varchar pattern_type
        text content
        vector_768 embedding "nomic-embed-text"
        int success_count
        int failure_count
        float success_rate "GENERATED"
        jsonb metadata
        timestamp created_at
        timestamp updated_at
    }

    TASKS {
        uuid id PK
        uuid agent_id FK
        text description
        varchar status
        jsonb result
        int tokens_used
        int duration_ms
        timestamp created_at
    }

    RL_EXPERIENCES {
        uuid id PK
        jsonb state
        jsonb action
        float reward
        jsonb next_state
        boolean done
        varchar algorithm
        timestamp created_at
    }

    CONTEXT_SNAPSHOTS {
        uuid id PK
        uuid agent_id FK
        varchar context_hash
        bytea compressed_context
        int token_count
        timestamp created_at
    }
```

### Semantic Search with Embeddings

```mermaid
sequenceDiagram
    participant Q as Query
    participant D as Daemon
    participant ES as Embedding Service
    participant OL as Ollama
    participant PG as PostgreSQL

    Q->>D: search("authentication patterns")
    D->>ES: embed(query_text)
    ES->>OL: POST /api/embeddings
    OL-->>ES: {embedding: [f32; 768]}
    ES-->>D: query_embedding

    D->>PG: SELECT * FROM patterns<br/>ORDER BY embedding <=> query_embedding<br/>WHERE 1 - (embedding <=> query) >= 0.7<br/>LIMIT 10
    PG-->>D: PatternWithScore[]
    D-->>Q: Ranked results with similarity
```

**Vector Search Configuration:**

| Property | Value |
|----------|-------|
| Model | nomic-embed-text:latest |
| Dimensions | 768 |
| Index Type | IVFFlat |
| Distance Metric | Cosine |
| Index Lists | 100 |

---

## Communication Protocols

### ACP (Agent Communication Protocol)

WebSocket-based real-time communication between daemon and agents.

```mermaid
sequenceDiagram
    participant A as Agent
    participant ACP as ACP Server
    participant D as Daemon

    Note over A,ACP: Connection Establishment
    A->>ACP: WebSocket Connect ws://localhost:9100
    ACP->>ACP: Assign connection ID
    ACP-->>A: Connection Accepted

    Note over A,ACP: Heartbeat Loop (30s)
    loop Every 30 seconds
        A->>ACP: {method: "heartbeat", params: {timestamp}}
        ACP-->>A: {result: {server_time, status: "ok"}}
    end

    Note over D,A: Task Assignment
    D->>ACP: send_to(agent_id, task_assign)
    ACP->>A: {method: "task_assign", params: {task_id, description}}
    A->>A: Execute task
    A->>ACP: {method: "task_result", params: {task_id, success, output}}
    ACP->>D: Forward result

    Note over D,A: Broadcast
    D->>ACP: broadcast(message)
    ACP->>A: {method: "notification", params: {message}}
```

**ACP Message Types:**

| Method | Direction | Purpose |
|--------|-----------|---------|
| `task_assign` | Server → Agent | Assign task to agent |
| `task_result` | Agent → Server | Return task result |
| `heartbeat` | Bidirectional | Keep-alive signal |
| `broadcast` | Server → All | System-wide message |
| `health_check` | Server → Agent | Verify agent status |

### MCP (Model Context Protocol)

JSON-RPC 2.0 over stdio for Claude Code integration.

**Exposed MCP Tools:**

| Tool | Parameters | Description |
|------|------------|-------------|
| `cca_task` | description, priority? | Create and route task |
| `cca_status` | task_id? | Get task/system status |
| `cca_agents` | - | List running agents |
| `cca_activity` | - | Get agent activity |
| `cca_memory` | query, limit? | Search ReasoningBank |
| `cca_broadcast` | message | Broadcast to all agents |
| `cca_acp_status` | - | ACP connection status |
| `cca_workloads` | - | Agent workload info |
| `cca_rl_status` | - | RL engine status |
| `cca_rl_train` | - | Trigger RL training |
| `cca_rl_algorithm` | algorithm | Set RL algorithm |
| `cca_tokens_analyze` | content | Analyze token usage |
| `cca_tokens_compress` | content, strategies? | Compress content |
| `cca_tokens_metrics` | - | Token efficiency metrics |
| `cca_tokens_recommendations` | - | Efficiency recommendations |
| `cca_index_codebase` | path, extensions?, exclude? | Index code for search |
| `cca_search_code` | query, language?, limit? | Semantic code search |

> **See Also:** [API Reference](./api-reference.md) for complete tool documentation.

---

## Reinforcement Learning System

### RL Architecture

```mermaid
graph TB
    subgraph "RL Engine (cca-rl)"
        AR[Algorithm Registry]
        QL[Q-Learning<br/>Tabular values]
        DQN[DQN<br/>Neural approximation]
        PPO[PPO<br/>Policy gradient]
        EB[Experience Buffer<br/>Batch storage]
    end

    subgraph "Training Pipeline"
        Exp[Experience] --> EB
        EB -->|Sample Batch| Train[Training Step]
        Train --> AR
        AR --> Update[Model Update]
    end

    subgraph "Inference Pipeline"
        State[Current State] --> Predict[Predict Action]
        Predict --> AR
        AR --> Action[Selected Action]
    end

    AR --> QL
    AR --> DQN
    AR --> PPO
```

### State/Action Space

```mermaid
graph LR
    subgraph "State Space (Observations)"
        TT[Task Type<br/>classification, code_gen, etc.]
        AA[Available Agents<br/>roles & workloads]
        TU[Token Usage<br/>current consumption]
        SH[Success History<br/>per-agent rates]
        TC[Task Complexity<br/>estimated difficulty]
    end

    subgraph "Action Space (Decisions)"
        RA[RouteToAgent<br/>Select role]
        AT[AllocateTokens<br/>Set budget]
        UP[UsePattern<br/>Retrieve pattern]
        CC[CompressContext<br/>Apply strategy]
        CP[Composite<br/>Multiple actions]
    end

    TT --> Decision[RL Decision Engine]
    AA --> Decision
    TU --> Decision
    SH --> Decision
    TC --> Decision

    Decision --> RA
    Decision --> AT
    Decision --> UP
    Decision --> CC
    Decision --> CP
```

### Reward Computation

```
base_reward = +1.0 if success, -0.5 if failure
token_bonus = (tokens_used < budget) * 0.2
speed_bonus = (completion_time < baseline) * 0.1
total_reward = base_reward + token_bonus + speed_bonus
```

**Supported Algorithms:**

| Algorithm | Type | Best For |
|-----------|------|----------|
| Q-Learning | Tabular | Small state spaces, fast learning |
| DQN | Neural | Larger state spaces, generalization |
| PPO | Policy Gradient | Complex decisions, stability |

> **See Also:** [cca-rl.md](./components/cca-rl.md) for detailed RL documentation.

---

## Token Efficiency System

```mermaid
graph TB
    subgraph "Token Service"
        AN[Analyzer<br/>Usage analysis]
        CT[Counter<br/>BPE estimation]
        CP[Compressor<br/>Reduction strategies]
        MT[Metrics<br/>Tracking & reporting]
    end

    Content[Input Content] --> AN
    AN -->|Token Count| CT
    AN -->|Redundancy Detection| CP

    CP -->|Strategy| S1[code_comments<br/>Remove comments]
    CP -->|Strategy| S2[deduplicate<br/>Remove repeats]
    CP -->|Strategy| S3[summarize<br/>Condense verbose]
    CP -->|Strategy| S4[history<br/>Trim old context]

    S1 --> Compressed[Compressed Output]
    S2 --> Compressed
    S3 --> Compressed
    S4 --> Compressed

    CT --> MT
    Compressed --> MT
    MT --> Rec[Recommendations]
```

**Compression Targets:**

| Metric | Target |
|--------|--------|
| Token Reduction | 30%+ |
| Context Compression | LZ4 |
| Cache TTL | 1 hour |

---

## Technology Stack

### Core Technologies

| Layer | Technology | Purpose |
|-------|------------|---------|
| **Language** | Rust | Performance, safety |
| **Async Runtime** | Tokio | Concurrent task handling |
| **HTTP Framework** | Axum + Tower | REST API and middleware |
| **WebSocket** | Tokio-Tungstenite | Real-time communication |
| **Serialization** | Serde + serde_json | Data (de)serialization |
| **Process Management** | portable-pty | Claude Code PTY control |

### Data Layer

| Component | Technology | Purpose |
|-----------|------------|---------|
| **Database** | PostgreSQL 16 | Primary persistence |
| **Vector Search** | pgvector | Semantic similarity |
| **Cache** | Redis 7 | State, pub/sub |
| **Compression** | LZ4 | Context compression |

### External Services

| Service | Technology | Purpose |
|---------|------------|---------|
| **Embeddings** | Ollama | Local vector generation |
| **Model** | nomic-embed-text | 768-dim embeddings |
| **Metrics** | Prometheus | Application metrics |

### Development Tools

| Tool | Purpose |
|------|---------|
| **tree-sitter** | Code analysis/parsing |
| **sqlx** | Database migrations/queries |
| **tracing** | Structured logging |
| **config** | Configuration management |

---

## Deployment Architecture

### Development Environment

```mermaid
graph TB
    subgraph "Development Host"
        CCAD[ccad<br/>CCA Daemon<br/>:9200]
        ACPS[ACP Server<br/>:9100]
        MCPS[cca-mcp<br/>stdio]
        CC[Claude Code<br/>Command Center]
    end

    subgraph "Docker Compose"
        PG[PostgreSQL + pgvector<br/>:5433]
        RD[Redis<br/>:6380]
        OL[Ollama<br/>:11434]
    end

    CC -->|MCP Protocol| MCPS
    MCPS -->|HTTP| CCAD
    CCAD --> ACPS
    CCAD --> PG
    CCAD --> RD
    CCAD -->|Embeddings| OL
```

### Production Architecture

```mermaid
graph TB
    subgraph "Load Balancer"
        LB[HAProxy / Nginx<br/>TLS Termination]
    end

    subgraph "Application Tier"
        D1[CCA Daemon 1<br/>:9200, :9100]
        D2[CCA Daemon 2<br/>:9200, :9100]
        DN[CCA Daemon N<br/>:9200, :9100]
    end

    subgraph "Data Tier"
        PG[(PostgreSQL<br/>Primary + Replica)]
        RD[(Redis<br/>Cluster / Sentinel)]
        OL[Ollama<br/>Embedding Service]
    end

    subgraph "Monitoring"
        PROM[Prometheus]
        GRAF[Grafana]
    end

    LB --> D1
    LB --> D2
    LB --> DN

    D1 --> PG
    D2 --> PG
    DN --> PG

    D1 --> RD
    D2 --> RD
    DN --> RD

    D1 --> OL
    D2 --> OL
    DN --> OL

    D1 -->|/metrics| PROM
    D2 -->|/metrics| PROM
    DN -->|/metrics| PROM
    PROM --> GRAF
```

### Port Assignments

| Port | Service | Protocol |
|------|---------|----------|
| 9200 | CCA Daemon HTTP API | HTTP/REST |
| 9100 | ACP WebSocket Server | WebSocket |
| 5433 | PostgreSQL (Docker) | TCP |
| 6380 | Redis (Docker) | TCP |
| 11434 | Ollama | HTTP |

### Container Deployment

```yaml
# docker-compose.prod.yml (simplified)
services:
  cca-daemon:
    build: .
    ports:
      - "9200:9200"
      - "9100:9100"
    environment:
      - CCA__REDIS__URL=redis://redis:6379
      - CCA__POSTGRES__URL=postgres://cca:${POSTGRES_PASSWORD}@postgres:5432/cca
      - CCA__DAEMON__REQUIRE_AUTH=true
    depends_on:
      - postgres
      - redis
      - ollama

  postgres:
    image: pgvector/pgvector:pg16
    volumes:
      - postgres_data:/var/lib/postgresql/data

  redis:
    image: redis:7-alpine
    command: redis-server --appendonly yes

  ollama:
    image: ollama/ollama:latest
    volumes:
      - ollama_data:/root/.ollama
```

> **See Also:** [Deployment Guide](./deployment.md) for complete deployment instructions.

---

## Security Architecture

### Permission Model

```mermaid
graph TB
    subgraph "Permission Modes"
        AL[Allowlist Mode<br/>DEFAULT - Recommended]
        SB[Sandbox Mode<br/>Read-only + External Sandbox]
        DG[Dangerous Mode<br/>DEPRECATED - Never Use]
    end

    subgraph "Allowlist Controls"
        AT[--allowedTools<br/>Permitted operations]
        DT[--disallowedTools<br/>Blocked operations]
    end

    AL --> AT
    AL --> DT
```

**Default Security Configuration:**

```toml
[agents.permissions]
mode = "allowlist"
allowed_tools = [
    "Read", "Glob", "Grep",
    "Write(src/**)", "Write(tests/**)",
    "Bash(git *)", "Bash(cargo *)"
]
denied_tools = [
    "Bash(rm -rf *)", "Bash(sudo *)",
    "Read(.env*)", "Write(.env*)"
]
allow_network = false
```

### Authentication Flow

```mermaid
graph TB
    subgraph "Request Flow"
        REQ[Incoming Request]
        AUTH[Auth Middleware]
        HEALTH{Path = /health?}
        KEY{Valid API Key?}
        HANDLER[Route Handler]
        REJECT[401 Unauthorized]
    end

    REQ --> AUTH
    AUTH --> HEALTH
    HEALTH -->|Yes| HANDLER
    HEALTH -->|No| KEY
    KEY -->|Yes| HANDLER
    KEY -->|No| REJECT
```

**Security Layers:**

| Layer | Protection |
|-------|------------|
| **API Keys** | Bearer token authentication |
| **Input Validation** | Size limits, sanitization |
| **Rate Limiting** | Governor-based throttling |
| **Secret Protection** | No secrets in logs |
| **Agent Permissions** | Granular tool control |

> **See Also:** [Security Hardening Guide](./security-hardening.md) for comprehensive security documentation.

---

## Performance & Scalability

### Connection Pooling

| Resource | Default | Max |
|----------|---------|-----|
| Redis Pool | 10 | Configurable |
| PostgreSQL Pool | 20 | 50 |
| Max Agents | 10 | Configurable |

### Timeouts

| Operation | Timeout |
|-----------|---------|
| Agent Task | 300s |
| ACP Request | 30s |
| HTTP API | 120s |
| PTY Response | 30s |
| DB Statement | 30s |
| DB Query | 10s |

### Scalability Patterns

| Pattern | Strategy |
|---------|----------|
| **Horizontal** | Multiple daemons, shared Redis/PostgreSQL |
| **Vertical** | Increase max_agents, pool sizes |
| **Vector Search** | IVFFlat indexing for O(log n) lookups |
| **Token Efficiency** | 30%+ context reduction target |

---

## Architectural Patterns

CCA employs several key architectural patterns:

| Pattern | Application |
|---------|-------------|
| **Command Pattern** | Tasks encapsulate requests with metadata |
| **Observer Pattern** | Redis Pub/Sub for event broadcasting |
| **Strategy Pattern** | Multiple RL algorithms (Q-Learning, DQN, PPO) |
| **Repository Pattern** | Database access layers (patterns, tasks, experiences) |
| **Builder Pattern** | Configuration and client construction |
| **Factory Pattern** | Agent and task creation |
| **Async/Await** | Tokio-based concurrent operations |
| **Circuit Breaker** | Connection retry with backoff |

### Error Handling Strategy

```mermaid
flowchart TB
    subgraph "Error Sources"
        AE[Agent Error]
        NE[Network Error]
        TE[Timeout Error]
        VE[Validation Error]
    end

    subgraph "Handling"
        CT[Catch & Classify]
        LG[Log with Context]
        RT[Retry with Backoff]
        FB[Fallback Action]
    end

    subgraph "Outcomes"
        RC[Recovery Success]
        ER[Error Response]
    end

    AE --> CT
    NE --> CT
    TE --> CT
    VE --> CT

    CT --> LG
    CT --> RT
    RT -->|Success| RC
    RT -->|Max Retries| FB
    FB --> ER
```

---

## Summary

CCA provides a robust, scalable architecture for multi-agent orchestration:

- **Real Claude Code instances** managed via PTY
- **Intelligent task routing** with RL-based optimization
- **Semantic pattern memory** via PostgreSQL + pgvector
- **Real-time communication** via WebSocket ACP
- **Enterprise-grade storage** with Redis and PostgreSQL
- **Security-first design** with granular permission controls
- **Token efficiency** for cost optimization

For detailed component documentation, see the [components/](./components/) directory.
