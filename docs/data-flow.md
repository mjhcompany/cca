# Data Flow

This document describes how data flows through the CCA system.

## Task Execution Flow

### Complete Task Lifecycle

```mermaid
sequenceDiagram
    participant U as User
    participant CC as Command Center
    participant MCP as MCP Server
    participant D as Daemon API
    participant O as Orchestrator
    participant AM as Agent Manager
    participant C as Coordinator
    participant A as Specialist Agent
    participant RL as RL Engine
    participant R as Redis
    participant PG as PostgreSQL

    Note over U,PG: Task Creation
    U->>CC: "Add authentication"
    CC->>MCP: cca_task(description)
    MCP->>D: POST /api/v1/tasks
    D->>D: Validate input
    D->>D: Create TaskState

    Note over D,C: Task Assignment
    D->>AM: Find Coordinator
    alt No Coordinator
        AM->>AM: Spawn Coordinator
    end
    D->>AM: send(task)
    AM->>C: PTY write

    Note over C,A: Task Delegation
    C->>C: Analyze requirements
    C->>O: Route to specialist
    O->>RL: Get routing prediction
    RL-->>O: Best agent
    O->>AM: Send to specialist
    AM->>A: PTY write

    Note over A,PG: Task Execution
    A->>A: Execute task
    A->>AM: PTY response
    AM->>O: Task result

    Note over O,PG: Result Processing
    O->>O: Update workload stats
    O->>RL: Record experience
    O->>R: Publish result event
    O->>PG: Store pattern (if successful)
    O-->>D: TaskResult

    Note over D,U: Response
    D->>D: Update TaskState
    D-->>MCP: TaskResponse
    MCP-->>CC: JSON result
    CC-->>U: Display result
```

## Agent Communication Flow

### ACP WebSocket Communication

```mermaid
flowchart TB
    subgraph "Daemon"
        D[Daemon]
        O[Orchestrator]
        ACP[ACP Server]
    end

    subgraph "Agents"
        A1[Agent 1]
        A2[Agent 2]
        A3[Agent 3]
    end

    subgraph "Message Types"
        TA[task_assign]
        TR[task_result]
        HB[heartbeat]
        BC[broadcast]
    end

    D -->|Send task| O
    O -->|Route| ACP
    ACP <-->|WebSocket| A1
    ACP <-->|WebSocket| A2
    ACP <-->|WebSocket| A3

    TA -.->|To agent| ACP
    TR -.->|From agent| ACP
    HB -.->|Both ways| ACP
    BC -.->|To all| ACP
```

### Redis Pub/Sub Flow

```mermaid
flowchart LR
    subgraph "Publishers"
        D[Daemon]
        O[Orchestrator]
    end

    subgraph "Redis Channels"
        BC[cca:broadcast]
        ST[cca:status]
        CO[cca:coord]
        TQ[cca:tasks:*]
    end

    subgraph "Subscribers"
        A1[Agent 1]
        A2[Agent 2]
        M[Monitor]
    end

    D -->|Publish| BC
    D -->|Publish| ST
    O -->|Publish| CO
    O -->|Publish| TQ

    BC -->|Subscribe| A1
    BC -->|Subscribe| A2
    BC -->|Subscribe| M
    ST -->|Subscribe| M
    CO -->|Subscribe| A1
    TQ -->|Subscribe| A1
```

## Memory Flow

### ReasoningBank Pattern Flow

```mermaid
flowchart TB
    subgraph "Pattern Creation"
        T[Successful Task]
        E[Extract Pattern]
        V[Generate Embedding]
    end

    subgraph "Storage"
        PG[(PostgreSQL)]
        PV[pgvector]
    end

    subgraph "Pattern Usage"
        Q[Query]
        S[Search]
        R[Results]
    end

    T -->|Success| E
    E -->|Content| V
    V -->|Store| PG
    PG --- PV

    Q -->|Text search| PG
    Q -->|Similarity| PV
    PG -->|Matches| R
    PV -->|Matches| R
```

### Context Caching Flow

```mermaid
flowchart LR
    subgraph "Agent"
        A[Agent Context]
    end

    subgraph "Compression"
        C[Compress LZ4]
        H[Hash Context]
    end

    subgraph "Redis"
        RC[Context Cache]
        RS[State]
    end

    subgraph "PostgreSQL"
        PS[Snapshot]
    end

    A -->|Large context| C
    A -->|Hash| H
    C -->|TTL: 1h| RC
    H -->|State| RS
    C -->|Persist| PS
```

## RL Learning Flow

### Experience Collection

```mermaid
flowchart TB
    subgraph "Task Execution"
        T[Task Start]
        S[Build State]
        P[Predict Action]
        E[Execute]
        R[Result]
    end

    subgraph "Experience"
        EX[Experience]
        EB[Buffer]
    end

    subgraph "Training"
        SA[Sample Batch]
        TR[Train]
        UP[Update Model]
    end

    T --> S
    S --> P
    P --> E
    E --> R
    R -->|Create| EX
    EX -->|Store| EB

    EB -->|Batch size met| SA
    SA --> TR
    TR --> UP
```

### Reward Computation

```mermaid
flowchart LR
    subgraph "Inputs"
        SU[Success/Fail]
        TK[Tokens Used]
        DU[Duration]
    end

    subgraph "Computation"
        BA[Base Reward]
        TB[Token Bonus]
        SB[Speed Bonus]
        TO[Total]
    end

    SU -->|+1.0/-0.5| BA
    TK -->|0.0-0.2| TB
    DU -->|0.0-0.1| SB
    BA --> TO
    TB --> TO
    SB --> TO
```

## Token Efficiency Flow

### Analysis Flow

```mermaid
flowchart TB
    subgraph "Input"
        C[Content]
    end

    subgraph "Analysis"
        CT[Count Tokens]
        DR[Detect Redundancy]
        CB[Find Code Blocks]
        LL[Find Long Lines]
    end

    subgraph "Output"
        R[Report]
    end

    C --> CT
    C --> DR
    C --> CB
    C --> LL

    CT --> R
    DR --> R
    CB --> R
    LL --> R
```

### Compression Flow

```mermaid
flowchart TB
    subgraph "Input"
        C[Original Content]
    end

    subgraph "Strategies"
        RC[Remove Comments]
        DD[Deduplicate]
        SM[Summarize]
    end

    subgraph "Processing"
        AP[Apply Strategies]
        MC[Measure Savings]
    end

    subgraph "Output"
        CC[Compressed Content]
        MT[Metrics]
    end

    C --> RC
    C --> DD
    C --> SM

    RC --> AP
    DD --> AP
    SM --> AP

    AP --> CC
    AP --> MC
    MC --> MT
```

## Data Persistence

### Write Path

```mermaid
flowchart LR
    subgraph "Sources"
        T[Tasks]
        P[Patterns]
        E[Experiences]
        S[Snapshots]
    end

    subgraph "Processing"
        V[Validate]
        SE[Serialize]
    end

    subgraph "Storage"
        PG[(PostgreSQL)]
        R[(Redis)]
    end

    T -->|Persist| V
    P -->|Persist| V
    E -->|Persist| V
    S -->|Cache| SE

    V --> PG
    SE --> R
```

### Read Path

```mermaid
flowchart LR
    subgraph "Request"
        Q[Query]
    end

    subgraph "Cache Check"
        RC{Redis?}
    end

    subgraph "Storage"
        R[(Redis)]
        PG[(PostgreSQL)]
    end

    subgraph "Response"
        RS[Result]
    end

    Q --> RC
    RC -->|Hit| R
    RC -->|Miss| PG
    R --> RS
    PG -->|Cache| R
    PG --> RS
```

## Error Handling Flow

```mermaid
flowchart TB
    subgraph "Error Sources"
        AE[Agent Error]
        NE[Network Error]
        TE[Timeout]
        VE[Validation Error]
    end

    subgraph "Handling"
        CT[Catch Error]
        LG[Log Error]
        RT[Retry Logic]
        FB[Fallback]
    end

    subgraph "Response"
        ER[Error Response]
        RC[Recovery]
    end

    AE --> CT
    NE --> CT
    TE --> CT
    VE --> CT

    CT --> LG
    CT --> RT
    RT -->|Max retries| FB
    RT -->|Success| RC
    FB --> ER
```
