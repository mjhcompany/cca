# CCA System Diagnostic Report

**Date:** 2026-01-13
**Version:** 0.3.0

## Executive Summary

Task delegation **WORKS** when the coordinator is connected. However, several critical subsystems are **NOT FUNCTIONAL** because the daemon's task execution path **bypasses the orchestrator's instrumentation entirely**.

### Working Features
| Feature | Status | Evidence |
|---------|--------|----------|
| Task Delegation | **WORKING** | tasks_completed=1 after test |
| ACP WebSocket | **WORKING** | 7 agents connected |
| Coordinator Routing | **WORKING** | Delegated to backend specialist |

### Broken Features
| Feature | Status | Root Cause |
|---------|--------|------------|
| RL Experience Collection | **BROKEN** | `orchestrator.process_result()` never called |
| Memory/ReasoningBank | **BROKEN** | `postgres.patterns.create()` never called |
| Token Tracking | **BROKEN** | No token data in WebSocket responses |
| Activity Tracking | **BROKEN** | Redis state not updated during tasks |
| Workload Distribution | **BROKEN** | ACP workers not registered in orchestrator |

---

## Diagnostic Results

### System Status
```json
{
  "status": "running",
  "version": "0.3.0",
  "agents_count": 7,
  "tasks_pending": 0,
  "tasks_completed": 1
}
```

### Connected Agents
| Role | Status |
|------|--------|
| coordinator | connected |
| backend | connected |
| frontend | connected |
| dba | connected |
| security | connected |
| qa | connected |
| devops | connected |

### RL Status (All Zeros)
```json
{
  "algorithm": "q_learning",
  "total_steps": 0,
  "experience_count": 0,
  "buffer_size": 0,
  "total_rewards": 0.0
}
```

### Token Metrics (All Zeros)
```json
{
  "total_tokens_used": 0,
  "total_tokens_saved": 0,
  "efficiency_percent": 0.0,
  "agent_count": 0
}
```

### Memory Search (Empty)
```json
{
  "patterns": []
}
```

### Workloads (Empty Agents)
```json
{
  "agents": [],
  "total_tasks": 5,
  "pending_tasks": 0
}
```

---

## Root Cause Analysis

### The Core Problem: Two Parallel Execution Paths

The codebase has **two parallel task execution paths** that don't communicate:

#### Path A: Orchestrator (Fully Instrumented but NOT USED)
```
orchestrator.route_task()
  → Updates agent_workloads ✓
  → orchestrator.process_result()
    → Records RL experience ✓
    → Updates workload stats ✓
    → Computes rewards ✓
```

#### Path B: Daemon Direct Execution (USED but NOT INSTRUMENTED)
```
create_task()
  → execute_delegations()
    → acp_server.send_task() (WebSocket)
    → Results collected but NEVER processed
    → ❌ No RL recording
    → ❌ No pattern storage
    → ❌ No token tracking
    → ❌ No Redis state updates
    → ❌ No orchestrator workload updates
```

---

## Detailed Gap Analysis

### 1. RL Experience Collection

**Location:** `crates/cca-daemon/src/daemon.rs`

**Gap:** After `execute_delegations()` returns results (line ~1493), the code never calls `orchestrator.process_result()` to record RL experiences.

**Infrastructure:** The `orchestrator.rs` has full implementation at lines 457-510:
```rust
// In orchestrator.process_result()
let experience = Experience {
    state: state.clone(),
    action: action_index,
    reward,
    next_state,
    done: true,
};
rl_service.record_experience(experience).await;
```

**Missing Call:** This method is never invoked from the daemon's task flow.

---

### 2. Memory/ReasoningBank

**Location:** `crates/cca-daemon/src/postgres.rs` lines 319-369

**Gap:** `PatternRepository::create()` exists but is never called after task completion.

**What Should Happen:**
- Successful task outputs should be stored as patterns
- Patterns need embeddings for semantic search (pgvector)
- Metadata should include role, task description, duration

**Missing Integration:** No code path calls pattern storage after `execute_delegations()`.

---

### 3. Token Tracking

**Location:** `crates/cca-daemon/src/tokens.rs` lines 481-527

**Multiple Gaps:**

1. **WebSocket Response Missing Token Data**
   - `acp_server.send_task()` returns only `String` output
   - No token count in `DelegateTaskResponse`

2. **Claude Code Workers Don't Report Tokens**
   - Worker agents execute tasks via Claude API
   - Token usage from API response is not captured/reported

3. **TokenService Never Called**
   - `state.token_service.track_usage()` exists but never invoked

---

### 4. Activity Tracking

**Location:** `crates/cca-daemon/src/redis.rs` lines 242-250

**Gap:** `update_agent_redis_state()` is only called when spawning agents (line 806), not during task execution.

**Missing Calls:**
- Before task: Set `current_task`
- After task: Clear `current_task`, increment counters
- Include token usage

---

### 5. Workload Distribution

**Location:** `crates/cca-daemon/src/orchestrator.rs` lines 741-744

**Gap:** ACP workers are never registered in the orchestrator.

**Problem:**
- `get_workloads()` reads from `orchestrator.get_agent_workloads()`
- ACP-connected agents are never registered via `orchestrator.register_agent()`
- Orchestrator's workload tracking is empty because daemon bypasses it

---

## Fix Work Plan

### Phase 1: Bridge Daemon to Orchestrator (Critical)

**File:** `crates/cca-daemon/src/daemon.rs`

**Task 1.1:** Add orchestrator result processing after `execute_delegations()`

Insert after line ~1493 in `create_task()`:
```rust
// Record results through orchestrator for instrumentation
for (delegation, result) in coord_response.delegations.iter().zip(&delegation_results) {
    let task_result = TaskResult {
        task_id: TaskId::new(),
        success: result.success,
        output: result.output.clone().unwrap_or_default(),
        tokens_used: result.tokens_used.unwrap_or(0),
        duration_ms: result.duration_ms,
        error: result.error.clone(),
        metadata: serde_json::Value::Null,
    };
    let _ = state.orchestrator.write().await
        .process_result(task_result).await;
}
```

**Task 1.2:** Register ACP workers in orchestrator

When ACP workers connect, add registration:
```rust
state.orchestrator.write().await.register_agent(
    agent_id,
    role.clone(),
    vec![], // capabilities
    10,     // max_tasks
).await;
```

---

### Phase 2: Token Tracking (High Priority)

**Task 2.1:** Update `DelegateTaskResponse` struct

**File:** `crates/cca-daemon/src/daemon.rs`
```rust
struct DelegateTaskResponse {
    success: bool,
    output: Option<String>,
    tokens_used: Option<u64>,  // ADD THIS
    duration_ms: u64,
    error: Option<String>,
}
```

**Task 2.2:** Extract token usage from worker responses

**File:** `crates/cca-acp/src/server.rs`

Modify worker task handling to include token metadata in responses.

**Task 2.3:** Call TokenService after task completion

**File:** `crates/cca-daemon/src/daemon.rs` in `execute_delegations()`
```rust
if let Some(tokens) = result.tokens_used {
    state.token_service.track_usage(&agent_id, tokens).await;
}
```

---

### Phase 3: Activity Tracking (Medium Priority)

**Task 3.1:** Update Redis state during task execution

**File:** `crates/cca-daemon/src/daemon.rs`

Before sending task (~line 1876):
```rust
update_agent_redis_state(
    &state.redis,
    agent_id,
    &delegation.role,
    Some(&delegation.task),
).await;
```

After task completion (~line 1900):
```rust
update_agent_redis_state(
    &state.redis,
    agent_id,
    &delegation.role,
    None,
).await;
```

---

### Phase 4: Memory/Pattern Storage (Medium Priority)

**Task 4.1:** Store patterns after successful tasks

**File:** `crates/cca-daemon/src/daemon.rs`

After successful task in `execute_delegations()`:
```rust
if result.success && state.postgres.is_some() {
    let postgres = state.postgres.as_ref().unwrap();
    let metadata = serde_json::json!({
        "role": delegation.role,
        "task": delegation.task,
        "duration_ms": result.duration_ms
    });

    let _ = postgres.patterns.create(
        Some(agent_id),
        PatternType::Solution,
        result.output.as_ref().unwrap(),
        None, // Embedding - implement later
        metadata
    ).await;
}
```

**Task 4.2:** Add embedding generation for semantic search

Integrate with an embedding service to generate vectors for pgvector search.

---

### Phase 5: Worker Token Reporting (Complex)

**Task 5.1:** Modify Claude Code worker prompt

Workers need to extract and report token usage from Claude API responses.

**Task 5.2:** Update ACP message protocol

Add token_used field to task completion messages.

**Task 5.3:** Parse token data in ACP server

Extract token counts when receiving worker responses.

---

## Priority Matrix

| Fix | Impact | Effort | Priority |
|-----|--------|--------|----------|
| Bridge daemon to orchestrator | High | Medium | **P0** |
| Register ACP workers in orchestrator | High | Low | **P0** |
| Update DelegateTaskResponse with tokens | Medium | Low | **P1** |
| Call TokenService after tasks | Medium | Low | **P1** |
| Update Redis activity during tasks | Medium | Low | **P1** |
| Store patterns after tasks | Medium | Medium | **P2** |
| Worker token extraction | High | High | **P2** |
| Embedding generation | Medium | High | **P3** |

---

## Key Files to Modify

1. **`crates/cca-daemon/src/daemon.rs`** - Primary changes
   - Lines 1319-1640: `create_task()`
   - Lines 1752-1926: `execute_delegations()`

2. **`crates/cca-daemon/src/orchestrator.rs`** - Reference implementation
   - Lines 399-547: `process_result()` (working instrumentation)

3. **`crates/cca-acp/src/server.rs`** - Token metadata in WebSocket
   - Worker message handling

4. **`crates/cca-daemon/src/redis.rs`** - Activity tracking
   - `update_agent_redis_state()` calls

---

## Verification Steps

After implementing fixes, verify with:

1. Submit a test task: `cca_task`
2. Check RL status: `cca_rl_status` - should show experience_count > 0
3. Check tokens: `cca_tokens_metrics` - should show usage
4. Check memory: `cca_memory` - should return patterns
5. Check activity: `cca_activity` - should show agent work
6. Check workloads: `cca_workloads` - agents array should have data

---

## Conclusion

The CCA system has **solid infrastructure** for RL learning, memory storage, and token tracking, but the **daemon's task execution bypasses all of it**. The fix is to bridge the daemon's direct WebSocket execution path to the orchestrator's instrumentation layer.

The most critical fixes are:
1. Call `orchestrator.process_result()` after task completion
2. Register ACP workers in the orchestrator
3. Add token tracking to the task completion flow

These three changes will restore most functionality without major architectural changes.
