# MRM — Media Resource Manager

**Quản lý tài nguyên dựng video trên Windows và macOS (chip Apple M)**: plugin, LUT, SFX, template, preset, footage — gom về một chỗ, tìm nhanh, xem trước, kéo thả thẳng vào app dựng. Kèm plugin **HNH SFX Finder** cho DaVinci Resolve.

> Files belong to Windows. Metadata belongs to MRM.
> MRM **không bao giờ** di chuyển, đổi tên hay xóa file thật của bạn — chỉ đọc để lập chỉ mục.

Nhà phát triển: **Phạm Nam** 

---

## Chọn bản nào?

| Bạn dùng… | Cài gì |
|---|---|
| Premiere, CapCut, After Effects… (hoặc nhiều app) | **MRM** |
| Chỉ DaVinci Resolve Studio | **Plugin HNH SFX Finder** (chạy độc lập, không cần MRM) |
| DaVinci Resolve + app khác | **Cả hai** — plugin tự dùng chung thư viện, tag, yêu thích với MRM |

Tải bản cài ở mục **[Releases](../../releases)**:

| | Windows 10/11 64-bit | macOS 11+ (Mac chip Apple M) |
|---|---|---|
| Ứng dụng MRM | `MRM_x.y.z_x64-setup.exe` | `MRM_x.y.z_aarch64.dmg` |
| Plugin DaVinci Resolve | `HNH_SFX_Finder_MRM_vx.y.z.zip` | `HNH_SFX_Finder_MRM_vx.y.z_macOS.zip` |

## Phiên bản hiện tại

| Thành phần | Phiên bản |
|---|---|
| MRM | **0.12.1** |
| Plugin HNH SFX Finder | **0.12.3** |

## Yêu cầu hệ thống

### MRM

| | Tối thiểu | Khuyên dùng |
|---|---|---|
| Hệ điều hành | Windows 10 64-bit · macOS 11 (Mac chip Apple M) | Windows 11 64-bit · macOS 14+ |
| RAM | 4 GB | 8 GB+ |
| Ổ đĩa | ~200 MB + cache xem trước (mặc định tối đa 4 GB, chỉnh được) | SSD |
| Khác | WebView2 (có sẵn trên Windows 11, bộ cài tự cài nếu thiếu) | |

### Tính năng AI (tùy chọn, chạy offline)

| | Yêu cầu |
|---|---|
| GPU | Windows: NVIDIA 6–8 GB VRAM trở lên (đã thử trên RTX 3060 Ti 8 GB). Không có GPU vẫn chạy bằng CPU nhưng chậm. Mac chip M: dùng GPU tích hợp (Metal), nên có 16 GB RAM |
| RAM | 16 GB khuyên dùng |
| Ổ đĩa | ~8–10 GB cho AI runtime + model; thêm ~3.3 GB nếu bật **AI nhìn ảnh/video** |
| Mạng | Chỉ cần khi tải model lần đầu. Sau đó chạy hoàn toàn offline, không gửi dữ liệu ra ngoài |

### Plugin HNH SFX Finder

| | Yêu cầu |
|---|---|
| Phần mềm | **DaVinci Resolve Studio 20.2+** (Workflow Integration chỉ có trên bản Studio) |
| Hệ điều hành | Windows 10/11 64-bit · macOS (Mac chip Apple M) |
| Cài đặt | Quyền Administrator (Mac: mật khẩu máy), đóng Resolve trước khi cài |

## Tính năng chính

### MRM

- **Thư viện nhiều ổ đĩa** — quét thư mục, tự nhận ra đâu là một gói tài nguyên (kể cả ZIP + folder đã giải nén là cùng một gói). Ổ ngoài rút ra vẫn xem được metadata.
- **Phân loại & tag** — Application, loại tài nguyên, chức năng, tag cá nhân; gợi ý tự động từ nội dung file, áp dụng hàng loạt.
- **Xem trước** — ảnh bìa, ảnh, video (kể cả ProRes/HEVC), audio có waveform, text/PDF, duyệt bên trong ZIP.
- **Media Browser** — duyệt từng file âm thanh / ảnh / video trong mọi gói, nghe thử, xem trước, **kéo thả thẳng vào Premiere, CapCut, After Effects, Explorer…** (chỉ sao chép, file gốc giữ nguyên).
- **AI offline** (tùy chọn):
  - Tìm theo ý nghĩa bằng tiếng Việt: *tiếng súng*, *vỗ tay*, *chuyển cảnh nhanh*…
  - Danh mục AI tự nhận diện (Whoosh, Impact, Glitch, Light leak, Film grain…), gắn tag cả danh mục bằng một cú bấm.
  - File tương tự; AI đọc README để gợi ý mô tả & tag.
  - **AI nhìn ảnh/video** (mặc định tắt): tìm theo nội dung hình — *cô gái tóc dài*, *nền trời xanh*…
- **Dọn dẹp** — tìm trùng lặp, gom các phiên bản của cùng một gói, kiểm tra "đã có chưa?" trước khi tải.
- **An toàn dữ liệu** — tự backup database mỗi ngày, tự kiểm tra và khôi phục nếu database hỏng.
- **Duyệt theo thư mục** — cây thư mục trong Media Browser, xem file của đúng thư mục bạn cần; ghim **thư mục yêu thích** để mở nhanh từ thanh bên.
- **Menu chuột phải** — mở thư mục chứa, sao chép đường dẫn, yêu thích… ngay trên danh sách.
- **Collections** — tự nhóm resource và file media theo ý bạn (vd: “SFX vlog”, “LUT điện ảnh”).
- **Xem bên trong ZIP, RAR, 7Z** — duyệt file, nghe/xem thử, ảnh bìa tự động từ bên trong file nén.
- **Đổi ảnh bìa nhanh** — dán ảnh (Ctrl+V) hoặc chọn ảnh; đặt ảnh/video trong Media Browser làm ảnh bìa.
- **Báo lỗi dễ** — xuất file chẩn đoán (không chứa đường dẫn hay dữ liệu cá nhân) để gửi nhà phát triển.
- **Loại trừ khi quét** — bỏ qua thư mục rác (cache của CapCut/Adobe, `node_modules`…), có sẵn danh sách mặc định.
- **Tự cập nhật** — báo khi có bản mới trên GitHub, bấm một nút là tải (kiểm tra SHA-256) và cài đè, dữ liệu giữ nguyên.
- **Hướng dẫn lần đầu** — 3 bước: loại trừ thư mục rác → chọn thư mục tài nguyên → bật AI nếu muốn.

### Plugin HNH SFX Finder (DaVinci Resolve)

- Tìm và nghe thử SFX, xem waveform, chèn vào timeline tại playhead, Auto Bin, chèn ảnh/video, chuỗi ảnh PNG.
- **Plugin lai**:
  - Máy **không có MRM** → chạy độc lập (tự quét thư mục, tự lưu tag/yêu thích).
  - Máy **có MRM** → tự dùng chung thư viện, tag, yêu thích với MRM.
- Tự kiểm tra bản plugin mới trên cùng trang Releases.
  - Cài MRM sau → thư viện, tag, yêu thích của plugin tự chuyển sang MRM.
- Khi plugin đang mở, MRM tạm dừng tác vụ nặng và nhả VRAM cho Resolve.

## Cài đặt

### MRM

1. Tải `MRM_x.y.z_x64-setup.exe` ở [Releases](../../releases) và chạy.
2. Mở MRM → **Libraries & Settings** → **Thêm Library**, chọn thư mục chứa tài nguyên.
3. (Tùy chọn) Bật **AI offline** trong Libraries & Settings.

Từ bản 0.9.0, MRM tự báo khi có bản mới (Libraries & Settings → Cập nhật). Cài bản mới đè lên bản cũ: bộ cài tự nhận ra và cập nhật đúng thư mục cũ, giữ nguyên dữ liệu.

> Windows SmartScreen có thể cảnh báo vì bộ cài chưa ký số → bấm **More info → Run anyway**.

### Plugin

1. Đóng DaVinci Resolve.
2. Giải nén `HNH_SFX_Finder_MRM_vx.y.z.zip`.
3. Chuột phải `install_windows.bat` → **Run as administrator**.
4. Mở Resolve → **Workspace → Workflow Integrations → HNH SFX Finder (MRM)**.

### macOS (Mac chip Apple M1/M2/M3/M4…)

> Bản Mac chưa ký số với Apple (miễn phí, không cần tài khoản Apple Developer) → lần đầu mở cần cho phép thủ công.

**MRM**

1. Tải `MRM_x.y.z_aarch64.dmg`, mở ra và kéo **MRM** vào **Applications**.
2. Mở MRM. Nếu macOS chặn: **System Settings → Privacy & Security** → kéo xuống, bấm **Open Anyway** (macOS 14 trở về trước: chuột phải MRM → **Open**).
3. Nếu báo *"MRM is damaged and can't be opened"*, mở **Terminal** và chạy:
   ```bash
   xattr -dr com.apple.quarantine /Applications/MRM.app
   ```

**Plugin**

1. Thoát hẳn DaVinci Resolve.
2. Giải nén `HNH_SFX_Finder_MRM_vx.y.z_macOS.zip`.
3. Mở **Terminal**, gõ `sh ` (có dấu cách), kéo file `install_mac.sh` vào cửa sổ Terminal rồi Enter; nhập mật khẩu máy Mac khi được hỏi.
4. Mở Resolve → **Workspace → Workflow Integrations → HNH SFX Finder (MRM)**.

Script lấy file bridge `WorkflowIntegration.node` bản macOS có sẵn trong Resolve (`/Library/Application Support/Blackmagic Design/DaVinci Resolve/Developer/Workflow Integrations/Examples`).

## Dữ liệu được lưu ở đâu

| Dữ liệu | Vị trí |
|---|---|
| Database MRM + backup | Windows: `%APPDATA%\com.nink.vault\` · Mac: `~/Library/Application Support/com.nink.vault/` |
| Cache xem trước, AI runtime, model | Thư mục chọn trong MRM → Settings → Lưu trữ |
| Dữ liệu plugin | `%APPDATA%\Electron\HNH-SFX-Finder-MRM\` |

## Câu hỏi thường gặp

**MRM có sửa hay xóa file của tôi không?** Không. MRM chỉ đọc file để lập chỉ mục; mọi thay đổi (tag, ghi chú, ảnh bìa…) chỉ nằm trong database của MRM.

**Sắp xếp lại thư mục có mất tag, yêu thích, ghi chú không?** Không, nếu làm đúng cách:
- Di chuyển / đổi tên thư mục gốc hoặc chuyển sang ổ khác → **Libraries & Settings → Đổi đường dẫn** ở library đó.
- Sắp xếp lại thư mục con bên trong → chỉ cần **Quét**: file di chuyển tự được nối lại.
- Gom sang library khác → thêm và quét library mới **trước**, rồi mới gỡ library cũ: file media đã chuyển mang theo tag, yêu thích. Gói tài nguyên: chọn bản cũ + bản mới → **Gộp**.

Khi bấm **Gỡ**, MRM báo trước dữ liệu nào sẽ mất và tự sao lưu database.

**Không có GPU NVIDIA thì sao?** MRM chạy bình thường. Chỉ phần AI sẽ chậm hơn (chạy bằng CPU), hoặc bạn có thể không bật AI.

**Plugin có chạy trên DaVinci Resolve bản miễn phí không?** Không — Workflow Integration chỉ có trên DaVinci Resolve Studio.

## Thành phần bên thứ ba

- [FFmpeg](https://ffmpeg.org) 8.1 bản **LGPL** — đóng gói kèm MRM để xem trước media (xem `src-tauri/binaries/FFMPEG-LICENSE.txt`).
- [Ollama](https://ollama.com) — AI runtime, MRM tải về khi người dùng bật AI.
- Model AI (`bge-m3`, `qwen2.5:7b`, `gemma3:4b`) — tải từ Ollama registry, theo giấy phép của từng model.
- `WorkflowIntegration.node` — thư viện của Blackmagic Design dành cho plugin DaVinci Resolve.

## Dành cho nhà phát triển

Build từ mã nguồn, cấu trúc dự án, kiến trúc dữ liệu: xem [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).
