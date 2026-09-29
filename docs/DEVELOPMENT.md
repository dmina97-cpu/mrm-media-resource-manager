# MRM — Media Resource Manager

Nhà phát triển: **Phạm Nam**

Ứng dụng quản lý tài nguyên dựng video: quản lý tập trung plugin, LUT, SFX, template, preset, footage.
Ứng dụng chạy trên Windows, local-first. **Không bao giờ tự di chuyển, đổi tên hay xóa file thật.**

> Files belong to Windows. Metadata belongs to MRM.

## Stack

| Lớp | Công nghệ |
|---|---|
| UI | React 19 + TypeScript (Vite) |
| Desktop shell | Tauri 2 (asset protocol để xem file local) |
| Backend | Rust: scanner, matcher, archive inspection, watcher |
| Database | SQLite (rusqlite, bundled) + FTS5 |
| Media | FFmpeg 8.1 LGPL (sidecar đóng gói sẵn), crate `image` |
| AI | Ollama (app tự tải và quản lý, cổng 11435) + `bge-m3` (embedding) + `qwen2.5:7b` (LLM) |

## Cấu trúc

```
src/                      Frontend React
  App.tsx                 Layout 3 cột, routing, phím tắt, scan/AI progress, event từ backend
  api.ts                  Kiểu dữ liệu + wrapper cho Tauri commands
  components/
    Sidebar.tsx           Điều hướng
    ResourceView.tsx      Grid (có ảnh bìa, hover để tua video) / List, có virtualization
    Inspector.tsx         Metadata, tag, gợi ý, AI, phiên bản, quan hệ, file bên trong, sources
    Media.tsx             Ảnh bìa, trình duyệt file, cửa sổ xem trước (ảnh/video/audio/text/PDF)
    Dashboard.tsx         Thống kê
    Matches.tsx           Xác nhận cặp ZIP ↔ folder "possible match"
    Cleanup.tsx           Trùng lặp · Phiên bản · "Đã có chưa?"
    Rules.tsx             Rule phân loại (gợi ý / tự động)
    Settings.tsx          Library, AI, lưu trữ & cache, backup/restore
    About.tsx             Giới thiệu nhà phát triển, tính năng, hướng dẫn sử dụng
src-tauri/src/
  normalize.rs            Chuẩn hóa tên + similarity
  matcher.rs              Ghép nguồn theo tên và chữ ký nội dung
  classify.rs             Gợi ý type/app từ extension
  scanner.rs              Quét library, chữ ký nội dung, incremental, volume serial
  media.rs                Phase 3: liệt kê file, thumbnail, sprite, waveform, proxy video, ảnh bìa, cache
  watcher.rs              Phase 4: theo dõi thư mục + ổ ngoài cắm/rút
  rules.rs                Phase 4: rule engine
  organize.rs             Phase 4: trùng lặp, version family, quan hệ, gợi ý học từ tag
  ai.rs                   Phase 5: Ollama runtime, embedding index, semantic search, phân tích LLM
  db.rs                   Schema (migration v1 → v2), FTS, merge
  commands.rs             Tauri commands chính
src-tauri/binaries/       ffmpeg-x86_64-pc-windows-msvc.exe (LGPL, kèm FFMPEG-LICENSE.txt)
```

## Phát triển

Yêu cầu: Node 20+, Rust (MSVC toolchain), Visual Studio C++ Build Tools, WebView2.

**FFmpeg sidecar** không nằm trong repo (file ~128 MB vượt giới hạn GitHub). Trước khi build, tải bản FFmpeg **LGPL** win64
(vd: `ffmpeg-n8.1-latest-win64-lgpl` từ github.com/BtbN/FFmpeg-Builds), lấy `bin/ffmpeg.exe` và đặt vào
`src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe`.

```bash
npm install
npm run tauri dev        # chạy app dev
npm run tauri build      # build installer (.msi / .exe NSIS) vào src-tauri/target/release/bundle
cd src-tauri && cargo test   # unit test Rust
```

- Database và backup: `%APPDATA%\com.nink.vault\` (giữ identifier cũ để không mất dữ liệu khi đổi tên app)
- Cache preview, AI runtime và model: thư mục chọn trong Settings → "Lưu trữ"
## Preview (Phase 3)

- **Ảnh**: png/jpg/webp/gif/bmp/tiff xem trực tiếp. EXR/PSD/TGA/DPX/HEIC được FFmpeg chuyển đổi để xem.
- **Video**: mp4/mov H.264, VP9 hoặc AV1 phát trực tiếp. ProRes, DNxHR, HEVC… được tạo bản xem thử 720p (2 phút đầu).
  Encoder thử lần lượt: NVENC (GPU) → OpenH264 → VP9. Hover lên card để tua nhanh 10 khung hình.
- **Audio**: phát kèm waveform, bấm vào waveform để tua.
- **ZIP**: duyệt bên trong, xem ảnh/text/audio trực tiếp. App chỉ giải nén đúng file cần xem vào cache.
- Cache tự dọn khi vượt giới hạn (mặc định 4 GB). Có thể xóa cache bất cứ lúc nào.

## Tự động hóa (Phase 4)

- **Watcher**: gom thay đổi trong 3 giây, rồi quét lại đúng resource bị ảnh hưởng. Cứ 20 giây kiểm tra ổ ngoài; khi ổ cắm lại thì tự quét.
- **Chữ ký nội dung**: hash danh sách (đường dẫn, size) của file bên trong. ZIP và folder có cùng nội dung được ghép dù tên khác nhau.
  File lẻ, RAR và 7Z dùng hash của 1MB đầu, 1MB cuối và size.
- **Dọn dẹp**: phát hiện trùng theo nội dung hoặc theo tên + version, gom version family (đánh dấu "Update Available"),
  và ô "Đã có chưa?" để kiểm tra trước khi tải.
- **Rules**: điều kiện theo tên, đường dẫn, loại file, library. Chế độ "Gợi ý" hoặc "Tự động".
- **Học từ bạn**: token trong tên hoặc extension thường đi kèm tag nào thì gợi ý tag đó.

## AI local (Phase 5)

Chạy hoàn toàn offline. Bật trong Settings → "Cài đặt & bật AI". Lần đầu app tải runtime (~1.4 GB) và 2 model (~6 GB).

- **Semantic search**: kết quả FTS và vector (bge-m3) được trộn bằng RRF, sắp theo "Liên quan". Hiểu cả tiếng Việt lẫn tiếng Anh.
- **Gợi ý tag**: lấy từ các resource giống nhất đã được gắn tag (kNN), hoặc so với nhãn tag.
- **Phân tích LLM**: đọc cấu trúc file và README rồi đề xuất mô tả, app, type, function tag và cách cài.
  Output ép theo JSON schema, app và type chỉ được chọn trong danh sách có sẵn. Người dùng luôn xác nhận trước khi áp dụng.
- **Tương tự**: danh sách resource gần nghĩa hiện trong Inspector.
- Khi tắt AI, runtime cũng được tắt để giải phóng VRAM.

## Tìm kiếm

- `light leak resolve`: tìm full-text (và theo ý nghĩa khi AI bật)
- `app:resolve` · `type:plugin` · `type:"stock video"` · `tag:tracking` · `status:installed`
- `is:fav` · `is:missing` · `is:matched` · `is:unclassified` · `lib:<tên library>`

Phím tắt: `Ctrl+F` tìm · `↑/↓` chọn (Shift để chọn dải) · `Ctrl+A` chọn hết · `F` favorite ·
`N` sang mục chưa phân loại tiếp theo · `Esc` bỏ chọn · `←/→` chuyển file trong cửa sổ xem trước.

## Các quy tắc an toàn dữ liệu

- Scanner và preview chỉ đọc file. Mọi dữ liệu sinh ra đều nằm trong thư mục cache của app.
- Nguồn biến mất chỉ bị đánh dấu `unavailable`, metadata vẫn giữ nguyên.
- Auto-merge chỉ khi similarity ≥95% (hoặc trùng chữ ký nội dung) và cùng thư mục cha. Mọi lần ghép đều tách lại được.
- Trùng lặp: app chỉ gộp metadata và không bao giờ xóa file.
- AI chỉ đề xuất. Rule "Tự động" chỉ gắn tag.
- Database tự backup mỗi ngày (giữ 10 bản).

## Roadmap

- Phase 1–5: xong.
- Còn lại: UI cho Collections (schema đã có), hỗ trợ xem bên trong RAR/7Z.
