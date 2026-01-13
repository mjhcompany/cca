# Security Hardening Guide

This guide covers security best practices for CCA (Claude Code Agentic) deployments, with a focus on agent permission controls and the risks associated with disabled security features.

## Agent Permission System (SEC-007)

CCA controls how Claude Code agents execute operations through a configurable permission system. This replaces the legacy `--dangerously-skip-permissions` flag with granular, auditable controls.

### Permission Modes

CCA supports three permission modes, configured via `[agents.permissions].mode`:

| Mode | Security Level | Use Case | Recommendation |
|------|---------------|----------|----------------|
| `allowlist` | **High** | Production, development | **RECOMMENDED** |
| `sandbox` | **Medium** | Containerized environments with external sandbox | Acceptable with external controls |
| `dangerous` | **None** | Legacy compatibility only | **NEVER USE IN PRODUCTION** |

### Understanding `--dangerously-skip-permissions`

When `mode = "dangerous"` is configured, CCA passes the `--dangerously-skip-permissions` flag to Claude Code. This flag:

1. **Disables ALL built-in permission checks** in Claude Code
2. **Allows unrestricted file system access** - agents can read/write any file
3. **Allows unrestricted command execution** - agents can run any shell command
4. **Bypasses safety prompts** - no user confirmation for dangerous operations

#### Security Implications

| Risk | Description | Potential Impact |
|------|-------------|------------------|
| **Data Exfiltration** | Agent can read sensitive files (.env, credentials, secrets) | Credential theft, data breach |
| **System Compromise** | Agent can execute arbitrary commands (sudo, rm -rf, etc.) | Full system takeover |
| **Lateral Movement** | Agent can modify SSH keys, create backdoors | Network-wide compromise |
| **Supply Chain Attack** | Agent can modify source code, inject malicious dependencies | Code integrity compromise |
| **Privilege Escalation** | Agent can modify system files, user permissions | Root access acquisition |

#### When Dangerous Mode Might Be Acceptable

The `dangerous` mode should **ONLY** be considered when **ALL** of the following conditions are met:

1. The agent runs in a **fully isolated container/VM** with no access to host resources
2. The container has **no network access** or strictly controlled egress
3. The container has **no volume mounts** to sensitive host paths
4. The container runs with **minimal capabilities** (no privileged mode)
5. Container state is **ephemeral** and discarded after each task
6. **External monitoring** is in place to detect anomalous behavior

Even then, using `allowlist` or `sandbox` mode is strongly preferred.

### Recommended Configuration: Allowlist Mode

The `allowlist` mode provides granular control over agent capabilities:

```toml
[agents.permissions]
mode = "allowlist"  # RECOMMENDED

# Tools allowed without prompting
allowed_tools = [
    "Read",                    # File reading (generally safe)
    "Glob",                    # File pattern matching
    "Grep",                    # Content search
    "Write(src/**)",           # Write to source files only
    "Write(tests/**)",         # Write to test files
    "Write(docs/**)",          # Write to documentation
    "Bash(git status)",        # Safe git operations
    "Bash(git diff*)",
    "Bash(git log*)",
    "Bash(cargo build)",       # Safe build commands
    "Bash(npm test)",
]

# Tools explicitly blocked (overrides allowed_tools)
denied_tools = [
    "Bash(rm -rf *)",          # Destructive deletion
    "Bash(rm -r *)",           # Recursive deletion
    "Bash(sudo *)",            # Privilege escalation
    "Bash(su *)",              # User switching
    "Read(.env*)",             # Sensitive files
    "Write(.env*)",
    "Read(*credentials*)",     # Credential files
    "Write(*credentials*)",
    "Read(*secret*)",          # Secret files
    "Write(*secret*)",
    "Bash(chmod 777 *)",       # Insecure permissions
    "Bash(chown *)",           # Ownership changes
]

# Block network access by default
allow_network = false  # Blocks curl, wget, nc, netcat

# Optional: restrict to specific directory
working_dir = "/path/to/project"
```

### Sandbox Mode

The `sandbox` mode is for environments where external sandboxing (containers, VMs) provides the security boundary:

```toml
[agents.permissions]
mode = "sandbox"
allowed_tools = ["Read", "Glob", "Grep"]  # Minimal read-only access
denied_tools = []  # External sandbox handles restrictions
```

In this mode:
- Agent can only read files (no writes, no command execution)
- External sandbox (container/VM) provides additional isolation
- Defense-in-depth: CCA restrictions + external sandbox

### Role-Based Permission Overrides

Different agent roles can have different permission levels:

```toml
# Coordinator gets minimal read-only access
[agents.permissions.role_overrides.coordinator]
mode = "sandbox"
allowed_tools = ["Read", "Glob", "Grep"]

# Backend developer gets more capabilities
[agents.permissions.role_overrides.backend]
mode = "allowlist"
allowed_tools = ["Read", "Glob", "Grep", "Write(src/**)", "Bash(cargo *)"]
denied_tools = ["Bash(cargo publish)"]  # Block publishing

# DBA gets database-specific access
[agents.permissions.role_overrides.dba]
mode = "allowlist"
allowed_tools = ["Read", "Glob", "Grep", "Bash(psql *)"]
denied_tools = ["Bash(psql * DROP *)"]
```

### Environment Variable Configuration

All permission settings can be configured via environment variables:

```bash
# Permission mode
export CCA__AGENTS__PERMISSIONS__MODE="allowlist"

# Allowed tools (comma or semicolon separated)
export CCA__AGENTS__PERMISSIONS__ALLOWED_TOOLS="Read,Glob,Grep,Write(src/**)"

# Denied tools
export CCA__AGENTS__PERMISSIONS__DENIED_TOOLS="Bash(rm -rf *);Bash(sudo *)"

# Network access
export CCA__AGENTS__PERMISSIONS__ALLOW_NETWORK="false"

# Working directory restriction
export CCA__AGENTS__PERMISSIONS__WORKING_DIR="/app/workspace"
```

### Tool Pattern Syntax

Tools can be specified using patterns:

| Pattern | Description | Example Match |
|---------|-------------|---------------|
| `Read` | All file reads | Any file |
| `Write(src/**)` | Writes under src/ | src/main.rs, src/lib/util.rs |
| `Bash(git *)` | Git commands | git status, git diff |
| `Bash(npm test)` | Exact command | npm test only |
| `Read(.env*)` | Env files | .env, .env.local |

## Migration from Legacy Configuration

If you were previously using `--dangerously-skip-permissions`, migrate as follows:

### Before (Insecure)

```bash
# Old approach - DO NOT USE
claude --dangerously-skip-permissions
```

### After (Secure)

```toml
# New approach - granular control
[agents.permissions]
mode = "allowlist"
allowed_tools = ["Read", "Glob", "Grep", "Write(src/**)", "Bash(git *)"]
denied_tools = ["Bash(rm -rf *)", "Bash(sudo *)"]
allow_network = false
```

## Monitoring and Auditing

### Log Analysis

CCA logs permission-related events. Monitor for:

```bash
# Warning when dangerous mode is used
grep "dangerously-skip-permissions" /var/log/cca/daemon.log

# Permission denials
grep "Permission denied" /var/log/cca/daemon.log
```

### Security Checklist

Before deploying CCA, verify:

- [ ] `mode` is set to `allowlist` or `sandbox` (NOT `dangerous`)
- [ ] `allowed_tools` is explicitly configured with minimal necessary tools
- [ ] `denied_tools` blocks sensitive operations (sudo, rm -rf, etc.)
- [ ] `allow_network` is `false` unless explicitly required
- [ ] `working_dir` is set to restrict file system access
- [ ] Role overrides give each role only necessary permissions
- [ ] Authentication is enabled (`require_auth = true`)
- [ ] API keys are set via environment variables (not in config files)

## Container Security for Sandbox Mode

If using `sandbox` mode with external containers, ensure:

### Docker Security

```dockerfile
# Use non-root user
USER appuser

# Read-only root filesystem
docker run --read-only ...

# Drop all capabilities
docker run --cap-drop ALL ...

# No network access
docker run --network none ...

# Minimal volume mounts (no sensitive paths)
docker run -v /project/src:/workspace:ro ...
```

### Kubernetes Security

```yaml
apiVersion: v1
kind: Pod
spec:
  securityContext:
    runAsNonRoot: true
    readOnlyRootFilesystem: true
    allowPrivilegeEscalation: false
    capabilities:
      drop:
        - ALL
  containers:
  - name: cca-agent
    securityContext:
      runAsUser: 1000
```

## Incident Response

If an agent is suspected of malicious behavior:

1. **Immediately stop** the agent: `cca agent stop <agent-id>`
2. **Review logs** for suspicious commands: Check daemon logs
3. **Audit file changes**: Review git diff, file modification times
4. **Check for persistence**: Look for new cron jobs, SSH keys, startup scripts
5. **Rotate credentials**: Change any credentials the agent may have accessed
6. **Report**: File an issue if you believe there's a vulnerability

## Summary

| Configuration | Security | Performance | Recommended For |
|--------------|----------|-------------|-----------------|
| `allowlist` with strict rules | Highest | Normal | Production |
| `sandbox` + containers | High | Normal | Isolated environments |
| `allowlist` with relaxed rules | Medium | Normal | Development |
| `dangerous` | **None** | Fastest | **Never** |

Always prefer the most restrictive configuration that allows your agents to function. Security and functionality can coexist with proper configuration.
