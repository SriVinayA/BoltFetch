# BoltFetch

> ⚡ A blazing-fast, resilient download manager written in Rust.

BoltFetch is a high-performance, multi-threaded download manager built entirely in Rust. It maximizes available bandwidth using concurrent multipart downloads while intelligently adapting to rate limits, temporary server failures, and network interruptions.

Designed with reliability in mind, BoltFetch can pause, resume, recover from crashes, and continue downloads exactly where they left off.

BoltFetch is available in two editions:

- **CLI** – Lightweight, fast, and script-friendly.
- **Desktop UI** – A native desktop application built with **Tauri v2** and **Leptos**.

> **Note**
>
> BoltFetch started as a personal project because I wanted an Internet Download Manager (IDM)-like experience on macOS, where there wasn't a native solution that met my needs. It is actively developed and used daily, and contributions, bug reports, and feature suggestions are always welcome.

---

## ✨ Features

### 🚀 High-Speed Multi-Threaded Downloads

- Dynamic multipart downloading
- Concurrent worker threads powered by **Tokio**
- Maximizes available network bandwidth
- Automatic chunk scheduling

### 🛡️ Adaptive Download Orchestrator

BoltFetch continuously monitors server behavior and automatically adjusts itself when necessary.

- Detects **HTTP 429 (Too Many Requests)** and **HTTP 503 (Service Unavailable)**
- Dynamically reduces concurrent workers
- Uses exponential backoff
- Automatically resumes downloading when the server becomes available again

### 💾 Persistent Resume Support

Downloads can be safely interrupted at any time.

- Pause and resume downloads
- Recover after crashes or system reboots
- Progress stored in `.boltfetch` state files
- Only unfinished chunks are retried

### 📄 Smart File Detection

- Automatically follows redirects
- Resolves filenames from the `Content-Disposition` header
- Preserves the correct filename and extension

### 🖥️ Dual Interfaces

#### CLI

- Lightweight and fast
- Real-time stacked progress bars
- Script-friendly
- Built with **Clap** and **Indicatif**

#### Desktop UI

- Native desktop application built with **Tauri v2**
- Modern, responsive UI powered by **Leptos**
- **IDM-inspired segment progression bar** (visualizes chunk boundaries and live downloads)
- **Real-time active connection tracking table**
- Premium dark mode with glassmorphic aesthetics

---

# 📦 Installation

## macOS (Homebrew)

### Install the Desktop UI

```bash
brew tap SriVinayA/tap
brew install --cask boltfetch-ui --no-quarantine
```

> `--no-quarantine` bypasses macOS Gatekeeper warnings for unsigned applications.

### Install the CLI

```bash
brew tap SriVinayA/tap
brew install boltfetch
```

---

## ⚠️ macOS Gatekeeper

If macOS displays:

> "BoltFetch is damaged and can't be opened."

remove the quarantine attribute manually:

```bash
xattr -cr /Applications/BoltFetch.app
```

---

# 🏗️ Project Structure

```text
BoltFetch/
├── cli/        # Command-line application
└── ui/         # Tauri + Leptos desktop application
```

---

# 🛠️ Building From Source

## Prerequisites

Install the Rust toolchain first.

For the desktop application you'll also need:

- WebAssembly target
- Trunk
- Tauri CLI

### Install dependencies

```bash
rustup target add wasm32-unknown-unknown

cargo install trunk --locked

cargo install tauri-cli --version "^2.0.0" --locked
```

---

# 🚀 Running the CLI

Navigate into the CLI workspace.

```bash
cd cli
```

Build the release binary:

```bash
cargo build --release
```

Run BoltFetch:

```bash
./target/release/cli \
    --url "https://proof.ovh.net/files/100Mb.dat" \
    --output "100mb.bin" \
    --threads 8
```

## Command-Line Options

| Option | Description |
|---------|-------------|
| `--url` | URL of the file to download |
| `--output` | Output filename |
| `--threads` | Maximum concurrent download threads |

Example:

```bash
./target/release/cli \
    --url "https://proof.ovh.net/files/100Mb.dat" \
    --output "movie.iso" \
    --threads 16
```

---

# 🖥️ Running the Desktop UI

Navigate into the UI workspace.

```bash
cd ui
```

Run in development mode:

```bash
cargo tauri dev
```

Build a production release:

```bash
cargo tauri build
```

This produces native desktop applications for your platform:

- macOS (`.app`)
- Windows (`.exe`)
- Linux packages/binaries

---

# 🧠 How BoltFetch Works

Unlike traditional download managers that use a fixed number of threads, BoltFetch continuously adapts to server conditions.

## 1. Dynamic Multipart Work Stealing

The download begins using the maximum number of worker threads specified by the user.

Unlike static chunking, BoltFetch uses an adaptive work-stealing queue. When a worker thread finishes its chunk (or is idle), it queries the active download state, finds the largest actively downloading chunk, and dynamically splits it in half. The thread immediately begins downloading the new second half. This guarantees that all threads are fully utilized until the very last byte is fetched.

---

## 2. Detecting Rate Limits

If the server responds with errors such as:

- HTTP 429 (Too Many Requests)
- HTTP 503 (Service Unavailable)

BoltFetch recognizes that the current level of concurrency is too aggressive.

---

## 3. Automatic Recovery

Instead of failing the download, BoltFetch:

- Reduces the worker count
- Applies exponential backoff
- Waits for the server to recover
- Automatically resumes downloading

No user intervention is required.

---

## 4. Single-Threaded Fallback for Legacy Servers

Not all servers support advanced features like `Range` headers for multipart downloading. When BoltFetch detects a server that doesn't support chunked downloads, it seamlessly:

- Falls back to a robust single-threaded download mode
- Captures the total file size directly from the full response
- Completes the download using standard streaming

This ensures universal compatibility while still retaining all smart file detection features.

---

## 5. Asynchronous Non-Blocking Disk I/O

To prevent slow disk writing speeds from bottle-necking fast internet connections, BoltFetch uses buffered concurrent writes.
Each thread buffers incoming network data into memory (up to 2MB). Once the threshold is reached, the buffer is handed off to a dedicated background task for writing. By using OS-level positioned writes (`write_at`), multiple threads write to the exact same file simultaneously at different byte offsets without needing file locks.

---

## 6. Persistent Chunk Tracking

Every time a buffer is flushed to disk, the progress is continuously stored inside a `.boltfetch` state file.

If the application:

- crashes
- is closed
- loses power
- or the system reboots

BoltFetch resumes exactly where it left off. Completed chunks are never downloaded again.

---

# 🛠️ Technology Stack

## Backend

- Rust
- Tokio
- Reqwest

## Desktop

- Tauri v2

## Frontend

- Leptos
- WebAssembly

## CLI

- Clap
- Indicatif

---

# 📌 Highlights

- ⚡ High-speed concurrent downloads
- 🧠 Adaptive thread scaling
- 🔄 Automatic resume support
- 💾 Persistent download state
- 🚫 Intelligent rate-limit recovery
- 🛡️ Single-threaded fallback for legacy servers
- 📄 Smart filename detection
- 🖥️ Native desktop application
- 🦀 Written entirely in Rust

---

# 🗺️ Roadmap

Planned features include:

- Download queue management
- Download scheduling
- Browser integration
- Speed limiting
- Authentication support
- Proxy support
- Automatic updates
- Checksum verification
- Download categories
- Plugin architecture

---

# 🤝 Contributing

Contributions are welcome!

If you'd like to improve BoltFetch:

1. Fork the repository.
2. Create a feature branch.
3. Commit your changes.
4. Open a pull request.

Bug reports, feature requests, and discussions are always appreciated.

---

# 📄 License

This project is licensed under the **MIT License**.