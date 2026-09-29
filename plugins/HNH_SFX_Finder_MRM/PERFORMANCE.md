# v0.9.10 thumbnail scheduling

The visible media grid is the only source of thumbnail work. Scroll resets a 180 ms quiet period. Candidate visibility is recomputed at dispatch rather than using a stale FIFO. One request runs at a time; an offscreen active request is cancelled once. Successful cards remain intact across tabs. Null/error results are attempted once per view generation, so unsupported files cannot spin. Memory hits avoid disk I/O; backend PNG cache is unchanged.

# Đo hiệu năng v0.9.5

Trung vị 3 lần chạy mỗi phiên bản trên cùng máy, Edge headless. Dữ liệu giả lập 10.000/50.000 tài nguyên: một nửa audio, một nửa video, 50 tags, 20 folder và 20% yêu thích. Thời gian đồng bộ bằng performance.now(), đơn vị ms. Không gồm đọc index từ ổ đĩa, quét ổ, tải codec/giải mã video, độ trễ debounce hoặc thời gian kết nối Resolve. Cột dựng danh sách gồm nạp dữ liệu vào state và render, không phải toàn bộ thời gian khởi động plugin. Không đại diện mọi thư viện thực tế.

| Tài nguyên | Tác vụ | v0.9.4 | v0.9.5 |
|---|---|---:|---:|
| 10,000 | Dựng danh sách | 87.5 | 33.2 |
| 10,000 | Tìm kiếm | 81.7 | 26.1 |
| 10,000 | Chuyển sang Video | 201.7 | 133.8 |
| 50,000 | Dựng danh sách | 1171.2 | 92.7 |
| 50,000 | Tìm kiếm | 1116.2 | 75.0 |
| 50,000 | Chuyển sang Video | 1521.3 | 251.3 |

Kiểm thử áp lực: 20 lần làm mới danh sách không vượt 2 yêu cầu thumbnail đang chờ; payload thumbnail giả lập 1 MiB để kiểm tra giới hạn cache 16 MiB/80 mục (đo riêng dung lượng chuỗi cache, không phải tổng RAM). 20 lựa chọn video liên tiếp chỉ gọi preview cho file cuối; phản hồi cũ không ghi đè nguồn mới. Waveform tối đa một lần giải mã, bỏ các lựa chọn chờ đã cũ. Lệnh native đang chạy không thể ngắt cưỡng bức; kết quả cũ bị loại bỏ. Video 4 giây thật được dùng trong kiểm thử hồi quy preview.
