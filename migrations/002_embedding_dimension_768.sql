-- Migration: Change embedding dimension from 1536 (OpenAI ada-002) to 768 (nomic-embed-text)
-- This migration is needed when switching from OpenAI embeddings to Ollama's nomic-embed-text model

-- Drop the existing index first (required to alter column type)
DROP INDEX IF EXISTS idx_patterns_embedding;

-- Alter the column to use 768 dimensions
-- Note: This will invalidate any existing embeddings - they will need to be regenerated
ALTER TABLE patterns
    ALTER COLUMN embedding TYPE vector(768);

-- Recreate the index with the new dimension
CREATE INDEX IF NOT EXISTS idx_patterns_embedding ON patterns
    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- Update the comment to reflect the new model
COMMENT ON COLUMN patterns.embedding IS 'nomic-embed-text embedding (768 dimensions via Ollama)';
