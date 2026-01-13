-- Migration: Add code chunks and indexing jobs tables for codebase indexing
-- This enables semantic search over indexed code functions, classes, and methods

-- Ensure required extensions are enabled
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "vector";

-- Code chunks table: stores indexed code fragments with embeddings
CREATE TABLE IF NOT EXISTS code_chunks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    file_path TEXT NOT NULL,
    chunk_type VARCHAR(50) NOT NULL,  -- 'function', 'class', 'method', 'struct', 'interface', 'trait', 'impl'
    name TEXT NOT NULL,                -- Function/class/method name
    signature TEXT,                    -- Full signature (for functions/methods)
    content TEXT NOT NULL,             -- The actual code
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    language VARCHAR(20) NOT NULL,     -- 'rust', 'python', 'javascript', etc.
    embedding vector(768),             -- nomic-embed-text dimension (via Ollama)
    metadata JSONB DEFAULT '{}',       -- Additional info (docstrings, visibility, etc.)
    indexed_at TIMESTAMPTZ DEFAULT NOW(),

    -- Unique constraint to prevent duplicates (same chunk in same location)
    UNIQUE(file_path, chunk_type, name, start_line)
);

-- Index for vector similarity search
CREATE INDEX IF NOT EXISTS idx_code_chunks_embedding ON code_chunks
    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- Index for file path lookups (for re-indexing)
CREATE INDEX IF NOT EXISTS idx_code_chunks_file_path ON code_chunks(file_path);

-- Index for language filtering
CREATE INDEX IF NOT EXISTS idx_code_chunks_language ON code_chunks(language);

-- Index for chunk type filtering
CREATE INDEX IF NOT EXISTS idx_code_chunks_type ON code_chunks(chunk_type);

-- Indexing jobs table: tracks background indexing operations
CREATE TABLE IF NOT EXISTS indexing_jobs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    path TEXT NOT NULL,
    status VARCHAR(20) DEFAULT 'pending',  -- 'pending', 'running', 'completed', 'failed', 'cancelled'
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

COMMENT ON TABLE code_chunks IS 'Indexed code chunks with embeddings for semantic search';
COMMENT ON TABLE indexing_jobs IS 'Background codebase indexing job tracking';
COMMENT ON COLUMN code_chunks.embedding IS 'nomic-embed-text embedding (768 dimensions via Ollama)';
