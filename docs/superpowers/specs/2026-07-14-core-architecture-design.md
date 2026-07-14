# Core Architecture Optimization Design

## Objective
Optimize the `boltfetch-core` download engine to maximize throughput, eliminate synchronous I/O blocking in the async runtime, and handle server rate-limiting gracefully.

## Architecture & Components

### 1. State Manager Task (Decoupling Mutex)
Instead of a giant `Arc<Mutex<ActiveState>>` shared across all worker threads, the `DownloadState` will be owned by a single, dedicated asynchronous background task (the State Manager).

- **Communication:** Worker threads will communicate with the State Manager using a `tokio::sync::mpsc` channel.
- **Messages:** Workers will send messages such as `ProgressUpdate { chunk_id, downloaded_bytes }` or `RequestWork`.
- **Throttled Persistence:** The State Manager will aggregate these updates and only persist the `.boltfetch` JSON file to disk at most once every 500ms, removing the massive disk I/O bottleneck.

### 2. Lock-free Asynchronous Disk I/O
Currently, workers use `std::os::unix::fs::FileExt::write_at` directly inside a `tokio::spawn` task. Because this is synchronous, it blocks Tokio's async reactor threads.

- **Solution:** We will wrap all `write_at` calls inside `tokio::task::spawn_blocking`. This offloads the blocking disk I/O to Tokio's dedicated blocking thread pool, allowing the network async tasks to continue fetching the next chunks of data uninterrupted.

### 3. Exponential Backoff for Rate Limiting
Instead of killing a thread instantly when encountering an HTTP 429 (Too Many Requests) or 503 (Service Unavailable):

- **Retry Logic:** The worker will implement a retry loop with exponential backoff (e.g., waiting 1s, then 2s, then 4s, up to a maximum of 5 retries).
- If the server recovers, the worker continues seamlessly. If all retries are exhausted, the worker aborts and records a generic error.

## Data Flow
1. **Worker** receives an HTTP chunk from `reqwest`.
2. **Worker** passes the chunk to `tokio::task::spawn_blocking` to write it to disk via `write_at`.
3. **Worker** sends a `ProgressUpdate` message over the `mpsc` channel to the **State Manager**.
4. **State Manager** updates the in-memory state and, if 500ms have passed since the last save, persists it to `.boltfetch`.
5. **State Manager** calls `progress.emit_progress()` so the UI remains up to date.

## Error Handling
The existing `cancel_flag` will be preserved to allow graceful pausing. Network errors will trigger the exponential backoff, and fatal errors will break the loop and send a failure message to the overarching download coordinator.
