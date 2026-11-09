# 🚀 Kaggle Code Server

This project starts code-server in a Kaggle Linux runtime and exposes it through a temporary Cloudflare Quick Tunnel. It lets you use a browser-based VS Code editor with /kaggle/working as the workspace.

## 📁 Files

- ▶️ **start_server.py** installs missing dependencies, starts code-server, and creates the Cloudflare Tunnel.
- ⏹️ **stop_server.py** stops the code-server and tunnel processes started by start_server.py.
- 📓 **coder-server.ipynb** contains the original notebook-based setup and usage notes.

## ✅ Requirements

- A Kaggle Linux notebook runtime.
- Internet access enabled in Kaggle. The start script downloads code-server and cloudflared if they are not already installed.
- Python 3.9 or newer. The scripts use only the Python standard library.

Place start_server.py and stop_server.py in the Kaggle working directory (usually /kaggle/working). If the files are elsewhere, use their full paths in the commands below.

## 🟢 Start the server

From a terminal in the directory containing the scripts:

~~~bash
python start_server.py
~~~

Or from a Kaggle notebook cell:

~~~python
!python /kaggle/working/start_server.py
~~~

When startup completes, the script prints:

- The temporary Cloudflare URL.
- A randomly generated password for the code-server login.

Open the URL in a browser and enter the password. Keep both values private.

## 🛑 Stop the server

From a terminal:

~~~bash
python stop_server.py
~~~

Or from a Kaggle notebook cell:

~~~python
!python /kaggle/working/stop_server.py
~~~

The stop script uses PID files created by the start script. It only signals processes whose command lines match the expected services.

## 📋 Logs and state

- code-server.log and cloudflared.log are written to the configured workspace.
- PID files and the temporary code-server configuration are stored under ~/.local/state/kaggle-code-server/ by default.
- The code-server configuration is created with owner-only permissions and removed when the stop script successfully stops code-server.

## ⚙️ Configuration

The scripts accept these optional environment variables. Use the same values when starting and stopping:

| Variable | Default | Purpose |
| --- | --- | --- |
| CODE_SERVER_PORT | 8080 | Local port used by code-server. |
| KAGGLE_WORKING_DIR | /kaggle/working | Workspace opened in code-server and location for logs. |
| CODE_SERVER_STATE_DIR | ~/.local/state/kaggle-code-server | Directory for PID files and code-server configuration. |

Example:

~~~bash
CODE_SERVER_PORT=9090 python start_server.py
CODE_SERVER_PORT=9090 python stop_server.py
~~~

## 🔒 Security notes

The Cloudflare Quick Tunnel makes the editor reachable over the public Internet. Anyone who has both the URL and password can access the editor and its terminal. Do not share notebook outputs or logs containing connection details, and stop the services when you finish.

Quick Tunnels are temporary and are intended for development use, not production deployments.
