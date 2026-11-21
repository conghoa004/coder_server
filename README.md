# 🚀 Kaggle Code Server

This project starts code-server in a Kaggle Linux runtime, bound to localhost. It opens /kaggle/working as the browser-based VS Code workspace.

## 📁 Files

- ▶️ **start_server.py** installs missing dependencies and starts code-server.
- ⏹️ **stop_server.py** stops code-server.

## ✅ Requirements

- A Kaggle Linux notebook runtime.
- Internet access enabled in Kaggle. The start script downloads code-server if it is not already installed.
- Git available in the Kaggle runtime.
- Python 3.9 or newer. The scripts use only the Python standard library.


## 🟢 Start the server

From a terminal in the directory containing the scripts:

~~~bash
git clone https://github.com/conghoa004/coder_server.git
cd coder_server
python start_server.py
~~~

By default, the Python script starts code-server in the background and exits. To keep it in the foreground and stop it with Ctrl+C:

~~~bash
python start_server.py --foreground
~~~

Or from a Kaggle notebook cell:

~~~python
!if [ -d coder_server/.git ]; then cd coder_server && git pull; else git clone https://github.com/conghoa004/coder_server.git; fi
!python /kaggle/working/coder_server/start_server.py --foreground
~~~

When startup completes, the script prints:

- The local address (`http://127.0.0.1:8080` by default).
- A randomly generated password for the code-server login.

The server is only available inside the Kaggle runtime. Enter the password when connecting through a local proxy or notebook interface.

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

- code-server.log is written to the configured workspace.
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

The editor listens only on localhost. The password protects access from other local processes that can reach the service.
