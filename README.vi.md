# 🚀 Kaggle Code Server

Dự án này khởi chạy code-server trong môi trường Kaggle Linux và tạo Cloudflare Quick Tunnel tạm thời để truy cập bằng trình duyệt. Workspace được mở là /kaggle/working.

## 📁 Các tệp trong dự án

- **start_server.py** cài các thành phần còn thiếu, khởi chạy code-server và tạo Cloudflare Tunnel.
- **stop_server.py** dừng code-server và tunnel do start_server.py khởi chạy.
- **coder-server.ipynb** chứa quy trình thiết lập ban đầu dưới dạng notebook.

## ✅ Yêu cầu

- Môi trường Kaggle notebook chạy Linux.
- Git có sẵn trong môi trường Kaggle.
- Bật Internet trong Kaggle. Khi chưa có sẵn, script sẽ tải code-server và cloudflared.
- Python 3.9 trở lên. Hai script chỉ dùng các thư viện có sẵn trong Python.


## 📥 Clone repository

Clone repository trước khi khởi chạy server. Trong cell Kaggle notebook:

~~~bash
!git clone https://github.com/conghoa004/coder_server.git
~~~

Sau đó chuyển notebook vào thư mục repository:

~~~python
%cd /kaggle/working/coder_server
~~~

Trong terminal, clone repository rồi chuyển vào thư mục vừa tạo:

~~~bash
git clone https://github.com/conghoa004/coder_server.git
cd coder_server
~~~

## 🟢 Khởi chạy server

Trong terminal ở thư mục chứa hai script:

~~~bash
python start_server.py
~~~

Hoặc chạy trong cell của Kaggle notebook:

~~~python
!python /kaggle/working/coder_server/start_server.py
~~~

Khi khởi chạy xong, script in ra URL Cloudflare tạm thời và mật khẩu ngẫu nhiên để đăng nhập code-server. Mở URL trong trình duyệt và nhập mật khẩu.

## 🛑 Dừng server

Trong terminal:

~~~bash
python stop_server.py
~~~

Hoặc trong cell của Kaggle notebook:

~~~python
!python /kaggle/working/coder_server/stop_server.py
~~~

Script dừng sử dụng các PID file do script khởi động tạo ra. Script chỉ gửi lệnh dừng nếu command line của tiến trình khớp với dịch vụ dự kiến.

## 📋 Log và trạng thái

- code-server.log và cloudflared.log được ghi trong workspace đang dùng.
- PID file và tệp cấu hình tạm thời nằm trong ~/.local/state/kaggle-code-server/ theo mặc định.
- Tệp cấu hình code-server chỉ cho phép tài khoản hiện tại đọc; stop_server.py sẽ xóa tệp này khi dừng code-server thành công.

## ⚙ Cấu hình

Có thể thiết lập các biến môi trường dưới đây. Hãy dùng cùng giá trị khi khởi chạy và dừng:

| Biến | Mặc định | Mô tả |
| --- | --- | --- |
| CODE_SERVER_PORT | 8080 | Cổng nội bộ của code-server. |
| KAGGLE_WORKING_DIR | /kaggle/working | Workspace mở trong code-server và nơi lưu log. |
| CODE_SERVER_STATE_DIR | ~/.local/state/kaggle-code-server | Thư mục chứa PID file và cấu hình code-server. |

Ví dụ:

~~~bash
CODE_SERVER_PORT=9090 python start_server.py
CODE_SERVER_PORT=9090 python stop_server.py
~~~

## 🔒 Bảo mật

Cloudflare Quick Tunnel mở trình soạn thảo ra Internet công khai. Ai có cả URL và mật khẩu đều có thể truy cập editor và terminal của nó. Không chia sẻ URL, mật khẩu, output notebook hoặc log có thông tin truy cập. Hãy dừng dịch vụ khi dùng xong.

Quick Tunnel chỉ phù hợp cho phát triển và thử nghiệm, không dùng cho hệ thống production.
