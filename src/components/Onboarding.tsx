import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AiStatus, api, Library, ScanProgress } from "../api";
import { errorText, formatNumber } from "../format";
import { Icon } from "./Icon";
import { ExcludeSettings } from "./Updates";

interface Props {
  libraries: Library[];
  scanning: Set<number>;
  progress: Record<number, ScanProgress>;
  ai: AiStatus | null;
  onScan: (id: number, full: boolean) => void;
  onChanged: () => void;
  refreshAi: () => void;
  onOpenMedia: () => void;
  onClose: () => void;
  toast: (msg: string, kind?: "ok" | "err") => void;
}

const STEPS = ["Chào mừng", "Loại trừ", "Thư mục", "AI", "Xong"];

/** Hướng dẫn nhanh lần đầu mở MRM (có thể mở lại trong Giới thiệu & Hướng dẫn). */
export function Onboarding({ libraries, scanning, progress, ai, onScan, onChanged, refreshAi, onOpenMedia, onClose, toast }: Props) {
  const [step, setStep] = useState(0);
  const [installing, setInstalling] = useState(false);

  const finish = (then?: () => void) => {
    api.setPref("show_onboarding", false).catch(() => undefined);
    onClose();
    then?.();
  };

  const addFolder = async () => {
    try {
      const dir = await open({ directory: true, multiple: false, title: "Chọn thư mục chứa tài nguyên" });
      if (typeof dir !== "string") return;
      const lib = await api.addLibrary(dir);
      onChanged();
      onScan(lib.id, false);
    } catch (err) {
      toast(errorText(err), "err");
    }
  };

  const aiReady = !!ai && ai.enabled && ai.running && ai.embed_ready;

  return (
    <div className="modal-backdrop onboarding-backdrop">
      <div className="onboarding">
        <div className="ob-steps">
          {STEPS.map((s, i) => (
            <span key={s} className={i === step ? "on" : i < step ? "done" : ""}>
              <b>{i < step ? "✓" : i + 1}</b> {s}
            </span>
          ))}
        </div>

        <div className="ob-body">
          {step === 0 && (
            <>
              <div className="ob-hero">
                <div className="brand-mark large">M</div>
                <div>
                  <h2>Chào mừng đến MRM</h2>
                  <p className="muted">Media Resource Manager — gom plugin, LUT, SFX, template, preset, footage về một chỗ.</p>
                </div>
              </div>
              <ul className="ob-points">
                <li>
                  <Icon name="check" size={16} /> <b>Chỉ đọc:</b> MRM không bao giờ di chuyển, đổi tên hay xóa file thật của bạn. Tag, ghi chú… chỉ nằm trong MRM.
                </li>
                <li>
                  <Icon name="music" size={16} /> <b>Media Browser:</b> nghe thử, xem trước, kéo thả thẳng file vào Premiere, CapCut, After Effects…
                </li>
                <li>
                  <Icon name="sparkles" size={16} /> <b>AI offline (tùy chọn):</b> tìm bằng tiếng Việt, tự nhận diện Whoosh, Impact, Light leak… — không gửi dữ liệu ra
                  ngoài.
                </li>
              </ul>
              <p className="muted small">3 bước ngắn: loại trừ thư mục rác → chọn thư mục tài nguyên → bật AI nếu muốn.</p>
            </>
          )}

          {step === 1 && (
            <>
              <h2>Bỏ qua thư mục rác</h2>
              <p>
                Thư mục tài nguyên thường lẫn cache của app (vd: CapCut sinh hàng nghìn ảnh trong <code>User Data/Cache</code>). MRM bỏ qua sẵn các thư mục dưới đây — bạn có
                thể thêm bớt, sau này chỉnh lại trong <b>Libraries & Settings</b>.
              </p>
              <ExcludeSettings toast={toast} compact />
            </>
          )}

          {step === 2 && (
            <>
              <h2>Chọn thư mục tài nguyên</h2>
              <p>
                Chọn thư mục gốc chứa tài nguyên (vd: <code>D:\Resources</code>). MRM tự nhận ra đâu là thư mục phân loại, đâu là một gói — kể cả ZIP và folder đã giải nén.
                Có thể thêm nhiều thư mục, ở nhiều ổ đĩa.
              </p>
              <div className="ob-libs">
                {libraries.map((l) => {
                  const p = progress[l.id];
                  const busy = scanning.has(l.id);
                  return (
                    <div key={l.id} className="ob-lib">
                      <Icon name="hdd" size={16} />
                      <div className="grow">
                        <div className="truncate">{l.name}</div>
                        <div className="muted small truncate">{l.path}</div>
                      </div>
                      <span className="small">
                        {busy ? (
                          <>
                            <span className="spinner" /> {p?.total ? `${formatNumber(p.done)}/${formatNumber(p.total)}` : "Đang quét…"}
                          </>
                        ) : (
                          <span className="ok-text">✓ {formatNumber(l.source_count)} nguồn</span>
                        )}
                      </span>
                    </div>
                  );
                })}
                <button className="btn primary" onClick={addFolder}>
                  <Icon name="plus" size={14} /> {libraries.length ? "Thêm thư mục khác" : "Chọn thư mục"}
                </button>
              </div>
              <p className="muted small">Quét chạy nền — bạn có thể sang bước tiếp theo ngay.</p>
            </>
          )}

          {step === 3 && (
            <>
              <h2>AI offline (tùy chọn)</h2>
              <p>
                Bật AI để <b>tìm theo ý nghĩa bằng tiếng Việt</b> (<i>tiếng súng</i>, <i>vỗ tay</i>, <i>chuyển cảnh nhanh</i>), tự nhận diện danh mục SFX / hình ảnh và gợi ý
                tag. AI chạy hoàn toàn trên máy, không cần mạng sau khi tải.
              </p>
              <div className="kv">
                <div>
                  <span>GPU</span>
                  <div>{ai?.gpu ?? <span className="muted">Không thấy GPU NVIDIA — AI vẫn chạy bằng CPU nhưng chậm</span>}</div>
                </div>
                <div>
                  <span>Cần</span>
                  <div>
                    ~8–10 GB ổ đĩa (runtime + 2 model), nên có GPU NVIDIA 6–8 GB VRAM và RAM 16 GB. Lưu tại: <code>{ai?.storage_dir ?? "…"}</code>
                  </div>
                </div>
              </div>
              <div className="row-actions">
                {aiReady ? (
                  <span className="ok-text">✓ AI đã bật</span>
                ) : (
                  <button
                    className="btn primary"
                    disabled={installing || !ai}
                    onClick={async () => {
                      if (!confirm("Tải AI runtime và 2 model (~8–10 GB) rồi bật AI? Có thể chạy nền, bạn vẫn dùng app bình thường.")) return;
                      setInstalling(true);
                      api.aiInstall().then(
                        () => {
                          toast("AI đã sẵn sàng");
                          refreshAi();
                        },
                        (err) => toast(errorText(err), "err"),
                      );
                      setTimeout(refreshAi, 500);
                    }}
                  >
                    <Icon name="sparkles" size={14} /> {installing ? "Đang tải (xem tiến độ ở thanh trạng thái)…" : "Tải & bật AI"}
                  </button>
                )}
                <span className="muted small">Hoặc để sau — bật bất cứ lúc nào trong Libraries & Settings → AI.</span>
              </div>
              <p className="muted small">
                <b>AI nhìn ảnh/video</b> (tìm theo nội dung hình, cần thêm ~3.3 GB) mặc định tắt — bật riêng trong Settings nếu cần.
              </p>
            </>
          )}

          {step === 4 && (
            <>
              <h2>Sẵn sàng!</h2>
              <ul className="ob-points">
                <li>
                  <Icon name="search" size={16} /> <kbd>Ctrl</kbd>+<kbd>F</kbd> để tìm — vd: <code>light leak app:resolve</code>
                </li>
                <li>
                  <Icon name="music" size={16} /> <b>Media Browser</b> (thanh bên): chọn file, <kbd>Space</kbd> để nghe, kéo thả vào app dựng.
                </li>
                <li>
                  <Icon name="tag" size={16} /> Chọn nhiều resource → gắn tag hàng loạt, hoặc <b>Áp dụng gợi ý</b> để tự phân loại.
                </li>
                <li>
                  <Icon name="film" size={16} /> Dùng DaVinci Resolve Studio? Cài thêm plugin <b>HNH SFX Finder</b> — tự dùng chung thư viện, tag với MRM.{" "}
                  <button className="link-btn" onClick={() => openUrl("https://github.com/dmina97-cpu/mrm-media-resource-manager/releases/latest")}>
                    Tải plugin
                  </button>
                </li>
              </ul>
              <p className="muted small">Xem lại hướng dẫn này trong Giới thiệu & Hướng dẫn.</p>
            </>
          )}
        </div>

        <div className="ob-foot">
          {step < 4 ? (
            <button className="link-btn" onClick={() => finish()}>
              Bỏ qua hướng dẫn
            </button>
          ) : (
            <span />
          )}
          <div className="row-actions">
            {step > 0 && (
              <button className="btn" onClick={() => setStep(step - 1)}>
                Quay lại
              </button>
            )}
            {step < 4 ? (
              <button className="btn primary" onClick={() => setStep(step + 1)}>
                {step === 2 && libraries.length === 0 ? "Để sau" : "Tiếp tục"} →
              </button>
            ) : (
              <>
                <button className="btn" onClick={() => finish()}>
                  Vào app
                </button>
                <button className="btn primary" onClick={() => finish(onOpenMedia)}>
                  <Icon name="music" size={14} /> Mở Media Browser
                </button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
