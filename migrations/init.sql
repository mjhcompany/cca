-- CCA Database Initialization
-- This file is run on first PostgreSQL startup

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "vector";

-- Agents table
CREATE TABLE IF NOT EXISTS agents (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    role VARCHAR(50) NOT NULL,
    name VARCHAR(100),
    config JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- ReasoningBank: Patterns
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

-- Create index for vector similarity search
CREATE INDEX IF NOT EXISTS idx_patterns_embedding ON patterns
    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- Task history
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

-- RL Training data
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

-- Context snapshots (for recovery)
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

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Triggers for updated_at
CREATE TRIGGER update_agents_updated_at
    BEFORE UPDATE ON agents
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_patterns_updated_at
    BEFORE UPDATE ON patterns
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Grant permissions (for future use with separate users)
-- GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO cca;
-- GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO cca;

-- Insert default coordinator agent
INSERT INTO agents (role, name, config)
VALUES ('coordinator', 'Default Coordinator', '{"priority": 1}')
ON CONFLICT DO NOTHING;

COMMENT ON TABLE agents IS 'CCA agents and their configurations';
COMMENT ON TABLE patterns IS 'ReasoningBank patterns for learned behaviors';
COMMENT ON TABLE tasks IS 'Task execution history';
COMMENT ON TABLE rl_experiences IS 'Reinforcement learning experience replay data';
COMMENT ON TABLE context_snapshots IS 'Agent context snapshots for recovery';
