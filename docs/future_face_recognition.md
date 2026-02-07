# Future: Face Recognition in dupdupninja

This document captures research notes for adding face-based grouping (similar to people grouping in photo apps).

## Goal

Group images by **person identity**, not just visual similarity.

Important distinction:
- `pHash` / `aHash` / `dHash` are image similarity hashes.
- Face grouping should use **face embeddings** (identity vectors) from face-recognition models.

## Recommended Pipeline

1. Detect faces in each image.
2. Align/crop each face.
3. Run face-recognition model to produce an embedding vector (commonly 512-d).
4. Compare vectors using cosine similarity.
5. Cluster embeddings into person groups (for example with DBSCAN/HNSW-assisted nearest-neighbor search).

## Rust/Platform Tooling Options

Likely stack for cross-platform support:

1. ONNX inference
- Crate: `ort` (ONNX Runtime Rust binding)
- Why: runtime provider selection (CPU, plus GPU providers when available)

2. Face detection
- Option A: OpenCV YuNet (`FaceDetectorYN`) via `opencv` crate
- Option B: model-specific Rust wrappers (for example SCRFD wrappers)

3. Face recognition embeddings
- ArcFace-style ONNX models are a common baseline.

4. Large-scale nearest-neighbor search
- Crate: `hnsw_rs` (or similar ANN index)

Note: there is currently no single mature Rust crate that provides full Apple-Photos-like people grouping end-to-end out of the box.

## CPU-Only Cost (Worst Case)

Without GPU, work is moderate-to-high but manageable with a good pipeline.

Typical rough costs:
- Face detection: ~20-200 ms/image (depends on resolution/model/CPU)
- Embedding: ~2-20 ms/face

At large scale (100k+ images), full first-time indexing on CPU can take hours.

## How to Keep CPU Practical

1. Two-stage pipeline
- Run cheap prefilters first (existing metadata/hash signals), then face pipeline only on candidate photos.

2. Resize before detection
- Example: long side 640-1024.

3. Strong caching in `.ddn`
- Store embeddings, detections, model/version, and file fingerprint (`mtime`, size, etc.)
- Recompute only when file or model changes.

4. Incremental scanning
- Process only new/changed files.

5. Controlled parallelism
- Multi-thread with caps to avoid thermal throttling and UI starvation.

6. User-selectable quality modes
- `fast`, `balanced`, `accurate`.

## CPU + GPU Support Strategy

1. Always support CPU provider as baseline.
2. Detect and enable GPU provider at runtime when available.
3. Keep model/output schema consistent across providers.
4. If GPU init fails, silently fall back to CPU.

Result: same features everywhere, speed differs by hardware.

## Storage Impact in `.ddn`

Storage cost is per **detected face** (not per image).

Embedding payload (512-d typical):
- `f32`: ~2.0 KB/face
- `f16`: ~1.0 KB/face
- `i8` quantized: ~0.5 KB/face

Additional metadata per face:
- bbox, landmarks, quality/confidence, cluster/person refs: ~100-400 bytes

SQLite + index overhead:
- often adds roughly 20-80% depending on schema/indexing.

Practical total per face:
- `f32`: ~2.5-4.0 KB
- `f16`: ~1.3-2.5 KB

Example scale:
- 100k images, average 0.4 faces/image => 40k faces
- `f32`: roughly 100-160 MB
- `f16`: roughly 50-100 MB

## Embedding Precision Tradeoffs

### `f32`
Pros:
- Highest numeric fidelity
- Best baseline for threshold calibration
- Lowest risk of accuracy regression

Cons:
- Largest storage/RAM footprint
- Slower memory bandwidth/cache behavior at scale

### `f16`
Pros:
- ~50% of `f32` storage
- Usually very close quality for retrieval/grouping
- Can be faster on compatible hardware

Cons:
- CPU performance/support varies by platform/toolchain
- Some runtimes may upcast internally, reducing speed benefit

### `i8` (quantized)
Pros:
- Smallest storage
- Best cache behavior and potential high-throughput ANN/search

Cons:
- Quantization error can reduce match quality
- Threshold tuning is harder
- More implementation complexity (calibration/scales/versioning)

## Practical Rollout Recommendation

1. Start with `f32` embeddings to establish correctness and thresholds.
2. Move default storage to `f16` once validated on representative datasets.
3. Add optional `i8` mode for very large libraries or space/perf-sensitive users.

## Suggested Future CLI/Config Surface

Potential settings to expose later:
- `face_recognition_enabled`
- `face_model` / `face_model_version`
- `face_provider` (`auto` / `cpu` / `gpu`)
- `face_quality_mode` (`fast` / `balanced` / `accurate`)
- `face_embedding_precision` (`f32` / `f16` / `i8`)
- `face_group_threshold`

## Suggested DB Additions (Future)

Potential tables/fields:
- `faces` table: file_id, bbox, landmarks, confidence, embedding, embedding_type, model_version
- `people` table: stable person cluster ids
- `face_assignments`: mapping faces to person clusters + confidence
- metadata fields for incremental invalidation (file fingerprint + model/provider version)

## Final Notes

- This feature is technically feasible cross-platform.
- Main engineering challenge is robust model/runtime integration and reliable threshold tuning.
- The user experience should emphasize incremental/background indexing and transparent CPU fallback.
