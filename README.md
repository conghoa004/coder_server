# 🚀 Kaggle Code Server with Cloudflare Tunnel

[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20Kaggle%20%7C%20Docker-blue?logo=linux)](https://github.com/conghoa004/coder_server)
[![Python](https://img.shields.io/badge/Python-3.9%2B-brightgreen?logo=python)](https://python.org)
[![code-server](https://img.shields.io/badge/VS%20Code-code--server-blueviolet?logo=visualstudiocode)](https://github.com/coder/code-server)
[![Cloudflare Tunnel](https://img.shields.io/badge/Cloudflare-Tunnel-orange?logo=cloudflare)](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/)
[![Port](https://img.shields.io/badge/Port-8001-success)](#configuration--options)

A production-ready, automated toolkit to run a full browser-based **VS Code (`code-server`)** instance in headless Linux environments (such as **Kaggle Notebooks**, Google Colab, Docker containers, or remote Linux VMs) and expose it securely to the internet via **Cloudflare Tunnel** without port-forwarding.

---

## ✨ Core Features

- ⚡ **Zero-Config Installation**: Automatically downloads and installs the official standalone `code-server` and `cloudflared` binary on demand (supports both `x86_64` and `arm64`).
- 🌐 **Instant HTTPS Public Access**: Creates a free, temporary Cloudflare Quick Tunnel (`https://*.trycloudflare.com`) on startup, or connects to your custom domain via Cloudflare Named Tunnel tokens.
- 📁 **Preconfigured Workspace (`/workspaces`)**: Defaults to `/workspaces` as the root workspace directory, created automatically if missing, with full `/workspace` symlink compatibility.
- 📦 **Automated Services Sync**: Automatically synchronizes the `services/` directory into `/workspaces/services/` upon launch, making your configs and scripts immediately accessible inside the IDE.
- 🔌 **VS Code Extension Installer**: Includes `services/coder/install_extension.sh` to quickly install essential development extensions (Python, Jupyter, etc.).
- 🔒 **Security First**: Automatically generates a cryptographically strong session password, restricts configuration permissions (`0600`), and binds the server locally to `127.0.0.1:8001`.
- 🔄 **Robust Lifecycle Management**: Supervise processes in the foreground with graceful `Ctrl+C` handling, or run as background daemons with safe PID tracking and `/proc` validation.

---

## 📁 Repository Structure

```text
coder_server/
├── start_server.py                  # Starts code-server, Cloudflare tunnel & copies services
├── stop_server.py                   # Safely terminates all managed processes & cleans state
├── services/                        # Service definitions (automatically copied to /workspaces/services)
│   ├── coder/
│   │   └── install_extension.sh     # Bash script to install VS Code extensions
│   ├── mysql+phpadmin/              # MySQL & phpMyAdmin service configs
│   └── redis/                       # Redis service configs
├── workspaces/                      # Local workspace folder (.gitignored)
└── README.md                        # Project documentation
```

---

## 📋 Requirements

- **Environment**: Linux x86_64 or arm64 (Kaggle Notebook runtime, Google Colab, Ubuntu, Docker).
- **Internet**: Enabled in runtime settings (on Kaggle: **Settings > Internet > On**).
- **Dependencies**: Python 3.9+ (Python standard library only; no `pip` dependencies required), `bash`, `curl`, and `git`.

---

## 🟢 Quick Start

### 1. In a Kaggle Notebook Cell

Create a code cell in your Kaggle notebook and run:

```bash
%%bash
if [ -d coder_server/.git ]; then
    cd coder_server && git pull
else
    git clone https://github.com/conghoa004/coder_server.git
fi
cd coder_server
python start_server.py --foreground
```

### 2. In a Linux Terminal

Clone the repository and run:

```bash
git clone https://github.com/conghoa004/coder_server.git
cd coder_server

# Run in foreground mode (recommended for interactive supervision; press Ctrl+C to stop):
python start_server.py --foreground

# Or run in background mode (daemon):
python start_server.py
```

### 3. Accessing the IDE

Once startup completes, the script prints your connection details:

```text
=======================================================
CODE-SERVER IS READY
URL: https://random-subdomain.trycloudflare.com
Local URL: http://127.0.0.1:8001
PASSWORD (keep private): dGhpcy1pcy1hLXNlY3JldC1rZXk...
=======================================================
```

1. Open the public **`URL`** in any web browser.
2. Enter the generated **`PASSWORD`**.
3. Your VS Code editor opens directly in `/workspaces` with your `services/` folder ready!

---

## 🔌 Installing VS Code Extensions

The repository bundles an extension installer at `services/coder/install_extension.sh`.

### Default Included Extensions:
- 🐍 **`ms-python.python`**: Python IntelliSense, code formatting, linting, and debugging.
- 📓 **`ms-toolsai.jupyter`**: Full interactive Jupyter Notebook editor and execution support.

### How to Run the Installer:

- **Inside the code-server integrated terminal**:
  ```bash
  bash /workspaces/services/coder/install_extension.sh
  ```

- **From the repository directory in terminal**:
  ```bash
  bash services/coder/install_extension.sh
  ```

- **From a Kaggle notebook cell**:
  ```bash
  !bash /workspaces/services/coder/install_extension.sh
  ```

### Adding More Extensions:
Edit the `EXTENSIONS` array in `services/coder/install_extension.sh`:

```bash
EXTENSIONS=(
    "ms-python.python"
    "ms-toolsai.jupyter"
    "GitHub.copilot"
    "eamodio.gitlens"
)
```

---

## 🛑 Stopping the Server

To stop both `code-server` and `cloudflared` cleanly and remove temporary configuration files:

### From Terminal:
```bash
python stop_server.py
```

### From Kaggle Notebook Cell:
```bash
!python /kaggle/working/coder_server/stop_server.py
```

> **Note**: `stop_server.py` validates process command lines via `/proc/<pid>/cmdline` before issuing signals, ensuring it never kills unrelated processes that reuse the same PID.

---

## ⚙️ Configuration & Options

The scripts can be configured via CLI flags or environment variables:

| CLI Option | Environment Variable | Default | Description |
|---|---|---|---|
| `--port <PORT>` | `CODE_SERVER_PORT` | `8001` | Local port used by `code-server`. |
| `--workspace <PATH>` | `WORKSPACE_DIR` / `KAGGLE_WORKING_DIR` | `/workspaces` | Workspace directory opened in code-server and destination for `services/`. |
| `--tunnel-token <TOKEN>` | `CLOUDFLARE_TUNNEL_TOKEN` | `None` | Cloudflare Zero Trust Named Tunnel token. If omitted, uses Cloudflare Quick Tunnel (`trycloudflare.com`). |
| `--no-tunnel` | - | `False` | Run code-server locally only, without Cloudflare Tunnel. |
| `--foreground` | - | `False` | Supervise services in the foreground; stops cleanly on `Ctrl+C`. |
| - | `CODE_SERVER_STATE_DIR` | `~/.local/state/kaggle-code-server` | Directory for temporary PID and YAML config files. |

### Configuration Examples:

```bash
# Run on a custom port and custom workspace:
python start_server.py --port 9090 --workspace /my/workspace --foreground

# Connect using a permanent Cloudflare Named Tunnel token:
python start_server.py --tunnel-token "eyJh..." --foreground

# Run local only (no public tunnel):
python start_server.py --no-tunnel --foreground
```

---

## 📋 Logs and Process State

- **Logs**: Written inside the active workspace directory:
  - `<WORKSPACE>/code-server.log`: code-server startup, output, and extension logs.
  - `<WORKSPACE>/cloudflared.log`: Cloudflare tunnel connection events and URLs.
- **State & Runtime Configuration**: Stored under `~/.local/state/kaggle-code-server/`:
  - `code-server.pid`: Saved PID for the `code-server` process.
  - `cloudflared.pid`: Saved PID for the `cloudflared` process.
  - `config.yaml`: Temporary runtime configuration with owner-only (`0600`) permissions (removed when `stop_server.py` runs).

---

## 🔒 Security Best Practices

- **Protect the Password**: Anyone with both the Cloudflare URL and password has full terminal access inside your environment. Do not publicly share notebook outputs containing connection details.
- **Quick Tunnels vs Named Tunnels**: Cloudflare Quick Tunnels (`trycloudflare.com`) are temporary and meant for development sessions. For long-running or production environments, use Cloudflare Named Tunnels via `--tunnel-token`.
- **Clean Shutdown**: Run `python stop_server.py` when your session ends to close open tunnels and wipe temporary credentials.

---

## 📄 License

MIT License. Free to use and customize for your research, Kaggle competitions, and remote workflows!
