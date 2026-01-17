-- CCA Database Schema
-- Consolidated migration for fresh installations
-- Version: 0.3.0

-- ============================================================================
-- Extensions
-- ============================================================================

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "vector";

-- ============================================================================
-- Agents Table
-- ============================================================================

CREATE TABLE IF NOT EXISTS agents (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    role VARCHAR(50) NOT NULL,
    name VARCHAR(100),
    config JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON TABLE agents IS 'CCA agents and their configurations';

-- ============================================================================
-- ReasoningBank: Patterns Table
-- ============================================================================

CREATE TABLE IF NOT EXISTS patterns (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    pattern_type VARCHAR(50) NOT NULL,
    content TEXT NOT NULL,
    embedding vector(768),  -- nomic-embed-text dimension (via Ollama)
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

CREATE INDEX IF NOT EXISTS idx_patterns_embedding ON patterns
    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

COMMENT ON TABLE patterns IS 'ReasoningBank patterns for learned behaviors';
COMMENT ON COLUMN patterns.embedding IS 'nomic-embed-text embedding (768 dimensions via Ollama)';

-- ============================================================================
-- Task History Table
-- ============================================================================

CREATE TABLE IF NOT EXISTS tasks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    description TEXT NOT NULL,
    status VARCHAR(20) DEFAULT 'pending',
    result JSONB,
    tokens_used INTEGER,
    duration_ms INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_tasks_agent_id ON tasks(agent_id);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_created_at ON tasks(created_at DESC);

COMMENT ON TABLE tasks IS 'Task execution history';

-- ============================================================================
-- RL Training Data Table
-- ============================================================================

CREATE TABLE IF NOT EXISTS rl_experiences (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    state JSONB NOT NULL,
    action JSONB NOT NULL,
    reward FLOAT NOT NULL,
    next_state JSONB,
    done BOOLEAN DEFAULT FALSE,
    algorithm VARCHAR(50),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_rl_experiences_algorithm ON rl_experiences(algorithm);
CREATE INDEX IF NOT EXISTS idx_rl_experiences_created_at ON rl_experiences(created_at DESC);

COMMENT ON TABLE rl_experiences IS 'Reinforcement learning experience replay data';

-- ============================================================================
-- Context Snapshots Table (for recovery)
-- ============================================================================

CREATE TABLE IF NOT EXISTS context_snapshots (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID REFERENCES agents(id) ON DELETE CASCADE,
    context_hash VARCHAR(64) NOT NULL,
    compressed_context BYTEA,
    token_count INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_context_snapshots_agent_id ON context_snapshots(agent_id);
CREATE INDEX IF NOT EXISTS idx_context_snapshots_context_hash ON context_snapshots(context_hash);

COMMENT ON TABLE context_snapshots IS 'Agent context snapshots for recovery';

-- ============================================================================
-- Code Chunks Table (for codebase indexing)
-- ============================================================================

CREATE TABLE IF NOT EXISTS code_chunks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    file_path TEXT NOT NULL,
    chunk_type VARCHAR(50) NOT NULL,  -- 'function', 'class', 'method', 'struct', 'interface', 'trait', 'impl'
    name TEXT NOT NULL,
    signature TEXT,
    content TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    language VARCHAR(20) NOT NULL,
    embedding vector(768),
    metadata JSONB DEFAULT '{}',
    indexed_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(file_path, chunk_type, name, start_line)
);

CREATE INDEX IF NOT EXISTS idx_code_chunks_embedding ON code_chunks
    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);
CREATE INDEX IF NOT EXISTS idx_code_chunks_file_path ON code_chunks(file_path);
CREATE INDEX IF NOT EXISTS idx_code_chunks_language ON code_chunks(language);
CREATE INDEX IF NOT EXISTS idx_code_chunks_type ON code_chunks(chunk_type);

COMMENT ON TABLE code_chunks IS 'Indexed code chunks with embeddings for semantic search';
COMMENT ON COLUMN code_chunks.embedding IS 'nomic-embed-text embedding (768 dimensions via Ollama)';

-- ============================================================================
-- Indexing Jobs Table (for background indexing)
-- ============================================================================

CREATE TABLE IF NOT EXISTS indexing_jobs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    path TEXT NOT NULL,
    status VARCHAR(20) DEFAULT 'pending',
    total_files INTEGER DEFAULT 0,
    processed_files INTEGER DEFAULT 0,
    total_chunks INTEGER DEFAULT 0,
    indexed_chunks INTEGER DEFAULT 0,
    errors JSONB DEFAULT '[]',
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_indexing_jobs_status ON indexing_jobs(status);
CREATE INDEX IF NOT EXISTS idx_indexing_jobs_created_at ON indexing_jobs(created_at DESC);

COMMENT ON TABLE indexing_jobs IS 'Background codebase indexing job tracking';

-- ============================================================================
-- Triggers for updated_at
-- ============================================================================

CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

DROP TRIGGER IF EXISTS update_agents_updated_at ON agents;
CREATE TRIGGER update_agents_updated_at
    BEFORE UPDATE ON agents
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

DROP TRIGGER IF EXISTS update_patterns_updated_at ON patterns;
CREATE TRIGGER update_patterns_updated_at
    BEFORE UPDATE ON patterns
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- Default Data
-- ============================================================================

INSERT INTO agents (role, name, config)
VALUES ('coordinator', 'Default Coordinator', '{"priority": 1}')
ON CONFLICT DO NOTHING;
