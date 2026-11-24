# 🚀 Remote Code Server & Cloudflare Tunnel

[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20Kaggle%20%7C%20Docker-blue?logo=linux)](https://github.com/conghoa004/coder_server)
[![Python](https://img.shields.io/badge/Python-3.9%2B-brightgreen?logo=python)](https://python.org)
[![code-server](https://img.shields.io/badge/VS%20Code-code--server-blueviolet?logo=visualstudiocode)](https://github.com/coder/code-server)
[![Cloudflare Tunnel](https://img.shields.io/badge/Cloudflare-Tunnel-orange?logo=cloudflare)](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/)

A lightweight, automated toolkit to launch a full-featured browser-based **VS Code (`code-server`)** inside headless Linux environments (such as **Kaggle Notebooks**, Google Colab, Docker containers, or remote Linux VMs).

It automatically sets up dependencies, configures secure authentication, exposes the service to the internet via **Cloudflare Tunnel** (HTTPS, zero port-forwarding required), initializes a dedicated `/workspaces` folder, and provides an extension installation manager.

---

## ✨ Features

- ⚡ **Zero-Configuration Setup**: Automatically downloads and installs the official standalone `code-server` and `cloudflared` binary (auto-detects `x86_64` and `arm64`).
- 🌐 **Instant HTTPS Public Access**: Creates an on-demand Cloudflare Quick Tunnel (`https://*.trycloudflare.com`) with zero account required, or connects to your custom domain using Cloudflare Named Tunnel tokens.
- 📁 **Dedicated Workspace**: Defaults to `/workspaces` (auto-creates if missing), keeping projects clean and isolated.
- 📦 **Automatic Services Sync**: Automatically copies the `services/` directory into your workspace (`/workspaces/services`) on startup.
- 🔌 **Automated Extensions**: Includes `install_extension.sh` to pre-install essential development tools like Python (`ms-python.python`) and Jupyter (`ms-toolsai.jupyter`).
- 🔒 **Security First**: Generates a strong cryptographically secure random session password, protects configuration files with `0600` permissions, and binds the web server to localhost.
- 🔄 **Process Lifecycle Management**: Run as a supervised foreground process (with `Ctrl+C` graceful cleanup) or as a detached daemon with safe PID tracking and validation.

---

## 📁 Repository Structure

```text
├── start_server.py        # Starts code-server and Cloudflare Tunnel
├── stop_server.py         # Gracefully stops all managed services and cleans state
├── services/              # Bundled services and scripts (auto-copied to workspace)
│   ├── coder/
│   │   └── install_extension.sh
│   ├── mysql+phpadmin/
│   └── redis/
├── workspaces/            # Default workspace directory (mounted in VS Code)
└── README.md              # Documentation
```

---

## 📋 Requirements

- **Operating System**: Linux (x86_64 or arm64), e.g., Kaggle Notebooks, Google Colab, Docker, or Ubuntu VM.
- **Python**: Python 3.9+ (uses standard library only; no `pip install` required).
- **Network**: Internet access enabled (in Kaggle: set **Settings > Internet > On**).
- **Tools**: `bash`, `curl`, and `git` installed in runtime.

---

## 🟢 Quick Start

### 1. In a Kaggle Notebook Cell

Copy and run this cell to clone and start the server in foreground mode:

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

```bash
git clone https://github.com/conghoa004/coder_server.git
cd coder_server

# Start in the background (daemon mode)
python start_server.py

# Or start in foreground mode (stops on Ctrl+C)
python start_server.py --foreground
```

### 3. Accessing the IDE

When startup completes, you will see output similar to:

```text
=======================================================
CODE-SERVER IS READY
URL: https://random-subdomain.trycloudflare.com
Local URL: http://127.0.0.1:8001
PASSWORD (keep private): dGhpcy1pcy1hLXNlY3JldC1rZXk...
=======================================================
```

1. Open the **URL** in any web browser.
2. Enter the generated **PASSWORD**.
3. Your VS Code editor is ready at `/workspaces`!

---

## 🔌 Installing VS Code Extensions

The repository includes `install_extension.sh` to quickly install extensions into `code-server`.

### Default Extensions Included:
- 🐍 **`ms-python.python`**: Python language support, linting, debugging, and IntelliSense.
- 📓 **`ms-toolsai.jupyter`**: Full Jupyter notebook editing and execution inside VS Code.

### How to Run:

From a terminal:
```bash
bash services/coder/install_extension.sh
```

Or from a Kaggle notebook cell:
```bash
!bash /workspaces/services/coder/install_extension.sh
```

### Adding Custom Extensions:
Edit the `EXTENSIONS` array inside `install_extension.sh`:

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

To stop both `code-server` and `cloudflared` services cleanly and remove temporary credentials:

### Terminal:
```bash
python stop_server.py
```

### Kaggle Notebook Cell:
```bash
!python /kaggle/working/coder_server/stop_server.py
```

The script verifies process command lines against PID records so it never terminates unrelated processes.

---

## ⚙️ Configuration & Options

`start_server.py` supports both command-line flags and environment variables:

| CLI Option | Environment Variable | Default | Description |
|---|---|---|---|
| `--port <PORT>` | `CODE_SERVER_PORT` | `8001` | Local port for `code-server`. |
| `--workspace <PATH>` | `WORKSPACE_DIR` / `KAGGLE_WORKING_DIR` | `/workspaces` | Workspace directory opened in VS Code and log directory. |
| `--tunnel-token <TOKEN>` | `CLOUDFLARE_TUNNEL_TOKEN` | `None` | Cloudflare Named Tunnel token. If omitted, uses Quick Tunnel (`trycloudflare.com`). |
| `--no-tunnel` | `NO_TUNNEL=1` | `False` | Run code-server locally only, without Cloudflare Tunnel. |
| `--foreground` | - | `False` | Run in foreground mode and supervise processes (press `Ctrl+C` to stop). |
| - | `CODE_SERVER_STATE_DIR` | `~/.local/state/kaggle-code-server` | Location for temporary PID and config files. |

### Configuration Examples:

```bash
# Custom port and custom workspace
python start_server.py --port 9090 --workspace /my/project

# Use custom Cloudflare Named Tunnel (Zero Trust token)
python start_server.py --tunnel-token "eyJh..."

# Run local-only (no public tunnel)
python start_server.py --no-tunnel
```

---

## 📋 Logs and Process State

- **Logs**: Written inside the workspace folder:
  - `code-server.log`: Server outputs and extension runtime messages.
  - `cloudflared.log`: Tunnel connections and status events.
- **State & Config**: Saved under `~/.local/state/kaggle-code-server/`:
  - `code-server.pid`: PID of the running code-server process.
  - `cloudflared.pid`: PID of the Cloudflare Tunnel process.
  - `config.yaml`: Temporary runtime config containing the session password (permissions: `0600`, auto-deleted upon stop).

---

## 🔒 Security Best Practices

- **Private Password**: The generated session password protects against unauthorized access. Keep notebook outputs and logs containing this password private.
- **Temporary Quick Tunnels**: Free Cloudflare Quick Tunnels are designed for temporary development sessions. For permanent access or custom domains, use Cloudflare Zero Trust Named Tunnel tokens via `--tunnel-token`.
- **Clean Shutdown**: Always run `python stop_server.py` when your session ends to terminate active tunnels and remove temporary authentication credentials.

---

## 📄 License

MIT License. Feel free to use and modify for your workflows!
