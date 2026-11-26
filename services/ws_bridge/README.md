# 🌉 WSB (WebSocket Bridge - Viết bằng Rust)

Một công cụ tạo đường hầm (tunnel bridge) đa giao thức, bất đồng bộ với hiệu năng cực cao được viết hoàn toàn bằng **Rust** và **Tokio**. `wsb` đóng gói và truyền tải toàn bộ luồng dữ liệu **TCP**, **UDP** và **HTTP** thông qua giao thức **WebSocket** (`ws://` hoặc `wss://`), giúp kết nối an toàn tới bất kỳ dịch vụ nội bộ nào trên máy chủ từ xa (như **Kaggle Notebooks**, Google Colab, Docker, Linux VM) qua các dịch vụ web tunnel thông dụng (như **Cloudflare Quick Tunnel**) mà **không cần mở cổng TCP thô (raw TCP port-forwarding) hay IP tĩnh**.

---

## ✨ Tính Năng Nổi Bật

- 🦀 **Rust Thuần & Tokio**: Xây dựng trên nền tảng `tokio`, `tokio-tungstenite` và `rustls` cho độ trễ cực thấp, thông lượng I/O cao, tiêu tốn ít RAM và không phụ thuộc vào thư viện C bên ngoài.
- ⚡ **Cú pháp CLI ngắn gọn**: Sử dụng bộ lệnh đơn giản: `wsb server` và `wsb client`.
- 🔌 **Hỗ Trợ Toàn Diện Mọi Giao Thức**:
  - **TCP**: Kết nối tới mọi dịch vụ TCP (MySQL, PostgreSQL, Redis, SSH, MongoDB, Kafka, v.v.).
  - **UDP**: Truyền tải gói tin datagram (truy vấn DNS, máy chủ game, Syslog, v.v.) kèm cơ chế tự động theo dõi phiên (session tracking) và giải phóng khi hết thời gian chờ (idle timeout).
  - **HTTP**: Truy cập ứng dụng web (phpMyAdmin, REST API, dashboard quản trị) kèm tính năng **tự động viết lại header `Host`** để Apache/Nginx nhận diện đúng virtual host và xử lý redirect/cookie chính xác.
  - 🧦 **SOCKS5 Dynamic Proxy**: Tích hợp sẵn máy chủ SOCKS5 proxy cục bộ, cho phép trình duyệt, `curl` hoặc các phần mềm quản trị database truy cập linh hoạt tới **bất kỳ cổng/dịch vụ nào** trên remote server qua một cổng duy nhất mà không cần map từng cổng riêng.
- 🔒 **Bảo Mật Cao**: Cơ chế xác thực bằng token bí mật (`SECRET`) ngay khi bắt tay (handshake) và hỗ trợ danh sách trắng (`allowed_targets`) trên server.
- ⚙️ **Cấu Hình Linh Hoạt**: Hỗ trợ đồng thời cả file cấu hình TOML rõ ràng lẫn cờ tham số dòng lệnh (CLI flags) nhanh.
- 🌐 **Tương Thích Tuyệt Đối Với Cloudflare Tunnel**: Hoạt động mượt mà với `cloudflared tunnel --url http://127.0.0.1:8766`.

---

## 📐 Kiến Trúc & Luồng Dữ Liệu

```text
Máy Cục Bộ (Client)                                   Máy Chủ Từ Xa (Server / Kaggle)
===================                                   ===============================

[ Phần mềm MySQL / DBeaver ] (TCP :13306)
[ Redis CLI                ] (TCP :16379)
[ Trình duyệt Web          ] (HTTP :18080)
[ DNS Client               ] (UDP :10053)
[ Ứng dụng dùng SOCKS5     ] (SOCKS5 :1080)
            │
            ▼
┌─────────────────────────────────┐
│        wsb client (Rust)        │
└─────────────────────────────────┘
            │
            ▼  WebSocket Binary Frames (WSS)
┌─────────────────────────────────┐
│  Cloudflare Tunnel / Web Proxy  │
│  (https://*.trycloudflare.com)  │
└─────────────────────────────────┘
            │
            ▼
┌─────────────────────────────────┐
│        wsb server (Rust)        │ (Port 8766)
└─────────────────────────────────┘
            │
            ├──► [ MySQL Database  ] (TCP :3306)
            ├──► [ Redis Cache     ] (TCP :6379)
            ├──► [ Apache/phpMyAdmin] (HTTP :80)
            ├──► [ Dịch vụ SSH     ] (TCP :22)
            └──► [ DNS Resolver    ] (UDP :53)
```

---

## 📦 Hướng Dẫn Cài Đặt Cargo & Rust

Nếu hệ thống của bạn chưa có sẵn `cargo` và `rustc`, bạn có thể cài đặt nhanh theo hướng dẫn dưới đây:

### 1. Trên Linux / Kaggle / Google Colab / Ubuntu
- **Cài qua `rustup` chính thức (Khuyên dùng - luôn có bản mới nhất)**:
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
  ```
- **Kiểm tra cài đặt**:
  ```bash
  cargo --version
  rustc --version
  ```

### 2. Trên Windows
 **Tải và chạy file `rustup-init.exe` từ trang chủ [https://rustup.rs/](https://rustup.rs/) (chọn tùy chọn mặc định `1` để cài đặt).**
- **Kiểm tra cài đặt**:
  ```powershell
  cargo --version
  ```

### 3. Trên macOS
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
# hoặc cài qua brew:
# brew install rust
```

---

## 🚀 Hướng Dẫn Biên Dịch (Build)

### Trên Linux / Kaggle / Google Colab
Dự án đã có sẵn script tự động kiểm tra và cài đặt Rust nếu môi trường chưa có:
```bash
# Chạy script tự động cài đặt Rust (nếu thiếu) và biên dịch:
bash services/ws_bridge/build.sh

# Hoặc biên dịch trực tiếp bằng Cargo:
cargo build --release
```
File thực thi sau khi build nằm tại: `target/release/wsb`.

> [!TIP]
> Bạn có thể chạy trực tiếp bằng đường dẫn `./target/release/wsb`, hoặc copy vào PATH để có thể gõ lệnh `wsb` từ bất kỳ thư mục nào:
> ```bash
> cp target/release/wsb ~/.local/bin/   # hoặc sudo cp target/release/wsb /usr/local/bin/
> ```

### Trên Windows
```powershell
cargo build --release
```
File thực thi: `target\release\wsb.exe`.

---

## 📖 Hướng Dẫn Khởi Chạy Nhanh

### 1. Phía Server (Kaggle / Máy chủ từ xa)

#### Cách 1: Dùng file cấu hình `config.server.toml`
```bash
./target/release/wsb server -c config.server.toml
```

#### Cách 2: Dùng cờ lệnh trực tiếp (CLI flags)
```bash
# Tự động tạo token ngẫu nhiên bảo mật 256-bit (Khuyên dùng):
./target/release/wsb server --listen 0.0.0.0:8766

# Hoặc chỉ định token cố định theo ý muốn:
./target/release/wsb server --listen 0.0.0.0:8766 --token "mat-khau-bi-mat"

# Tạo riêng một token ngẫu nhiên để lưu vào file cấu hình:
./target/release/wsb gen-token
```

#### Public server ra internet bằng Cloudflare Tunnel:
```bash
cloudflared tunnel --url http://127.0.0.1:8766 --no-autoupdate
```
Cloudflare sẽ in ra một địa chỉ HTTPS công khai dạng:
`https://xxx-xxx-xxx.trycloudflare.com`

---

### 2. Phía Client (Máy tính cá nhân)

#### Cách 1: Dùng file cấu hình `config.client.toml`
Mở file [config.client.toml] và điền URL Cloudflare cùng token:
```toml
# Địa chỉ WebSocket Server (Cloudflare tunnel)
ws_url = "wss://xxx-xxx-xxx.trycloudflare.com"

# Token bí mật khớp với server
token = "mat-khau-bi-mat"

# (Tùy chọn) Bật SOCKS5 Proxy động
socks5 = "127.0.0.1:1080"

# --- Danh sách các cổng cần chuyển tiếp ---

# 1. Đường hầm TCP cho MySQL
[[tunnels]]
name = "mysql"
protocol = "tcp"
listen = "127.0.0.1:13306"
target = "127.0.0.1:3306"

# 2. Đường hầm TCP cho Redis
[[tunnels]]
name = "redis"
protocol = "tcp"
listen = "127.0.0.1:16379"
target = "127.0.0.1:6379"

# 3. Đường hầm HTTP cho phpMyAdmin / Web
[[tunnels]]
name = "web_phpmyadmin"
protocol = "http"
listen = "127.0.0.1:18080"
target = "127.0.0.1:80"

# 4. Đường hầm UDP cho DNS
[[tunnels]]
name = "dns"
protocol = "udp"
listen = "127.0.0.1:10053"
target = "1.1.1.1:53"
```

Chạy client với cấu hình trên:
```bash
./target/release/wsb client -c config.client.toml
```

#### Cách 2: Dùng dòng lệnh (CLI flags) nhanh gọn
```bash
./target/release/wsb client \
  --ws "wss://xxx-xxx-xxx.trycloudflare.com" \
  --token "mat-khau-bi-mat" \
  --tcp 127.0.0.1:13306=127.0.0.1:3306 \
  --tcp 127.0.0.1:16379=127.0.0.1:6379 \
  --http 127.0.0.1:18080=127.0.0.1:80 \
  --udp 127.0.0.1:10053=1.1.1.1:53 \
  --socks5 127.0.0.1:1080
```

---

## 🎯 Ví Dụ Sử Dụng Chi Tiết Từng Giao Thức

### 1. Kết nối cơ sở dữ liệu MySQL (Giao thức TCP)
Mở các công cụ quản trị như DBeaver, Navicat, MySQL Workbench hoặc VS Code:
- **Host**: `127.0.0.1`
- **Port**: `13306`
- **User / Password**: Nhập tài khoản MySQL trên máy chủ (ví dụ `admin`).

Hoặc kiểm tra kết nối bằng script Python có sẵn:
```bash
python test_mysql.py
```

### 2. Truy cập phpMyAdmin / Web (Giao thức HTTP)
Mở trình duyệt web và truy cập địa chỉ:
```text
http://127.0.0.1:18080/phpmyadmin/
```
> [!NOTE]
> `wsb` tự động viết lại header `Host: 127.0.0.1:80` để Apache và phpMyAdmin nhận diện đúng cấu hình virtual host và xử lý redirect chính xác, tránh lỗi chuyển hướng sai trang.

### 3. Phân giải tên miền DNS (Giao thức UDP)
Kiểm tra khả năng truyền tải UDP bằng công cụ `nslookup` hoặc `dig` qua cổng UDP local `10053`:
```bash
nslookup google.com 127.0.0.1 -port=10053
```

### 4. Sử dụng SOCKS5 Dynamic Proxy
Chỉ với duy nhất một cổng proxy `127.0.0.1:1080`, bạn có thể truy cập **mọi dịch vụ nội bộ** trên máy chủ từ xa:
- **Truy cập web bằng `curl`**:
  ```bash
  curl --socks5-hostname 127.0.0.1:1080 http://127.0.0.1:80/phpmyadmin/
  ```
- **Trình duyệt Web (Chrome / Firefox với tiện ích SwitchyOmega)**:
  Cấu hình SOCKS5 proxy tới `127.0.0.1:1080` để duyệt toàn bộ các trang nội bộ trên server mà không cần cấu hình port forwarding từng trang.
- **SSH qua SOCKS5 ProxyCommand**:
  ```bash
  ssh -o "ProxyCommand=nc -X 5 -x 127.0.0.1:1080 %h %p" root@127.0.0.1
  ```

---

## ⚙️ Bảng Tra Cứu Tham Số Dòng Lệnh (CLI Reference)

### Lệnh `wsb server`
| Cờ (Flag) | Biến môi trường | Giá trị mặc định | Giải thích chức năng |
|---|---|---|---|
| `-c, --config <PATH>` | - | - | Đường dẫn tới file cấu hình TOML của server |
| `-l, --listen <ADDR>` | `WS_BRIDGE_LISTEN` | `0.0.0.0:8766` | Địa chỉ IP và cổng lắng nghe kết nối WebSocket |
| `-t, --token <SECRET>` | `WS_BRIDGE_TOKEN` | `auto` (tự sinh 256-bit) | Chuỗi bí mật dùng để xác thực client khi kết nối |
| `--allow <TARGET>` | - | `(cho phép tất cả)` | Giới hạn chỉ cho phép kết nối tới các target cụ thể (vd: `127.0.0.1:3306`) |

### Lệnh `wsb client`
| Cờ (Flag) | Biến môi trường | Giá trị mặc định | Giải thích chức năng |
|---|---|---|---|
| `-c, --config <PATH>` | - | - | Đường dẫn tới file cấu hình TOML của client |
| `-w, --ws <URL>` | `WS_BRIDGE_URL` | - | URL WebSocket của máy chủ từ xa (`wss://...` hoặc `ws://...`) |
| `-t, --token <SECRET>` | `WS_BRIDGE_TOKEN` | `12345` | Chuỗi bí mật xác thực (phải khớp với server) |
| `--tcp <LISTEN=TARGET>` | - | - | Khởi tạo đường hầm TCP (vd: `127.0.0.1:13306=127.0.0.1:3306`) |
| `--udp <LISTEN=TARGET>` | - | - | Khởi tạo đường hầm UDP (vd: `127.0.0.1:10053=1.1.1.1:53`) |
| `--http <LISTEN=TARGET>` | - | - | Khởi tạo đường hầm HTTP có hỗ trợ viết lại Host header |
| `--socks5 <LISTEN>` | - | - | Bật SOCKS5 dynamic proxy trên cổng chỉ định (vd: `127.0.0.1:1080`) |
