import { useEffect, useState } from "react";
import { EXAMPLE_ROOT, FILE_MANAGER } from "../format";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Icon } from "./Icon";

const DEV_NAME = "Phạm Nam";
const DEV_FACEBOOK = "https://www.facebook.com/phamnam22s/";

const FEATURES: { icon: string; title: string; text: string }[] = [
  { icon: "hdd", title: "Thư viện nhiều ổ đĩa", text: "Gom plugin, LUT, SFX, template, preset, footage từ nhiều thư mục/ổ vào một nơi. Ổ ngoài rút ra vẫn xem được metadata." },
  { icon: "link", title: "Ghép ZIP + folder", text: "Nhận ra file nén và folder đã giải nén là cùng một gói — theo tên và theo nội dung bên trong." },
  { icon: "tag", title: "Tag & phân loại", text: "Application, Resource Type, Function, Personal tag và trạng thái cài đặt. Gợi ý tự động từ nội dung file." },
  { icon: "search", title: "Tìm kiếm tức thì", text: "Tìm theo tên, đường dẫn, tag, ghi chú, kèm bộ lọc app:, type:, tag:, is:fav…" },
  { icon: "eye", title: "Xem trước", text: "Ảnh bìa tự động, xem ảnh, video (kể cả ProRes), nghe audio có waveform, đọc text/PDF, duyệt bên trong ZIP." },
  { icon: "broom", title: "Dọn dẹp", text: "Phát hiện tài nguyên trùng, gom các phiên bản của cùng một gói, kiểm tra “đã có chưa” trước khi tải." },
  { icon: "rule", title: "Tự động hóa", text: "Theo dõi thư mục, tự quét khi có file mới hoặc cắm lại ổ ngoài. Rule tự gắn tag." },
  { icon: "sparkles", title: "AI chạy offline", text: "Tìm theo ý nghĩa (hiểu tiếng Việt), gợi ý tag, AI đọc README để viết mô tả — không gửi dữ liệu ra ngoài." },
  { icon: "music", title: "Media Browser", text: "Duyệt từng file âm thanh, ảnh, video trong mọi gói; nghe thử, xem waveform, kéo thả thẳng vào Premiere, CapCut, After Effects…" },
  { icon: "check", title: "An toàn tuyệt đối", text: "MRM chỉ đọc file để lập chỉ mục. Không bao giờ tự di chuyển, đổi tên hay xóa file thật của bạn." },
];

const GUIDE: { id: string; title: string; steps: React.ReactNode[] }[] = [
  {
    id: "start",
    title: "1. Bắt đầu: thêm thư viện",
    steps: [
      <>Vào <b>Libraries & Settings</b> → <b>Thêm Library</b>, chọn thư mục chứa tài nguyên (vd: <code>{EXAMPLE_ROOT}</code>). MRM tự quét ngay.</>,
      <>Mặc định <b>Cách quét: Tự động</b>: MRM tự nhận ra đâu là thư mục phân loại (vd: <code>Luts</code>, <code>Plug in</code>) để đi vào trong, đâu là một gói tài nguyên — kể cả gói bọc nhiều lớp <code>Gói\Gói\…</code> hay bundle có thư mục Help/Fonts.</>,
      <>Nhận sai? Trong Inspector → Sources bấm <b>Tách nhỏ</b> (thư mục chứa nhiều gói) hoặc <b>Gộp thư mục cha</b> (một gói bị chia vụn). MRM ghi nhớ lựa chọn và giữ lại tag/notes khi nhóm lại.</>,
      <>Tên thư mục chứa được dùng để gợi ý phân loại: gói trong <code>Luts</code> → LUT, trong <code>DR plugin</code> → DaVinci Resolve, trong <code>Preset LR</code> → Lightroom.</>,
      <>Có thể thêm nhiều library ở nhiều ổ. Bấm <b>Quét</b> trên thanh công cụ để quét lại tất cả.</>,
    ],
  },
  {
    id: "browse",
    title: "2. Duyệt & chọn",
    steps: [
      <>Chuyển giữa <b>Grid</b> (có ảnh bìa, rê chuột lên video để tua nhanh) và <b>List</b> (nhiều cột, hợp với thư viện lớn).</>,
      <>Click để chọn, <kbd>Ctrl</kbd>+click chọn nhiều, <kbd>Shift</kbd>+click chọn một dải, <kbd>Ctrl</kbd>+<kbd>A</kbd> chọn hết.</>,
      <>Khi chọn nhiều, panel bên phải cho phép gắn tag hàng loạt, Favorite, Gộp, hoặc Phân tích AI.</>,
    ],
  },
  {
    id: "classify",
    title: "3. Phân loại & ghi chú",
    steps: [
      <>Trong <b>Inspector</b> (cột phải): gắn Application, Resource Type, Function tag, Personal tag và Status bằng nút <b>+ Thêm</b> — gõ tên mới để tạo tag.</>,
      <>Mục <b>Gợi ý</b> hiện tag đề xuất từ nội dung file, từ Rules, từ thói quen gắn tag của bạn và từ AI. Bấm để áp dụng, hoặc <b>Áp dụng tất cả</b>.</>,
      <>Ghi <b>Notes</b>: cái này là gì, dùng làm gì, cài thế nào. Notes tự lưu và được tìm kiếm.</>,
      <>Thêm tối đa <b>3 ảnh minh họa</b> vào Notes: dán ảnh (<kbd>Ctrl</kbd>+<kbd>V</kbd>), kéo thả file ảnh vào cửa sổ, bấm <b>Thêm ảnh</b>, hoặc trong cửa sổ xem trước bấm <b>Thêm vào Notes</b> để lấy ảnh/khung hình ngay trong gói. Ảnh đầu tiên tự làm ảnh bìa.</>,
      <>Mở <b>Unclassified</b> để xử lý dần các mục chưa phân loại; nhấn <kbd>N</kbd> để sang mục tiếp theo.</>,
      <>Gắn nhanh cho cả thư viện: nút <b>Áp dụng gợi ý</b> trên thanh công cụ (hoặc cho các mục đang chọn). Tag do máy gắn có <b>viền đứt</b>, rê chuột để xem lý do; mục <b>Cần rà soát</b> gom các resource có tag tự động — bấm ✓ để xác nhận, ✕ để gỡ (MRM nhớ và không gắn lại). Tìm nhanh bằng <code>is:auto</code>.</>,
    ],
  },
  {
    id: "match",
    title: "4. Ghép ZIP và folder",
    steps: [
      <>File nén và folder giống nhau ≥95% (cùng thư mục) hoặc có cùng nội dung được <b>tự ghép</b> thành một resource nhiều nguồn.</>,
      <>Cặp giống 75–94% nằm ở <b>Match suggestions</b>: bấm <b>Ghép</b> hoặc <b>Không phải</b>.</>,
      <>Ghép nhầm? Trong Inspector → Sources, bấm <b>Tách</b> ở nguồn đó. Muốn gộp thủ công: chọn nhiều resource → <b>Gộp</b>.</>,
    ],
  },
  {
    id: "search",
    title: "5. Tìm kiếm",
    steps: [
      <><kbd>Ctrl</kbd>+<kbd>F</kbd> rồi gõ tự nhiên, vd: <code>light leak resolve</code>. Khi bật AI, có thể tìm theo ý nghĩa: <code>chữ phát sáng</code>, <code>âm thanh vút gió</code>.</>,
      <>Bộ lọc: <code>app:resolve</code> · <code>type:plugin</code> · <code>type:"stock video"</code> · <code>tag:tracking</code> · <code>status:installed</code> · <code>lib:tên-library</code></>,
      <>Lọc nhanh: <code>is:fav</code> · <code>is:missing</code> · <code>is:matched</code> · <code>is:unclassified</code>. Có thể kết hợp nhiều bộ lọc với từ khóa.</>,
    ],
  },
  {
    id: "preview",
    title: "6. Xem trước",
    steps: [
      <>Trong Inspector, mở <b>Nội dung bên trong</b> để duyệt file của folder/ZIP; click một file để xem. Dùng <kbd>←</kbd> <kbd>→</kbd> để chuyển file, <kbd>Esc</kbd> để đóng.</>,
      <>Video ProRes/DNxHR/HEVC được tự tạo bản xem thử 720p (dùng GPU nếu có). Audio có waveform — click vào waveform để tua.</>,
      <>Muốn đổi ảnh bìa: mở ảnh/video trong cửa sổ xem trước → <b>Đặt làm ảnh bìa</b>.</>,
      <>Bấm <b>Mở vị trí</b> để mở thư mục trong {FILE_MANAGER}, hoặc <b>Copy path</b>.</>,
    ],
  },
  {
    id: "cleanup",
    title: "7. Dọn dẹp thư viện",
    steps: [
      <>Mục <b>Dọn dẹp</b> → <b>Trùng lặp</b>: các resource có nội dung giống hệt hoặc cùng tên + phiên bản. Chọn <b>Gộp làm một</b> hoặc <b>Không phải trùng</b>.</>,
      <><b>Phiên bản</b>: các bản v1.0, v2.0… của cùng gói; <b>Đánh dấu bản cũ</b> để gắn status “Update Available”.</>,
      <><b>Đã có chưa?</b>: dán tên gói định tải để biết thư viện đã có thứ tương tự chưa.</>,
      <>MRM không tự xóa file — muốn xóa bản thừa, dùng <b>Mở vị trí</b> rồi xóa trong {FILE_MANAGER}.</>,
    ],
  },
  {
    id: "rules",
    title: "8. Rules tự gắn tag",
    steps: [
      <>Mục <b>Rules</b> → <b>Rule mới</b>: đặt điều kiện (tên, đường dẫn, loại file, library) và chọn tag sẽ gắn.</>,
      <>Chế độ <b>Gợi ý</b> chỉ hiện đề xuất; chế độ <b>Tự động</b> gắn tag ngay khi quét. Ô xem trước cho biết rule khớp bao nhiêu resource.</>,
      <>Bấm <b>Áp dụng ngay</b> để chạy rule cho toàn bộ thư viện hiện có.</>,
    ],
  },
  {
    id: "ai",
    title: "9. AI offline",
    steps: [
      <>Vào <b>Libraries & Settings</b> → <b>AI local</b> → <b>Cài đặt & bật AI</b>. Lần đầu tải ~7 GB (runtime + 2 model) vào thư mục lưu trữ — nên chọn ổ còn nhiều dung lượng.</>,
      <>Sau khi bật: tìm kiếm hiểu ý nghĩa, Inspector có gợi ý tag từ AI và danh sách <b>Tương tự</b>.</>,
      <>Bấm <b>Phân tích</b> trong Inspector (hoặc <b>Phân tích AI</b> ở mục Unclassified) để AI đọc cấu trúc file & README và đề xuất mô tả, tag, cách cài. Bạn luôn xác nhận trước khi áp dụng.</>,
      <>Khi chơi game hoặc dựng nặng, bấm <b>Tắt AI</b> để giải phóng VRAM.</>,
    ],
  },
  {
    id: "drives",
    title: "10. Ổ ngoài, theo dõi & sao lưu",
    steps: [
      <>Rút ổ ngoài: resource vẫn còn, nguồn được đánh dấu <b>offline</b>. Cắm lại (kể cả khác ký tự ổ) → MRM tự nhận lại và quét.</>,
      <><b>Theo dõi thư mục</b> (bật mặc định): thêm/sửa file trong library thì vài giây sau MRM tự cập nhật.</>,
      <>Database tự backup mỗi ngày (giữ 10 bản). Có thể <b>Backup ngay</b>, <b>Khôi phục</b>, <b>Export JSON</b> trong Settings.</>,
      <><b>Duyệt theo thư mục</b>: trong Media Browser bấm nút thư mục trên thanh công cụ để hiện cây thư mục — bấm một thư mục để chỉ xem file trong đó (gồm thư mục con).</>,
      <><b>Thư mục yêu thích</b>: bấm ☆ bên cạnh thư mục trong cây thư mục để ghim — thư mục ghim hiện ở đầu cây và ở thanh bên (dưới Media yêu thích).</>,
      <><b>Chuột phải</b> vào resource hoặc file media để mở thư mục chứa, sao chép đường dẫn, yêu thích, xem cả thư mục…</>,
      <><b>Collections</b> (thanh bên): tự nhóm resource và file media theo ý bạn — vd “SFX vlog”. Thêm từ Inspector (ô <i>Thêm vào collection</i>) hoặc chọn nhiều mục cùng lúc.</>,
      <><b>Ảnh bìa</b>: bấm vào ảnh bìa rồi dán ảnh (<kbd>Ctrl</kbd>+<kbd>V</kbd>) hoặc bấm <b>Đổi ảnh bìa</b>; trong Media Browser bấm <b>Đặt làm ảnh bìa</b> trên một ảnh/video.</>,
      <>Xem được bên trong <b>ZIP, RAR, 7Z</b>. File nén có mật khẩu chỉ xem được danh sách file.</>,
      <>Gặp lỗi: Libraries & Settings → <b>Báo lỗi</b> → <b>Xuất chẩn đoán</b>, gửi file cho nhà phát triển (không chứa đường dẫn hay ghi chú của bạn).</>,
      <><b>Loại trừ khi quét</b> (Libraries & Settings): bỏ qua thư mục rác như cache của app (<code>User Data/Cache</code>), <code>node_modules</code>… Mỗi dòng một luật, có sẵn danh sách mặc định.</>,
      <><b>Tự cập nhật</b>: MRM tự kiểm tra bản mới trên GitHub (1 lần/ngày, tắt được) và báo ở đầu app. Bấm <b>Cập nhật ngay</b> — tải, kiểm tra mã SHA-256 rồi cài đè, dữ liệu giữ nguyên.</>,
      <>Khi khởi động, MRM tự kiểm tra database. Nếu phát hiện hỏng, MRM giữ lại bản hỏng và tự khôi phục từ bản backup tốt gần nhất, rồi báo cho bạn.</>,
    ],
  },
  {
    id: "media",
    title: "11. Media Browser & plugin DaVinci",
    steps: [
      <>Mục <b>Media Browser</b> ở thanh bên liệt kê từng file <b>Âm thanh</b>, <b>Hình ảnh</b>, <b>Video</b> bên trong các gói. Chuỗi ảnh PNG (frame_0001…) được gom thành một mục.</>,
      <>Chọn file để xem trước: waveform cho âm thanh (bật <b>Tự nghe</b> để nghe ngay khi chọn), ảnh gốc, video (ProRes/HEVC tự tạo bản xem thử bằng GPU). Phím <kbd>↑</kbd> <kbd>↓</kbd> để duyệt, <kbd>Space</kbd> để nghe / dừng.</>,
      <><b>Kéo file</b> (hoặc nút <b>Kéo vào app dựng</b>) thả thẳng vào timeline / bin của Premiere, CapCut, After Effects, {FILE_MANAGER}… Chọn nhiều file bằng <kbd>Ctrl</kbd>/<kbd>Shift</kbd> để kéo cùng lúc. App đích chỉ sao chép hoặc tham chiếu — file gốc giữ nguyên.</>,
      <>Gắn <b>tag</b> và <b>★ yêu thích</b> cho từng file; dữ liệu dùng chung với plugin <b>HNH SFX Finder</b> trong DaVinci Resolve. Mục <b>Dùng gần đây</b> ghi lại file bạn đã kéo / sao chép.</>,
      <>Inspector của một resource có nút <b>File media</b> (vd: 1.622 âm thanh) để mở Media Browser chỉ với file của gói đó; ngược lại, file media có liên kết <b>Thuộc resource</b>.</>,
      <><b>AI cho file media</b> (khi bật AI offline): gõ theo ý nghĩa bằng tiếng Việt — vd <i>tiếng súng</i>, <i>vỗ tay</i>, <i>chuyển cảnh nhanh</i> — MRM tìm cả file không chứa đúng chữ đó (AK47.wav, Shotgun Pump…), sắp theo mức <b>Liên quan</b>.</>,
      <>Bộ lọc <b>✦ Danh mục AI</b>: AI tự nhận diện Whoosh, Impact, Glitch, Gun, Light leak, Film grain… từ tên file và thư mục (chỉ gán khi đủ chắc chắn). Bấm <b>Gắn tag “…”</b> để biến cả danh mục thành tag thật, hoặc <b>Gắn thành tag</b> cho từng file.</>,
      <><b>AI nhìn ảnh/video</b> (tùy chọn, mặc định tắt — bật trong Libraries & Settings → AI): AI xem từng ảnh và khung giữa của video rồi viết một câu mô tả, để tìm theo nội dung hình như <i>cô gái tóc dài</i>, <i>nền trời xanh</i>. Chạy nền ~1 giây/ảnh, có nút <b>Tạm dừng</b>, tự dừng khi plugin DaVinci mở; tắt là nhả VRAM ngay. Chỉ dùng trong MRM.</>,
      <>Inspector có mục <b>Tương tự</b>: các file giống file đang chọn — bấm để nghe / xem ngay.</>,
      <>Khi plugin đang mở trong DaVinci, Media Browser của MRM tạm khóa (<b>Đang được sử dụng trong DaVinci</b>), MRM tạm dừng tác vụ nặng và nhả VRAM cho Resolve. Đóng plugin là MRM tự mở lại.</>,
    ],
  },
];

const SHORTCUTS: [string, string][] = [
  ["Ctrl + F", "Tìm kiếm"],
  ["↑ / ↓", "Chọn mục trên / dưới (giữ Shift để chọn dải)"],
  ["Ctrl + A", "Chọn tất cả"],
  ["F", "Favorite / bỏ Favorite"],
  ["N", "Mục chưa phân loại tiếp theo"],
  ["Esc", "Bỏ chọn / đóng cửa sổ xem trước"],
  ["← / →", "Chuyển file trong cửa sổ xem trước"],
];

export function About({ toast, onShowOnboarding }: { toast: (msg: string, kind?: "ok" | "err") => void; onShowOnboarding?: () => void }) {
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion().then(setVersion).catch(() => undefined);
  }, []);

  return (
    <div className="page about">
      <section className="about-hero">
        <div className="brand-mark large">M</div>
        <div>
          <h1>MRM — Media Resource Manager</h1>
          <p className="muted">Quản lý tài nguyên dựng video: biết mình đang có gì, tìm ra ngay khi cần.{version && ` · Phiên bản ${version}`}</p>
        </div>
        {onShowOnboarding && (
          <button className="btn" style={{ marginLeft: "auto" }} onClick={onShowOnboarding}>
            <Icon name="sparkles" size={14} /> Xem lại hướng dẫn nhanh
          </button>
        )}
      </section>

      <section className="panel dev-card">
        <div className="dev-avatar">PN</div>
        <div className="dev-info">
          <div className="muted small">Nhà phát triển</div>
          <div className="dev-name">{DEV_NAME}</div>
          <div className="dev-contact">
            <span className="fb-badge">Facebook</span> facebook.com/phamnam22s
          </div>
        </div>
        <div className="row-actions">
          <button className="btn primary" onClick={() => openUrl(DEV_FACEBOOK).catch(() => toast("Không mở được Facebook", "err"))}>
            Nhắn qua Facebook
          </button>
          <button
            className="btn"
            onClick={() => {
              navigator.clipboard.writeText(DEV_FACEBOOK);
              toast("Đã copy link Facebook");
            }}
          >
            <Icon name="copy" size={14} /> Copy link
          </button>
        </div>
      </section>

      <h2 className="section-title">Tính năng</h2>
      <div className="feature-grid">
        {FEATURES.map((f) => (
          <div key={f.title} className="feature">
            <span className="feature-icon">
              <Icon name={f.icon} size={18} />
            </span>
            <div>
              <div className="feature-title">{f.title}</div>
              <div className="feature-text">{f.text}</div>
            </div>
          </div>
        ))}
      </div>

      <h2 className="section-title">Hướng dẫn sử dụng</h2>
      <nav className="guide-toc">
        {GUIDE.map((g) => (
          <button key={g.id} onClick={() => document.getElementById(`guide-${g.id}`)?.scrollIntoView({ behavior: "smooth", block: "start" })}>
            {g.title}
          </button>
        ))}
      </nav>
      {GUIDE.map((g) => (
        <section key={g.id} id={`guide-${g.id}`} className="panel guide">
          <h3>{g.title}</h3>
          <ul>
            {g.steps.map((s, i) => (
              <li key={i}>{s}</li>
            ))}
          </ul>
        </section>
      ))}

      <section className="panel guide">
        <h3>Phím tắt</h3>
        <table className="table">
          <tbody>
            {SHORTCUTS.map(([k, v]) => (
              <tr key={k}>
                <td style={{ width: 160 }}>
                  <kbd>{k}</kbd>
                </td>
                <td>{v}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <p className="muted small about-foot">
        © {new Date().getFullYear()} {DEV_NAME} · MRM chạy hoàn toàn trên máy của bạn · Góp ý & báo lỗi qua Facebook Phạm Nam
      </p>
    </div>
  );
}
