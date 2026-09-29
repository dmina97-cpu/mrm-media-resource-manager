import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { AiStatus, api, CacheStats, StorageInfo, VisionStatus } from "../api";
import { errorText, formatNumber, formatSize } from "../format";
import { Icon } from "./Icon";

interface Props {
  ai: AiStatus | null;
  refreshAi: () => void;
  toast: (msg: string, kind?: "ok" | "err") => void;
}

/** Thư mục lưu dữ liệu lớn + cache preview + tự theo dõi thư mục. */
export function StorageSettings({ toast }: { toast: Props["toast"] }) {
  const [storage, setStorage] = useState<StorageInfo | null>(null);
  const [cache, setCache] = useState<CacheStats | null>(null);
  const [watch, setWatch] = useState<boolean | null>(null);
  const [autoTag, setAutoTag] = useState<boolean | null>(null);
  const load = () => {
    api.getStorageInfo().then(setStorage);
    api.cacheStats().then(setCache);
    api.getAutoWatch().then(setWatch);
    api.getPref("auto_tag_new").then(setAutoTag);
  };
  useEffect(load, []);

  return (
    <section className="panel">
      <h2>Lưu trữ, cache & tự động</h2>
      <div className="kv">
        <div>
          <span>Thư mục dữ liệu lớn</span>
          <div>
            <code>{storage?.dir}</code>
            <span className="muted small"> · còn trống {storage?.free_bytes != null ? formatSize(storage.free_bytes) : "?"}</span>
            <div className="muted small">Chứa cache preview (thumbnail, proxy video) và AI runtime + model (~8–10 GB).</div>
          </div>
          <button
            className="btn tiny"
            onClick={async () => {
              const dir = await open({ directory: true, title: "Chọn thư mục lưu cache & AI (nên ở ổ còn nhiều dung lượng)" });
              if (typeof dir !== "string") return;
              try {
                await api.setStorageDir(dir);
                toast("Đã đổi thư mục lưu trữ. Nếu đã cài AI ở thư mục cũ, hãy copy thư mục “ai” sang hoặc cài lại.");
                load();
              } catch (err) {
                toast(errorText(err), "err");
              }
            }}
          >
            Đổi…
          </button>
        </div>
        <div>
          <span>Cache preview</span>
          <div>
            {cache ? formatSize(cache.size) : "…"} / giới hạn{" "}
            <select
              value={cache?.limit_mb ?? 4096}
              onChange={async (e) => {
                await api.setCacheLimit(Number(e.target.value));
                load();
              }}
            >
              {[1024, 2048, 4096, 8192, 16384, 32768].map((mb) => (
                <option key={mb} value={mb}>{mb / 1024} GB</option>
              ))}
            </select>
            <div className="muted small">Tự dọn file ít dùng nhất khi vượt giới hạn. Không ảnh hưởng file tài nguyên.</div>
          </div>
          <button
            className="btn tiny"
            onClick={async () => {
              await api.clearCache();
              toast("Đã xóa cache preview");
              load();
            }}
          >
            Xóa cache
          </button>
        </div>
        <div>
          <span>Theo dõi thư mục</span>
          <div>
            <label className="check-label">
              <input
                type="checkbox"
                checked={!!watch}
                onChange={async (e) => {
                  await api.setAutoWatch(e.target.checked);
                  setWatch(e.target.checked);
                }}
              />
              Tự quét lại khi có file mới/thay đổi và khi ổ ngoài được cắm lại
            </label>
          </div>
        </div>
        <div>
          <span>Tự gắn tag</span>
          <div>
            <label className="check-label">
              <input
                type="checkbox"
                checked={!!autoTag}
                onChange={async (e) => {
                  await api.setPref("auto_tag_new", e.target.checked);
                  setAutoTag(e.target.checked);
                }}
              />
              Resource mới quét được tự gắn App/Type từ nội dung file, tên thư mục và Rules (đánh dấu “tự động”, xem lại ở mục Cần rà soát)
            </label>
          </div>
        </div>
      </div>
    </section>
  );
}

export function AiSettings({ ai, refreshAi, toast }: Props) {
  const [embed, setEmbed] = useState("");
  const [chat, setChat] = useState("");
  const [autoApply, setAutoApply] = useState<boolean | null>(null);
  useEffect(() => {
    api.aiGetAutoApply().then(setAutoApply).catch(() => undefined);
  }, []);
  useEffect(() => {
    if (ai) {
      setEmbed((v) => v || ai.embed_model);
      setChat((v) => v || ai.chat_model);
    }
  }, [ai]);
  if (!ai) return null;

  const p = ai.progress;
  const working = ai.busy || ["downloading", "extracting", "starting", "pulling"].includes(p.phase);
  const pct = p.total ? Math.round((p.done / p.total) * 100) : null;
  const ready = ai.enabled && ai.running && ai.embed_ready;

  return (
    <section className="panel ai-panel">
      <h2>
        <Icon name="sparkles" size={14} /> AI local (Phase 5)
      </h2>
      <p className="muted small">
        Chạy hoàn toàn offline trên máy qua Ollama tích hợp — không gửi dữ liệu ra ngoài. Semantic search (tìm theo ý nghĩa, hiểu cả tiếng Việt/Anh), gợi ý tag và phân
        tích README/cấu trúc folder để đề xuất mô tả. AI chỉ đề xuất, bạn luôn là người xác nhận.
      </p>

      <div className="kv">
        <div>
          <span>GPU</span>
          <div>{ai.gpu ?? <span className="muted">Không phát hiện GPU NVIDIA — AI vẫn chạy bằng CPU nhưng chậm hơn</span>}</div>
        </div>
        <div>
          <span>Trạng thái</span>
          <div>
            {!ai.installed ? (
              <span className="muted">Chưa cài</span>
            ) : ai.running ? (
              <span className="ok-text">● Đang chạy (cổng 11435)</span>
            ) : (
              <span className="muted">Đã cài, đang tắt</span>
            )}
            {ai.running && (
              <span className="muted small">
                {" "}
                · {ai.embed_model} {ai.embed_ready ? "✓" : "✗"} · {ai.chat_model} {ai.chat_ready ? "✓" : "✗"}
              </span>
            )}
          </div>
        </div>
        <div>
          <span>Chỉ mục</span>
          <div>
            {formatNumber(ai.indexed)} / {formatNumber(ai.total)} resource
            {ai.enabled && ai.indexed < ai.total && ai.running && <span className="muted small"> · đang lập chỉ mục nền</span>}
          </div>
        </div>
        <div>
          <span>Lưu tại</span>
          <div>
            <code>{ai.storage_dir}</code>
            <span className="muted small"> · còn trống {ai.free_bytes != null ? formatSize(ai.free_bytes) : "?"}</span>
          </div>
        </div>
      </div>

      {(working || p.phase === "error") && (
        <div className={`ai-progress ${p.phase === "error" ? "err" : ""}`}>
          {p.phase !== "error" && <span className="spinner" />}
          <span>{p.message}</span>
          {pct !== null && p.phase !== "error" && (
            <>
              <span className="progress-track wide">
                <span className="progress-fill" style={{ width: `${pct}%` }} />
              </span>
              <span className="muted small">
                {formatSize(p.done)} / {formatSize(p.total)}
              </span>
            </>
          )}
        </div>
      )}

      <div className="row-actions">
        {!ready && (
          <button
            className="btn primary"
            disabled={working}
            onClick={async () => {
              const msg = ai.installed
                ? `Khởi động AI và tải model còn thiếu (${ai.embed_model} ~1.2 GB, ${ai.chat_model} ~4.7 GB)?`
                : `Tải AI runtime (~1.4 GB) và 2 model (${ai.embed_model} ~1.2 GB, ${ai.chat_model} ~4.7 GB) vào:\n${ai.storage_dir}\n\nTổng ~8–10 GB. Tiếp tục?`;
              if (!confirm(msg)) return;
              api.aiInstall().then(
                () => {
                  toast("AI đã sẵn sàng");
                  refreshAi();
                },
                (err) => {
                  toast(errorText(err), "err");
                  refreshAi();
                },
              );
              setTimeout(refreshAi, 500);
            }}
          >
            <Icon name="sparkles" size={14} /> {ai.installed ? "Bật AI" : "Cài đặt & bật AI"}
          </button>
        )}
        {ai.enabled && (
          <button
            className="btn"
            onClick={async () => {
              await api.aiSetEnabled(false);
              refreshAi();
            }}
          >
            Tắt AI (giải phóng VRAM)
          </button>
        )}
      </div>

      <label className="check-label ai-auto">
        <input
          type="checkbox"
          checked={!!autoApply}
          onChange={async (e) => {
            await api.aiSetAutoApply(e.target.checked);
            setAutoApply(e.target.checked);
          }}
        />
        Tự động áp dụng kết quả phân tích AI (tag + mô tả) — chưa chuẩn thì chỉnh lại sau, tìm lại bằng <code>is:ai</code>
      </label>

      {ai.enabled && <VisionSettings toast={toast} />}

      <details className="ai-advanced">
        <summary>Nâng cao: đổi model</summary>
        <div className="form-row">
          <label>Embedding</label>
          <input value={embed} onChange={(e) => setEmbed(e.target.value)} />
          <label>LLM</label>
          <input value={chat} onChange={(e) => setChat(e.target.value)} />
          <button
            className="btn tiny"
            onClick={async () => {
              await api.aiSetModels(embed, chat);
              toast("Đã lưu. Bấm “Bật AI” để tải model mới nếu chưa có.");
              refreshAi();
            }}
          >
            Lưu
          </button>
        </div>
        <p className="muted small">
          Mặc định: bge-m3 (đa ngôn ngữ) và qwen2.5:7b (vừa 8 GB VRAM của 3060 Ti). Đổi embedding model sẽ phải lập chỉ mục lại từ đầu.
        </p>
      </details>
    </section>
  );
}

/** AI nhìn ảnh/video — tùy chọn, mặc định tắt; chỉ dùng trong MRM (plugin DaVinci không chạy AI). */
export function VisionSettings({ toast }: { toast: Props["toast"] }) {
  const [v, setV] = useState<VisionStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const load = () => api.visionStatus().then(setV).catch(() => undefined);
  useEffect(() => {
    load();
    const t = setInterval(load, 4000);
    return () => clearInterval(t);
  }, []);
  if (!v) return null;
  const pct = v.total ? Math.round((v.done / v.total) * 100) : 0;
  const left = Math.max(0, v.total - v.done);
  return (
    <div className="vision-box">
      <label className="check-label">
        <input
          type="checkbox"
          checked={v.enabled}
          disabled={busy || v.installing}
          onChange={async (e) => {
            const on = e.target.checked;
            if (on && !v.installed && !confirm(`Tải model thị giác ${v.model} (~3.3 GB) vào thư mục AI để bật tính năng này?`)) return;
            setBusy(true);
            try {
              await api.visionSetEnabled(on);
              toast(on ? "Đã bật AI nhìn ảnh/video" : "Đã tắt AI nhìn ảnh/video (đã nhả VRAM)");
            } catch (err) {
              toast(errorText(err), "err");
            } finally {
              setBusy(false);
              load();
            }
          }}
        />
        <b>AI nhìn ảnh/video</b> <span className="muted small">({v.model})</span>
      </label>
      <p className="muted small">
        AI xem từng ảnh (và khung giữa của video) rồi viết một câu mô tả — để tìm theo nội dung hình như <i>cô gái tóc dài</i>, <i>nền trời xanh</i>,{" "}
        <i>light leak màu cam</i>. Chạy nền ~1 giây/ảnh, bỏ qua icon/ảnh nhỏ, tự dừng khi plugin DaVinci mở. Chỉ dùng trong MRM.
      </p>
      {(busy || v.installing) && (
        <div className="ai-progress">
          <span className="spinner" /> <span>Đang tải model {v.model}… (xem tiến độ ở thanh trạng thái)</span>
        </div>
      )}
      {v.enabled && (
        <div className="vision-progress">
          <span className="progress-track wide">
            <span className="progress-fill" style={{ width: `${pct}%` }} />
          </span>
          <span className="small">
            {formatNumber(v.done)} / {formatNumber(v.total)} ảnh/video
            {left > 0 && (v.paused ? " · đang tạm dừng" : v.running ? ` · còn ~${Math.ceil(left / 60)} phút` : " · đang chờ (AI bận hoặc plugin DaVinci đang mở)")}
          </span>
          {left > 0 && (
            <button
              className="btn tiny"
              onClick={async () => {
                await api.visionSetPaused(!v.paused);
                load();
              }}
            >
              {v.paused ? "Tiếp tục" : "Tạm dừng"}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
