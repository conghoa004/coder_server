# 🚀 Kaggle Code Server

This project starts code-server in a Linux runtime (such as Kaggle) and exposes it securely over the internet through Cloudflare Tunnel. It opens `./workspaces` (or a configured workspace) as the browser-based VS Code workspace.

## 📁 Files

- ▶️ **start_server.py** installs missing dependencies, starts code-server, and creates the Cloudflare Tunnel.
- ⏹️ **stop_server.py** stops the code-server and tunnel processes started by start_server.py.

## ✅ Requirements

- A Kaggle Linux notebook runtime.
- Internet access enabled in Kaggle. The start script downloads code-server and cloudflared if they are not already installed.
- Git available in the Kaggle runtime.
- Python 3.9 or newer. The scripts use only the Python standard library.

## 🟢 Start the server

From a terminal in the directory containing the scripts:

~~~bash
git clone https://github.com/conghoa004/coder_server.git
cd coder_server
python start_server.py
~~~

By default, the Python script starts both services in the background and exits. To keep it in the foreground and stop services started by this run with Ctrl+C:

~~~bash
python start_server.py --foreground
~~~

Or from a Kaggle notebook cell:

~~~python
!if [ -d coder_server/.git ]; then cd coder_server && git pull; else git clone https://github.com/conghoa004/coder_server.git; fi
!python /kaggle/working/coder_server/start_server.py --foreground
~~~

When startup completes, the script prints:

- The temporary Cloudflare Quick Tunnel URL (`https://*.trycloudflare.com`).
- The local address (`http://127.0.0.1:8080`).
- A randomly generated password for the code-server login.

Open the public Cloudflare URL in any browser and enter the password. Keep both values private.

## 🛑 Stop the server

From a terminal:

~~~bash
python stop_server.py
~~~

Or from a Kaggle notebook cell:

~~~python
!python /kaggle/working/coder_server/stop_server.py
~~~

The stop script uses PID files created by the start script. It only signals processes whose command lines match the expected services.

## 📋 Logs and state

- `code-server.log` and `cloudflared.log` are written to the configured workspace directory (`./workspaces` by default).
- PID files and the temporary code-server configuration are stored under `~/.local/state/kaggle-code-server/` by default.
- The code-server configuration is created with owner-only permissions and removed when the stop script successfully stops code-server.

## ⚙️ Configuration

The scripts accept these optional environment variables or CLI flags:

| Variable / Flag | Default | Purpose |
| --- | --- | --- |
| `CODE_SERVER_PORT` | `8080` | Local port used by code-server. |
| `WORKSPACE_DIR` / `KAGGLE_WORKING_DIR` / `--workspace` | `./workspaces` | Workspace directory opened in code-server and location for logs (created automatically if not exists). |
| `CODE_SERVER_STATE_DIR` | `~/.local/state/kaggle-code-server` | Directory for PID files and code-server configuration. |
| `CLOUDFLARE_TUNNEL_TOKEN` / `--tunnel-token` | `None` | (Optional) Cloudflare Named Tunnel token. If omitted, uses Cloudflare Quick Tunnel (`trycloudflare.com`). |
| `--no-tunnel` | `False` | Run code-server locally only, without starting Cloudflare Tunnel. |

Example:

~~~bash
# Run with custom port
CODE_SERVER_PORT=9090 python start_server.py
CODE_SERVER_PORT=9090 python stop_server.py

# Run with a Cloudflare Named Tunnel token
CLOUDFLARE_TUNNEL_TOKEN="your_token_here" python start_server.py
~~~

## 🔒 Security notes

The Cloudflare Quick Tunnel makes the editor reachable over the public Internet. Anyone who has both the URL and password can access the editor and its terminal. Do not share notebook outputs or logs containing connection details, and stop the services when you finish.

Quick Tunnels are temporary and are intended for development use.
