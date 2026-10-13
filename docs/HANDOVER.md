# Bàn giao dự án MRM + plugin HNH SFX Finder

Cập nhật: 13/10/2026 · MRM **0.12.1** · Plugin **0.12.3** · schema DB **v13** · `PLUGIN_CONTRACT = 1`

Tài liệu này dành cho người (hoặc phiên làm việc AI) tiếp nhận dự án. Đọc xong là build, chạy thử, sửa lỗi và phát hành được.
Phần giới thiệu cho người dùng nằm ở [README](../README.md); chi tiết từng tính năng cũ nằm ở [DEVELOPMENT.md](DEVELOPMENT.md).

## 1. Dự án gồm gì

| Thành phần | Vị trí | Vai trò |
|---|---|---|
| **MRM** — Media Resource Manager | `src/` (React) + `src-tauri/` (Rust) | App desktop quản lý tài nguyên dựng video: plugin, LUT, SFX, template, preset, footage |
| **Plugin HNH SFX Finder** | `plugins/HNH_SFX_Finder_MRM/` | Workflow Integration cho DaVinci Resolve Studio (Electron do Resolve cung cấp) |
| CI | `.github/workflows/` | Build Windows + macOS, tạo GitHub Release |
| Script hỗ trợ | `scripts/` | Theo dõi CI, phát hành, điều khiển app dev |

Nền tảng: **Windows 10/11 x64** và **macOS 11+ chip Apple M**. Repo công khai: `dmina97-cpu/mrm-media-resource-manager`.
Repo cũ `dmina97-cpu/hnh-sfx-finder-releases` đã dừng ở bản v0.12.2 (bản cuối, trỏ người dùng sang repo này) — không phát hành thêm ở đó.

### Ba nguyên tắc không được phá

1. **Không bao giờ di chuyển, đổi tên hay xóa file thật của người dùng.** App chỉ đọc; mọi thứ sinh ra nằm trong thư mục dữ liệu / cache của app.
2. **Không thử nghiệm trên database thật.** Luôn dùng bản sao (mục 4).
3. **Không đưa số điện thoại hay thông tin cá nhân của nhà phát triển vào code, tài liệu, release.** Liên hệ duy nhất: Facebook `facebook.com/phamnam22s`.

## 2. Kiến trúc tóm tắt

```
React (src/)  ──invoke──▶  Rust commands (src-tauri/src/)  ──▶  SQLite (WAL)  ◀──  Plugin (node:sqlite)
                                   │
                                   ├─ FFmpeg sidecar (thumbnail, waveform, proxy video)
                                   └─ Ollama (AI offline, cổng 11435, app tự tải và quản lý)
```

- **Một database dùng chung**: `nink-vault.db` trong thư mục dữ liệu của app
  (Windows `%APPDATA%\com.nink.vault\`, macOS `~/Library/Application Support/com.nink.vault/`).
  Tên `nink-vault` / `com.nink.vault` là tên cũ của app, **giữ nguyên** để không mất dữ liệu người dùng.
- **MRM là bên lập chỉ mục duy nhất.** Plugin chỉ đọc qua các view `api_*` (hợp đồng `PLUGIN_CONTRACT`) và ghi tag / yêu thích / thư mục yêu thích.
  Đổi cấu trúc các view `api_*` = phá plugin đang cài → phải tăng `PLUGIN_CONTRACT` và xử lý tương thích.
- **Plugin lai**: lúc khởi động, mở được database của MRM → chế độ `mrm` (dùng chung dữ liệu); không mở được → `standalone` (tự quét, tự lưu).
  Quyết định trong `initMode()` (`plugins/.../main.js`), không phụ thuộc MRM đang mở hay tắt.
- **Đường dẫn lưu trong DB là tương đối** theo thư mục gốc của library, dùng dấu phân cách của hệ điều hành (`\` trên Windows, `/` trên macOS).
  Mọi chỗ ghép đường dẫn phải theo nền tảng — xem mục 7.

### Module Rust (`src-tauri/src/`)

| File | Việc |
|---|---|
| `db.rs` | Schema + migration (`SCHEMA_VERSION`), `open_safe` (tự phục hồi DB hỏng từ backup), view `api_*` |
| `scanner.rs`, `autodetect.rs`, `matcher.rs`, `normalize.rs`, `classify.rs` | Quét library thành *resource* (gói), ghép ZIP ↔ thư mục, gợi ý loại |
| `media_index.rs` | Chỉ mục cấp file (âm thanh / hình / video) cho Media Browser và plugin; nối lại file đã di chuyển (trong và giữa các library) |
| `assets.rs` | Backend của Media Browser (truy vấn, cây thư mục, thư mục yêu thích, kéo thả) |
| `media.rs`, `archive.rs`, `noteimg.rs` | Preview, thumbnail, waveform, proxy video; đọc ZIP / 7Z / RAR; ảnh ghi chú và ảnh bìa |
| `commands.rs` | Lệnh chính: library (thêm, gỡ, **đổi đường dẫn**), resource, backup |
| `rules.rs`, `autotag.rs`, `organize.rs`, `collections.rs`, `exclude.rs` | Rule gắn tag, nguồn gốc tag, trùng lặp, collection, loại trừ khi quét |
| `ai.rs`, `asset_ai.rs`, `vision.rs` | Ollama runtime, embedding + tìm theo ý nghĩa, AI nhìn ảnh / video (tùy chọn) |
| `updater.rs`, `diag.rs`, `presence.rs`, `watcher.rs` | Tự cập nhật từ GitHub Releases, file chẩn đoán, nhịp tim MRM ↔ plugin, theo dõi thư mục |

### Frontend (`src/`)

`App.tsx` (route, thanh công cụ, All Resources) · `api.ts` (kiểu dữ liệu + wrapper lệnh Tauri) ·
`components/MediaBrowser.tsx` (lớn nhất: duyệt file, phát âm thanh, waveform) · `Inspector.tsx` · `Sidebar.tsx` · `Settings.tsx` ·
`LibraryDialogs.tsx` (gỡ / đổi đường dẫn library) · `AppFolders.tsx` (gán thư mục cho ứng dụng) · `Updates.tsx` · `Onboarding.tsx`.
Giao diện tiếng Việt; chuỗi khác nhau theo nền tảng lấy từ `format.ts` (`IS_MAC`, `FILE_MANAGER`, `EXAMPLE_ROOT`).

## 3. Chuẩn bị môi trường

- Node 20+ (CI dùng 24), Rust stable. Windows: MSVC Build Tools + WebView2. macOS: Xcode Command Line Tools.
- **FFmpeg sidecar không nằm trong repo.** Đặt bản **LGPL** vào `src-tauri/binaries/`:
  - Windows: `ffmpeg-x86_64-pc-windows-msvc.exe` (BtbN `ffmpeg-n8.1-latest-win64-lgpl-8.1`; dùng 8.1 vì 9.x đòi driver NVIDIA mới cho NVENC)
  - macOS: `ffmpeg-aarch64-apple-darwin` (CI tự build từ mã nguồn, xem `release-macos.yml`)
- Nếu Rust cài ngoài vị trí mặc định: đặt `PATH`, `RUSTUP_HOME`, `CARGO_HOME` trước khi gọi `cargo`.

```bash
npm ci
npm run tauri dev                 # app dev (dùng database THẬT — xem mục 4 trước khi thử nghiệm)
npx tsc --noEmit -p .             # kiểm tra kiểu frontend
cd src-tauri && cargo test --locked
cd plugins/HNH_SFX_Finder_MRM && for f in tests/*.test.cjs; do node "$f" || break; done
```

Test tích hợp của plugin với database (`tests/mrm-store.integration.cjs`) cần Electron của Resolve (có `node:sqlite`):
`ELECTRON_RUN_AS_NODE=1 MRM_DB=<bản sao DB> <electron của Resolve> tests/mrm-store.integration.cjs`.

## 4. Chạy thử an toàn (bắt buộc khi thử tính năng ghi dữ liệu)

1. Tạo bản sao database (không copy file đang mở): trong SQLite chạy `VACUUM INTO '<thư mục thử>/nink-vault.db'`, hoặc dùng một file backup trong `backups/`.
2. Chạy app dev trỏ vào bản sao, WebView2 dùng thư mục riêng (để không đụng MRM thật đang mở):

   ```bash
   MRM_DATA_DIR=<thư mục thử> WEBVIEW2_USER_DATA_FOLDER=<thư mục wv riêng> \
     npx tauri dev --config scripts/dev/tauri.debug.json
   ```

   Khi có `MRM_DATA_DIR`, app bỏ qua chế độ một-phiên-bản nên chạy song song được với MRM đã cài.
3. Điều khiển app không cần bấm tay (cổng debug 9229):

   ```bash
   node scripts/dev/cdp.mjs eval "I('list_libraries')"      # gọi lệnh Tauri
   node scripts/dev/cdp.mjs file thu-nghiem.mjs             # export default async (I) => {...}
   node scripts/dev/cdp.mjs shot man-hinh.png               # chụp cửa sổ
   ```

Lưu ý: sửa file trong `src-tauri/` khi `tauri dev` đang chạy sẽ khởi động lại app. Nếu WebView2 báo "resource is in use",
đổi sang thư mục `WEBVIEW2_USER_DATA_FOLDER` mới (tiến trình WebView2 của lần chạy trước còn giữ thư mục cũ).
Hộp thoại chọn thư mục là hộp thoại hệ điều hành — không điều khiển được qua CDP; thử phần đó bằng cách gọi thẳng lệnh backend.

## 5. Phát hành

Phiên bản nằm ở nhiều file — tăng cho đủ:

| Sản phẩm | File |
|---|---|
| MRM | `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (mục `nink-vault`), bảng phiên bản trong `README.md` |
| Plugin | `package.json`, `main.js` (`PLUGIN_VERSION`), `manifest.xml`, `UI/index.html` (2 chỗ), `install_windows.ps1`, thêm mục vào đầu `release-history.json` |

Quy trình:

1. Tăng phiên bản, viết `release-notes/vX.Y.Z.md` (tag theo phiên bản **MRM**), chạy đủ test ở mục 3.
2. `git tag vX.Y.Z && git push origin main vX.Y.Z` → workflow **Build & Release** (Windows) build, test và tạo Release.
3. `python scripts/ci.py mac vX.Y.Z` → workflow **macOS build** chạy song song, tự chờ Release xuất hiện (tối đa 40 phút) rồi gắn `.dmg` và gói plugin Mac.
4. `python scripts/ci.py status` / `logs` để theo dõi; `python scripts/ci.py release vX.Y.Z` để kiểm tra đủ 4 file.

Điều cần biết:

- **Bộ tự cập nhật nhận bản mới theo tên file**, không theo tag: MRM `MRM_x.y.z_x64-setup.exe` / `MRM_x.y.z_aarch64.dmg`;
  plugin `HNH_SFX_Finder_MRM_vX.Y.Z.zip` / `..._macOS.zip`. Đổi cách đặt tên = bản cũ không thấy bản mới.
  MRM kiểm tra SHA-256 qua trường `digest` của GitHub.
- Phát hành chỉ plugin: gắn zip mới vào một Release đã có (plugin tự chọn zip có phiên bản cao nhất trong các Release).
- Repo cần **Settings → Actions → General → Workflow permissions = Read and write** (đã bật). Nếu bước tạo Release vẫn lỗi 403:
  `python scripts/ci.py publish vX.Y.Z <run Windows> <run macOS>` — tạo Release từ artifact của hai lần chạy (nháp → đủ file → công bố).
- Bản macOS ký **ad-hoc** (không có tài khoản Apple Developer): người dùng phải *Open Anyway* lần đầu, hoặc `xattr -dr com.apple.quarantine /Applications/MRM.app`.
  Bản Windows chưa ký số: SmartScreen cảnh báo → *More info → Run anyway*.

## 6. Trạng thái hiện tại

### Đã phát hành (v0.12.1)

- Quét library theo chế độ Tự động, ghép ZIP ↔ thư mục, tag có nguồn gốc (tay / scan / rule / AI / folder), Rules, Dọn dẹp trùng lặp, Collections.
- Media Browser: duyệt theo thư mục, thư mục yêu thích, waveform dưới tên file, kéo thả sang app dựng, menu chuột phải.
- Xem bên trong ZIP / 7Z / RAR (RAR có mật khẩu: liệt kê được, giải nén báo lỗi mật khẩu).
- AI offline tùy chọn (bge-m3, qwen2.5:7b; AI nhìn ảnh gemma3:4b mặc định tắt).
- Đổi đường dẫn library giữ nguyên tag; file chuyển giữa các library được nối lại; hộp thoại cảnh báo khi gỡ library.
- Lọc All Resources theo kiểu nguồn; gán thư mục cho ứng dụng (rule `folder_app`).
- Tự cập nhật, file chẩn đoán (Settings → Báo lỗi; không chứa đường dẫn hay tên file), hướng dẫn lần đầu.
- Plugin: chế độ lai, thư mục yêu thích dùng chung với MRM, hộp thoại hướng dẫn khi gỡ thư mục, bản macOS.

### Chưa kiểm chứng trên máy thật

- **Toàn bộ bản macOS** (MRM lẫn plugin) mới qua build + test tự động trên máy Mac của GitHub — chưa ai trong nhóm phát triển mở trên Mac thật.
  Riêng lỗi "bấm nghe không có tiếng" đã sửa ở 0.12.1 theo phân tích code; cần người dùng Mac xác nhận.
  Nên thử: phát âm thanh, kéo thả sang app khác, AI offline (ollama-darwin), proxy video (VideoToolbox), plugin kết nối Resolve qua `install_mac.sh`.
- Hộp thoại gỡ thư mục của plugin mới thử trên trang giả lập, chưa bấm trong Resolve.

### Việc tồn đọng / ý tưởng đã bàn

- Gói tài nguyên (resource) bị chuyển thư mục: hiện thành mục mới, mục cũ "không có sẵn" — phải **Gộp** tay để chuyển tag / ghi chú. Có thể làm tự gợi ý gộp.
- RAR có mật khẩu: chưa có chỗ nhập mật khẩu.
- Waveform trên danh sách làm mỗi dòng cao hơn (~12 dòng / màn hình); có thể thêm tùy chọn gọn.
- `docs/DEVELOPMENT.md` và `plugins/HNH_SFX_Finder_MRM/CHANGELOG.md` chưa cập nhật theo các bản gần đây
  (lịch sử plugin hiển thị trong app lấy từ `release-history.json`). `package-lock.json` còn ghi phiên bản gốc cũ — không ảnh hưởng build.
- GitHub Actions cảnh báo Node 20 hết hạn cho `actions/checkout@v4`, `setup-node@v4`, `upload-artifact@v4` — nên nâng lên bản mới.
- Lịch sử git đã được viết lại một lần để xóa thông tin cá nhân; các commit cũ có thể còn trong cache của GitHub cho tới khi GitHub Support dọn.
- Chưa có chứng chỉ ký số cho Windows và macOS.

## 7. Những chỗ dễ sai (đã từng dính)

- **Dấu phân cách đường dẫn.** Backend dùng `std::path::MAIN_SEPARATOR` (`SEP` trong `assets.rs`); frontend `joinPath` trong `MediaBrowser.tsx`;
  plugin `full()` trong `mrm-store.js`. Lỗi âm thanh trên Mac ở 0.12.0 là do một chỗ ghép cứng `\`. Rule so khớp đường dẫn ở dạng chữ thường + `/`.
- **So khớp bỏ dấu tiếng Việt** dễ nhầm tên người với từ thường ("Dương" ≠ "đường") — so có dấu khi cần chính xác.
- **Migration:** thêm khối `if version < N` trong `db.rs`, tăng `SCHEMA_VERSION`, và cập nhật test `restore_old_backup_then_migrate`
  (test hạ một DB mới về schema v4 — cột / bảng mới phải được bỏ trong đó).
- **Khóa database:** không gọi hàm tự khóa `state.db` (đọc setting, trạng thái AI…) khi đang giữ khóa — đã từng treo app.
- **Mọi lần gắn tag** đi qua `autotag::insert_auto` / `insert_manual` / `remove_and_reject`, để tag đã bị người dùng gỡ không bị gắn lại.
- **Plugin và MRM cùng ghi một DB:** MRM tạm dừng việc nặng khi plugin đang mở (`presence.rs`); plugin chỉ tải lại khi `settings.media_rev` đổi.
  Plugin chạy bằng `node:sqlite` của Resolve — **không có FTS5**.
- **Gỡ library trong chế độ dùng chung** chỉ làm trong MRM (có backup tự động trước khi gỡ); plugin chỉ hướng dẫn.
- **Thông báo lỗi cho người dùng**: không nuốt lỗi (`.catch(() => undefined)`) ở thao tác người dùng bấm — lỗi im lặng rất khó chẩn đoán từ xa.
- **Bash trên Windows (Git Bash) làm hỏng dấu `\` trong heredoc.** Khi cần vá file có `\`, viết script ra file rồi chạy, đừng nhúng trong lệnh shell.

## 8. Khi người dùng báo lỗi

1. Xin **file chẩn đoán** (MRM: Settings → Báo lỗi; plugin: Cài đặt → xuất chẩn đoán). File chỉ có số đếm, phiên bản, trạng thái và lỗi gần đây đã che đường dẫn.
2. Tái hiện trên bản sao dữ liệu (mục 4). Thư mục `corrupt-<thời gian>` trong thư mục dữ liệu là DB hỏng đã được app tự cất lại khi phục hồi.
3. Sửa → test → phát hành bản vá (tăng số cuối). Góp ý của người dùng được gom trong một Google Doc do chủ dự án quản lý.
