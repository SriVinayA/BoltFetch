# ⚡ BoltFetch

BoltFetch is a blazing-fast, multi-threaded, and highly resilient download manager built completely in Rust. It is designed to maximize your bandwidth through concurrent multipart downloading while intelligently handling hostile servers, rate limits, and network interruptions.

BoltFetch comes in two flavors:

- **CLI Utility** — A lightweight, high-performance command-line downloader.
- **Desktop UI** — A fully native desktop application powered by Tauri v2 and Leptos.

> **Note:** BoltFetch started as a personal project because I wanted an Internet Download Manager (IDM)-like experience on macOS, where there isn't a native equivalent that met my needs. I'm actively developing it for my own daily use, and the project is still a work in progress. Contributions, feedback, and feature suggestions are always welcome.

---

# ✨ Features

- 🚀 **Extreme Concurrency**
  - Splits files into dynamic chunks and downloads them simultaneously using `tokio` and `reqwest` to maximize available bandwidth.

- 🛡️ **Self-Healing Orchestrator**
  - Automatically detects server rate limits (`HTTP 429`, `503`) and network interruptions.
  - Uses dynamic exponential backoff to reduce worker threads.
  - Waits for the server to recover.
  - Automatically resumes downloads without user intervention.

- 💾 **Stateful Download Engine**
  - Progress is stored in `.boltfetch` state files.
  - Pause, quit, or reboot your machine and continue downloading without losing progress.

- 📄 **Smart Filename Resolution**
  - Follows redirects automatically.
  - Parses `Content-Disposition` headers to determine the correct filename and extension.

- 🖥️ **Dual Interfaces**
  - **BoltFetch CLI**
    - Fast terminal experience with real-time stacked progress bars powered by `indicatif`.
  - **BoltFetch UI**
    - Native desktop application built with Tauri v2 and Leptos (WebAssembly).
    - Responsive dark-mode interface.

---

# 🏗️ Project Structure

```text
BoltFetch/
├── cli/        # Command-line application
└── ui/         # Tauri + Leptos desktop application
```

---

# 🚀 Getting Started

## Prerequisites

Install the Rust toolchain first.

For the desktop UI you'll also need:

- WebAssembly target
- Trunk
- Tauri CLI

```bash
# Install WebAssembly target
rustup target add wasm32-unknown-unknown

# Install Trunk
cargo install trunk --locked

# Install Tauri CLI
cargo install tauri-cli --version "^2.0.0" --locked
```

---

# 1. Running the CLI

Navigate into the CLI workspace.

```bash
cd cli

# Build optimized release binary
cargo build --release
```

Run BoltFetch:

```bash
./target/release/cli \
    --url "https://proof.ovh.net/files/100Mb.dat" \
    --output "test.bin" \
    --threads 8
```

### Command Line Options

| Option | Description |
|---------|-------------|
| `--url` | File URL |
| `--output` | Output filename |
| `--threads` | Number of concurrent download threads |

Example:

```bash
./target/release/cli \
    --url "https://proof.ovh.net/files/100Mb.dat" \
    --output "100mb.bin" \
    --threads 8
```

---

# 2. Running the Desktop UI

Navigate into the UI workspace.

```bash
cd ui
```

Run the development application:

```bash
cargo tauri dev
```

Build the production application:

```bash
cargo tauri build
```

This produces native desktop applications for your platform (e.g. macOS `.app`, Windows `.exe`, Linux binaries/packages).

---

# 🧠 How the Orchestrator Works

BoltFetch doesn't simply spawn a fixed number of threads. Instead, it continuously adapts to server behavior.

### 1. Initial Connection

The downloader begins with the user-defined maximum thread count.

Example:

```
Threads: 8
```

---

### 2. Rate Limit Detection

If the server responds with errors such as:

- `429 Too Many Requests`
- `503 Service Unavailable`

BoltFetch immediately detects that the current level of parallelism is too aggressive.

---

### 3. Dynamic Fallback

Instead of failing the download, BoltFetch:

- Counts successful connections.
- Shrinks the worker pool.
- Uses exponential backoff.
- Waits briefly.
- Restarts automatically.

---

### 4. Stateful Chunk Processing

Each completed chunk is safely written to disk.

Progress is continuously saved inside the `.boltfetch` state file.

If the application crashes or is closed:

- completed chunks remain valid
- unfinished chunks are retried
- downloading resumes exactly where it stopped

---

### 5. Automatic Completion

Whenever a worker finishes downloading a chunk, it immediately requests the next pending chunk until every byte of the file has been downloaded.

---

# 🛠️ Built With

## Backend

- Rust
- Tokio
- Reqwest

## Desktop Framework

- Tauri v2

## Frontend

- Leptos v0.7
- WebAssembly

## CLI

- Clap
- Indicatif

---

# 📌 Highlights

- ⚡ Multi-threaded downloads
- 🔄 Automatic resume support
- 💾 Persistent download state
- 🧠 Adaptive thread scaling
- 🚫 Automatic rate-limit recovery
- 📄 Smart filename detection
- 🖥️ Native desktop application
- 🦀 Written entirely in Rust

---

# 📄 License

This project is licensed under the MIT License.