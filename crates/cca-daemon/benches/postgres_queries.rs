//! PostgreSQL Query Benchmarks
//!
//! Benchmarks for database query patterns and serialization.
//! Note: These are simulated benchmarks that don't require actual database connections.
//! For real database benchmarks, use the load tests in tests/load/.
//!
//! ## Hot Paths
//! - pgvector serialization/deserialization
//! - Query parameter building
//! - Result mapping and transformation

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;

// ============================================================================
// Simulated Database Components
// ============================================================================

/// Simulated vector for pgvector operations
#[derive(Clone, Debug)]
struct Vector(Vec<f32>);

impl Vector {
    fn new(dimensions: usize) -> Self {
        Self((0..dimensions).map(|i| (i as f32) * 0.01).collect())
    }

    /// Serialize to pgvector binary format (simulated)
    fn to_binary(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(4 + self.0.len() * 4);
        // Dimension count (u16)
        bytes.extend_from_slice(&(self.0.len() as u16).to_le_bytes());
        // Reserved bytes
        bytes.extend_from_slice(&[0u8, 0u8]);
        // Float values
        for val in &self.0 {
            bytes.extend_from_slice(&val.to_le_bytes());
        }
        bytes
    }

    /// Deserialize from pgvector binary format (simulated)
    fn from_binary(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 4 {
            return None;
        }
        let dim = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
        if bytes.len() < 4 + dim * 4 {
            return None;
        }
        let mut values = Vec::with_capacity(dim);
        for i in 0..dim {
            let start = 4 + i * 4;
            let val = f32::from_le_bytes([
                bytes[start],
                bytes[start + 1],
                bytes[start + 2],
                bytes[start + 3],
            ]);
            values.push(val);
        }
        Some(Self(values))
    }

    /// Old string-based format (for comparison)
    fn to_string_format(&self) -> String {
        let values: Vec<String> = self.0.iter().map(|v| format!("{:.6}", v)).collect();
        format!("[{}]", values.join(","))
    }

    /// Parse from string format
    fn from_string_format(s: &str) -> Option<Self> {
        let trimmed = s.trim_start_matches('[').trim_end_matches(']');
        let values: Result<Vec<f32>, _> = trimmed.split(',').map(|s| s.trim().parse()).collect();
        values.ok().map(Self)
    }

    /// Calculate cosine similarity with another vector
    fn cosine_similarity(&self, other: &Self) -> f32 {
        if self.0.len() != other.0.len() {
            return 0.0;
        }

        let dot: f32 = self.0.iter().zip(&other.0).map(|(a, b)| a * b).sum();
        let mag_a: f32 = self.0.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = other.0.iter().map(|x| x * x).sum::<f32>().sqrt();

        if mag_a == 0.0 || mag_b == 0.0 {
            0.0
        } else {
            dot / (mag_a * mag_b)
        }
    }
}

/// Simulated query builder
struct QueryBuilder {
    query: String,
    params: Vec<String>,
}

impl QueryBuilder {
    fn new(base: &str) -> Self {
        Self {
            query: base.to_string(),
            params: Vec::new(),
        }
    }

    fn add_param(&mut self, value: &str) -> &mut Self {
        self.params.push(value.to_string());
        self
    }

    fn add_vector_param(&mut self, vector: &Vector) -> &mut Self {
        self.params.push(vector.to_binary().len().to_string());
        self
    }

    fn build(&self) -> (String, Vec<String>) {
        let mut query = self.query.clone();
        for (i, _) in self.params.iter().enumerate() {
            query = query.replacen("?", &format!("${}", i + 1), 1);
        }
        (query, self.params.clone())
    }
}

/// Simulated query result row
#[derive(Clone)]
#[allow(dead_code)]
struct ResultRow {
    id: String,
    content: String,
    embedding: Vector,
    similarity: f32,
    metadata: HashMap<String, String>,
}

/// Simulated result set processing
struct ResultProcessor;

impl ResultProcessor {
    fn map_to_pattern(row: &ResultRow) -> PatternResult {
        PatternResult {
            id: row.id.clone(),
            content: row.content.clone(),
            score: row.similarity,
        }
    }

    fn rank_results(results: &mut [PatternResult]) {
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    }

    fn filter_by_threshold(results: &[PatternResult], threshold: f32) -> Vec<&PatternResult> {
        results.iter().filter(|r| r.score >= threshold).collect()
    }
}

#[derive(Clone)]
#[allow(dead_code)]
struct PatternResult {
    id: String,
    content: String,
    score: f32,
}

// ============================================================================
// Test Data Generators
// ============================================================================

fn generate_vectors(count: usize, dimensions: usize) -> Vec<Vector> {
    (0..count)
        .map(|i| {
            let mut v = Vector::new(dimensions);
            // Add some variation
            for (j, val) in v.0.iter_mut().enumerate() {
                *val += (i as f32 * 0.001) + (j as f32 * 0.0001);
            }
            v
        })
        .collect()
}

fn generate_result_rows(count: usize, dimensions: usize) -> Vec<ResultRow> {
    (0..count)
        .map(|i| ResultRow {
            id: format!("row_{}", i),
            content: format!("Content for row {} with some sample text", i),
            embedding: Vector::new(dimensions),
            similarity: 0.5 + (i as f32 * 0.001),
            metadata: {
                let mut m = HashMap::new();
                m.insert("type".to_string(), "pattern".to_string());
                m.insert("source".to_string(), format!("agent_{}", i % 10));
                m
            },
        })
        .collect()
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_vector_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("postgres/vector_serialization");

    // Benchmark different vector dimensions (typical embedding sizes)
    for dimensions in [384, 768, 1536, 3072].iter() {
        let vector = Vector::new(*dimensions);

        group.throughput(Throughput::Elements(*dimensions as u64));

        // Binary format (optimized)
        group.bench_with_input(
            BenchmarkId::new("binary", dimensions),
            &vector,
            |b, v| b.iter(|| v.to_binary()),
        );

        // String format (legacy)
        group.bench_with_input(
            BenchmarkId::new("string", dimensions),
            &vector,
            |b, v| b.iter(|| v.to_string_format()),
        );
    }

    group.finish();
}

fn bench_vector_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("postgres/vector_deserialization");

    for dimensions in [384, 768, 1536].iter() {
        let vector = Vector::new(*dimensions);
        let binary = vector.to_binary();
        let string = vector.to_string_format();

        group.throughput(Throughput::Elements(*dimensions as u64));

        group.bench_with_input(
            BenchmarkId::new("binary", dimensions),
            &binary,
            |b, bytes| b.iter(|| Vector::from_binary(black_box(bytes))),
        );

        group.bench_with_input(
            BenchmarkId::new("string", dimensions),
            &string,
            |b, s| b.iter(|| Vector::from_string_format(black_box(s))),
        );
    }

    group.finish();
}

fn bench_similarity_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("postgres/similarity");

    for dimensions in [384, 768, 1536].iter() {
        let v1 = Vector::new(*dimensions);
        let v2 = Vector::new(*dimensions);

        group.throughput(Throughput::Elements(*dimensions as u64));
        group.bench_with_input(
            BenchmarkId::new("cosine", dimensions),
            &(v1.clone(), v2.clone()),
            |b, (a, b_vec)| b.iter(|| a.cosine_similarity(black_box(b_vec))),
        );
    }

    group.finish();
}

fn bench_query_building(c: &mut Criterion) {
    let vector = Vector::new(768);

    c.bench_function("postgres/query_building", |b| {
        b.iter(|| {
            let mut builder = QueryBuilder::new(
                "SELECT id, content, embedding <=> ? AS similarity \
                 FROM patterns WHERE category = ? AND active = ? \
                 ORDER BY embedding <=> ? LIMIT ?",
            );
            builder
                .add_vector_param(black_box(&vector))
                .add_param(black_box("search"))
                .add_param(black_box("true"))
                .add_vector_param(black_box(&vector))
                .add_param(black_box("10"));
            builder.build()
        })
    });
}

fn bench_result_mapping(c: &mut Criterion) {
    let mut group = c.benchmark_group("postgres/result_mapping");

    for count in [10, 50, 100, 500].iter() {
        let rows = generate_result_rows(*count, 768);

        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::new("map", count), &rows, |b, rows| {
            b.iter(|| {
                let results: Vec<PatternResult> =
                    rows.iter().map(ResultProcessor::map_to_pattern).collect();
                results.len()
            })
        });
    }

    group.finish();
}

fn bench_result_ranking(c: &mut Criterion) {
    let rows = generate_result_rows(100, 768);
    let results: Vec<PatternResult> = rows
        .iter()
        .map(ResultProcessor::map_to_pattern)
        .collect();

    c.bench_function("postgres/result_ranking_100", |b| {
        b.iter(|| {
            let mut r = results.clone();
            ResultProcessor::rank_results(&mut r);
        })
    });
}

fn bench_result_filtering(c: &mut Criterion) {
    let rows = generate_result_rows(1000, 768);
    let results: Vec<PatternResult> = rows
        .iter()
        .map(ResultProcessor::map_to_pattern)
        .collect();

    let mut group = c.benchmark_group("postgres/filtering");

    for threshold in [0.5, 0.7, 0.9].iter() {
        group.bench_with_input(
            BenchmarkId::new("threshold", format!("{:.1}", threshold)),
            threshold,
            |b, &threshold| {
                b.iter(|| ResultProcessor::filter_by_threshold(black_box(&results), threshold))
            },
        );
    }

    group.finish();
}

fn bench_batch_vector_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("postgres/batch_vectors");

    for count in [10, 50, 100].iter() {
        let vectors = generate_vectors(*count, 768);

        // Batch serialization
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(
            BenchmarkId::new("serialize", count),
            &vectors,
            |b, vecs| {
                b.iter(|| {
                    let binaries: Vec<Vec<u8>> = vecs.iter().map(|v| v.to_binary()).collect();
                    binaries.len()
                })
            },
        );

        // Batch similarity calculation (compare all pairs)
        if *count <= 50 {
            // Limit due to O(n²)
            group.bench_with_input(
                BenchmarkId::new("pairwise_similarity", count),
                &vectors,
                |b, vecs| {
                    b.iter(|| {
                        let mut similarities = Vec::new();
                        for i in 0..vecs.len() {
                            for j in (i + 1)..vecs.len() {
                                similarities.push(vecs[i].cosine_similarity(&vecs[j]));
                            }
                        }
                        similarities.len()
                    })
                },
            );
        }
    }

    group.finish();
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    benches,
    bench_vector_serialization,
    bench_vector_deserialization,
    bench_similarity_calculation,
    bench_query_building,
    bench_result_mapping,
    bench_result_ranking,
    bench_result_filtering,
    bench_batch_vector_operations,
);

criterion_main!(benches);
