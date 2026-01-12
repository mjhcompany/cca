# CCA Architecture

This document provides a comprehensive overview of the CCA system architecture with detailed diagrams.

## High-Level Architecture

```mermaid
graph TB
    subgraph "User Layer"
        CC[Command Center<br/>Claude Code + CCA Plugin]
    end

    subgraph "CCA System"
        MCP[MCP Server<br/>cca-mcp]
        Daemon[CCA Daemon<br/>cca-daemon]
        ACP[ACP WebSocket Server<br/>cca-acp]
        RL[RL Engine<br/>cca-rl]
    end

    subgraph "Agent Layer"
        Coord[Coordinator Agent]
        FE[Frontend Agent]
        BE[Backend Agent]
        Sec[Security Agent]
        QA[QA Agent]
    end

    subgraph "Data Layer"
        Redis[(Redis<br/>Session State)]
        PG[(PostgreSQL<br/>ReasoningBank)]
    end

    CC -->|MCP Tools| MCP
    MCP -->|HTTP API| Daemon
    Daemon -->|Spawn/Manage| Coord
    Daemon -->|Spawn/Manage| FE
    Daemon -->|Spawn/Manage| BE
    Daemon -->|Spawn/Manage| Sec
    Daemon -->|Spawn/Manage| QA

    Coord <-->|ACP WebSocket| ACP
    FE <-->|ACP WebSocket| ACP
    BE <-->|ACP WebSocket| ACP
    Sec <-->|ACP WebSocket| ACP
    QA <-->|ACP WebSocket| ACP

    Daemon -->|Cache/Pub-Sub| Redis
    Daemon -->|Patterns/Tasks| PG

    RL --> Daemon
    Daemon --> RL
```

## Component Architecture

### CCA Daemon (Core Service)

```mermaid
graph TB
    subgraph "CCA Daemon"
        HTTP[HTTP API Server<br/>axum]

        subgraph "Core Components"
            AM[Agent Manager]
            Orch[Orchestrator]
            TS[Token Service]
        end

        subgraph "External Connections"
            ACP[ACP Server]
            RLS[RL Service]
            REDIS[Redis Services]
            PGS[PostgreSQL Services]
        end
    end

    HTTP --> AM
    HTTP --> Orch
    HTTP --> TS
    HTTP --> RLS

    AM -->|PTY Management| Agents[Claude Code Instances]
    Orch -->|Task Routing| AM
    Orch --> ACP
    Orch --> RLS
    Orch --> REDIS

    TS --> REDIS
    RLS --> PGS
```

### Agent Manager

The Agent Manager handles spawning and managing Claude Code instances using PTY (pseudo-terminal).

```mermaid
sequenceDiagram
    participant D as Daemon
    participant AM as Agent Manager
    participant PTY as PTY System
    participant CC as Claude Code

    D->>AM: spawn(role: Backend)
    AM->>PTY: openpty()
    PTY-->>AM: PtyPair
    AM->>CC: spawn_command(claude)
    CC-->>AM: Child Process
    AM->>AM: Create PtyHandle
    AM-->>D: AgentId

    Note over D,CC: Agent Communication

    D->>AM: send(agent_id, message)
    AM->>PTY: write(message)
    PTY->>CC: stdin
    CC->>PTY: stdout
    PTY-->>AM: response
    AM-->>D: response
```

### Orchestrator

The Orchestrator handles task routing, delegation, and result aggregation.

```mermaid
graph TB
    subgraph "Orchestrator"
        TR[Task Router]
        WM[Workload Manager]
        RA[Result Aggregator]

        subgraph "RL Integration"
            RL[RL Service]
            SB[State Builder]
        end
    end

    Task[Incoming Task] --> TR
    TR -->|RL Routing| RL
    RL --> SB
    SB --> TR
    TR -->|Heuristic Fallback| WM
    WM -->|Select Agent| Agent[Agent]
    Agent --> RA
    RA --> Result[Aggregated Result]
```

## Communication Flow

### Task Execution Flow

```mermaid
sequenceDiagram
    participant User
    participant CC as Command Center
    participant MCP as MCP Server
    participant D as Daemon
    participant C as Coordinator
    participant A as Specialist Agent
    participant DB as PostgreSQL

    User->>CC: "Add authentication to API"
    CC->>MCP: cca_task(description)
    MCP->>D: POST /api/v1/tasks
    D->>D: Find/Spawn Coordinator
    D->>C: Send task via PTY
    C->>C: Analyze requirements
    C->>D: Route to Backend agent
    D->>A: Delegate subtask
    A->>A: Execute task
    A-->>D: Task result
    D->>DB: Store pattern
    D-->>MCP: TaskResponse
    MCP-->>CC: Result JSON
    CC-->>User: Display result
```

### ACP WebSocket Communication

```mermaid
sequenceDiagram
    participant A as Agent
    participant ACP as ACP Server
    participant D as Daemon

    Note over A,ACP: Connection Establishment
    A->>ACP: WebSocket Connect
    ACP->>ACP: Generate AgentId
    ACP-->>A: Connection Accepted

    Note over A,ACP: Heartbeat Loop
    loop Every 30s
        A->>ACP: heartbeat(timestamp)
        ACP-->>A: heartbeat_response(server_time)
    end

    Note over D,A: Task Assignment
    D->>ACP: send_to(agent_id, task_assign)
    ACP->>A: task_assign(task_id, description)
    A->>A: Process task
    A->>ACP: task_result(success, output)
    ACP->>D: Forward result

    Note over D,A: Broadcast
    D->>ACP: broadcast(message)
    ACP->>A: notification(message)
```

## Data Architecture

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
        bytes compressed_context
        int ttl_seconds
    }
```

**Redis Key Patterns:**

| Pattern | Purpose |
|---------|---------|
| `cca:session:{id}` | Session data |
| `cca:agent:{id}:state` | Agent state |
| `cca:agent:{id}:context` | Compressed context |
| `cca:broadcast` | Broadcast channel |
| `cca:tasks:{agent_id}` | Task queue per agent |
| `cca:status` | Status updates |
| `cca:coord` | Coordination messages |

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
        vector embedding
        int success_count
        int failure_count
        float success_rate
        jsonb metadata
        timestamp created_at
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

## Reinforcement Learning Architecture

```mermaid
graph TB
    subgraph "RL Engine"
        Algs[Algorithm Registry]
        QL[Q-Learning]
        DQN[DQN]
        PPO[PPO]
        EB[Experience Buffer]
    end

    subgraph "Training Loop"
        Exp[Experience] --> EB
        EB -->|Sample Batch| Train[Training]
        Train --> Algs
        Algs --> Update[Model Update]
    end

    subgraph "Inference"
        State[State] --> Predict[Predict]
        Predict --> Algs
        Algs --> Action[Action]
    end

    Algs --> QL
    Algs --> DQN
    Algs --> PPO
```

### RL State/Action Space

```mermaid
graph LR
    subgraph "State Space"
        TT[Task Type]
        AA[Available Agents]
        TU[Token Usage]
        SH[Success History]
        C[Complexity]
    end

    subgraph "Action Space"
        RA[RouteToAgent<br/>role]
        AT[AllocateTokens<br/>budget]
        UP[UsePattern<br/>pattern_id]
        CC[CompressContext<br/>strategy]
        Comp[Composite<br/>actions]
    end

    TT --> Decision[RL Decision]
    AA --> Decision
    TU --> Decision
    SH --> Decision
    C --> Decision

    Decision --> RA
    Decision --> AT
    Decision --> UP
    Decision --> CC
    Decision --> Comp
```

## Token Efficiency System

```mermaid
graph TB
    subgraph "Token Service"
        AN[Analyzer]
        CT[Counter]
        CP[Compressor]
        MT[Metrics]
    end

    Content[Input Content] --> AN
    AN -->|Token Count| CT
    AN -->|Redundancy| CP

    CP -->|Strategies| S1[Remove Comments]
    CP -->|Strategies| S2[Deduplicate]
    CP -->|Strategies| S3[Summarize]

    S1 --> Compressed[Compressed Output]
    S2 --> Compressed
    S3 --> Compressed

    CT --> MT
    Compressed --> MT
    MT --> Rec[Recommendations]
```

## Deployment Architecture

```mermaid
graph TB
    subgraph "Docker Compose"
        PG[PostgreSQL<br/>port 5433]
        RD[Redis<br/>port 6380]
    end

    subgraph "Host System"
        CCAD[CCA Daemon<br/>port 9200]
        ACPS[ACP Server<br/>port 9100]
        MCPS[MCP Server<br/>stdio]
    end

    subgraph "Claude Code"
        CC[Command Center]
    end

    CC -->|MCP Protocol| MCPS
    MCPS -->|HTTP| CCAD
    CCAD -->|WebSocket| ACPS
    CCAD --> PG
    CCAD --> RD
```

## Security Architecture

```mermaid
graph TB
    subgraph "Authentication"
        AK[API Keys]
        AM[Auth Middleware]
    end

    subgraph "Authorization"
        HB[Health Bypass]
        RP[Route Protection]
    end

    subgraph "Data Security"
        IV[Input Validation]
        SL[Size Limits]
        NS[No Secrets in Logs]
    end

    Request --> AM
    AM -->|Check Header| AK
    AK -->|Valid| RP
    AK -->|/health| HB
    RP --> IV
    IV --> SL
    SL --> Handler[Request Handler]
```

## Module Dependencies

```mermaid
graph BT
    Core[cca-core]
    ACP[cca-acp]
    MCP[cca-mcp]
    RL[cca-rl]
    Daemon[cca-daemon]
    CLI[cca-cli]

    ACP --> Core
    RL --> Core
    MCP --> Core
    Daemon --> Core
    Daemon --> ACP
    Daemon --> RL
    CLI --> Core
```

## Performance Considerations

### Connection Pooling

- **Redis**: Configured via `pool_size` (default: 10)
- **PostgreSQL**: Configured via `max_connections` (default: 20)

### Timeouts

| Component | Default Timeout |
|-----------|----------------|
| Agent task | 300 seconds |
| ACP request | 30 seconds |
| HTTP API | 120 seconds |
| PTY response | 30 seconds |

### Scalability

- **Horizontal**: Multiple daemon instances with shared Redis/PostgreSQL
- **Vertical**: Max agents configurable per daemon (default: 10)
- **Token efficiency**: Target 30% reduction in context size
