import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, UpdateInfo } from "../api";
import { errorText, formatDateTime, formatSize } from "../format";
import { Icon } from "./Icon";

type Toast = (msg: string, kind?: "ok" | "err") => void;

/** Tải bản mới + chạy bộ cài (MRM tự đóng để bộ cài ghi đè). */
function useInstaller(toast: Toast) {
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  useEffect(() => {
    const un = listen<{ done: number; total: number }>("update-progress", (e) => setProgress(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, []);
  const install = async (info: UpdateInfo) => {
    if (!info.latest) return;
    if (!confirm(`Tải MRM ${info.latest.version} (${formatSize(info.latest.size)}) và cài đè bản hiện tại?\n\nMRM sẽ tự đóng khi bộ cài chạy. Dữ liệu, tag, ghi chú giữ nguyên.`)) return;
    setProgress({ done: 0, total: info.latest.size });
    try {
      await api.updateInstall();
    } catch (err) {
      setProgress(null);
      toast(errorText(err), "err");
    }
  };
  return { progress, install };
}

function ProgressLine({ p }: { p: { done: number; total: number } }) {
  const pct = p.total ? Math.round((p.done / p.total) * 100) : 0;
  return (
    <span className="update-progress">
      <span className="spinner" /> {pct >= 100 ? "Đang mở bộ cài…" : `Đang tải ${formatSize(p.done)} / ${formatSize(p.total)}`}
      <span className="progress-track">
        <span className="progress-fill" style={{ width: `${pct}%` }} />
      </span>
    </span>
  );
}

/** Thanh báo bản mới ở đầu app (tự kiểm tra tối đa 1 lần/ngày, tắt được trong Settings). */
export function UpdateBanner({ toast }: { toast: Toast }) {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [hidden, setHidden] = useState(false);
  const { progress, install } = useInstaller(toast);
  useEffect(() => {
    const t = setTimeout(() => api.updateCheck(false).then(setInfo).catch(() => undefined), 4000);
    return () => clearTimeout(t);
  }, []);
  if (!info?.available || info.skipped || hidden || !info.latest) return null;
  const l = info.latest;
  return (
    <div className="update-banner">
      <Icon name="sparkles" size={15} />
      <span className="grow">
        Có bản mới <b>MRM {l.version}</b> (đang dùng {info.current}).
      </span>
      {progress ? (
        <ProgressLine p={progress} />
      ) : (
        <>
          <button className="btn primary tiny" onClick={() => install(info)}>
            Cập nhật ngay
          </button>
          <button className="btn tiny" onClick={() => openUrl(l.page_url)}>
            Có gì mới?
          </button>
          <button className="btn tiny" onClick={() => setHidden(true)} title="Nhắc lại lần mở app sau">
            Để sau
          </button>
          <button
            className="link-btn"
            onClick={() =>
              api.updateConfigure(null, l.version).then(() => {
                setHidden(true);
                toast(`Sẽ không nhắc bản ${l.version} nữa`);
              })
            }
          >
            Bỏ qua bản này
          </button>
        </>
      )}
    </div>
  );
}

/** Mục Cập nhật trong Settings. */
export function UpdateSettings({ toast }: { toast: Toast }) {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const { progress, install } = useInstaller(toast);
  useEffect(() => {
    api.updateCheck(false).then(setInfo).catch(() => undefined);
  }, []);
  const check = async () => {
    setChecking(true);
    try {
      const i = await api.updateCheck(true);
      setInfo(i);
      if (i.error) toast(i.error, "err");
      else toast(i.available ? `Có bản mới ${i.latest?.version}` : "Đang dùng bản mới nhất");
    } finally {
      setChecking(false);
    }
  };
  if (!info) return null;
  return (
    <section className="panel">
      <h2>
        <Icon name="refresh" size={14} /> Cập nhật
      </h2>
      <div className="kv">
        <div>
          <span>Phiên bản</span>
          <div>
            MRM <b>{info.current}</b>
            {info.available && info.latest ? (
              <span className="ok-text"> · có bản mới {info.latest.version}</span>
            ) : info.latest ? (
              <span className="muted small"> · đang dùng bản mới nhất</span>
            ) : null}
          </div>
        </div>
        <div>
          <span>Kiểm tra</span>
          <div>
            {info.checked_at ? formatDateTime(info.checked_at) : "Chưa kiểm tra"}
            {info.error && <span className="err-text small"> · {info.error}</span>}
          </div>
        </div>
      </div>
      <div className="row-actions">
        {info.available && info.latest && !progress && (
          <button className="btn primary" onClick={() => install(info)}>
            <Icon name="sparkles" size={14} /> Cập nhật lên {info.latest.version}
          </button>
        )}
        {progress && <ProgressLine p={progress} />}
        <button className="btn" onClick={check} disabled={checking}>
          <Icon name="refresh" size={14} /> {checking ? "Đang kiểm tra…" : "Kiểm tra ngay"}
        </button>
        <button className="btn" onClick={() => openUrl("https://github.com/dmina97-cpu/mrm-media-resource-manager/releases")}>
          <Icon name="link" size={14} /> Trang phát hành
        </button>
      </div>
      <label className="check-label">
        <input
          type="checkbox"
          checked={info.auto}
          onChange={(e) => api.updateConfigure(e.target.checked, null).then(setInfo)}
        />
        Tự kiểm tra bản mới (1 lần/ngày khi mở app)
      </label>
      {info.available && info.latest?.notes && (
        <details className="update-notes">
          <summary>Có gì mới trong {info.latest.version}</summary>
          <pre>{info.latest.notes}</pre>
        </details>
      )}
      <p className="muted small">
        Bộ cài tải từ GitHub của nhà phát triển, được kiểm tra mã SHA-256 trước khi chạy. Cài đè giữ nguyên toàn bộ dữ liệu. Plugin DaVinci tự kiểm tra cập nhật
        riêng trong plugin.
      </p>
    </section>
  );
}

/** Loại trừ thư mục khi quét. */
export function ExcludeSettings({ toast, onSaved, compact }: { toast: Toast; onSaved?: () => void; compact?: boolean }) {
  const [text, setText] = useState<string | null>(null);
  const [defaults, setDefaults] = useState<string[]>([]);
  const [dirty, setDirty] = useState(false);
  useEffect(() => {
    api.getScanExcludes().then((r) => {
      setText(r.rules.join("\n"));
      setDefaults(r.defaults);
    });
  }, []);
  if (text === null) return null;
  const save = async (quiet = false) => {
    try {
      await api.setScanExcludes(text.split("\n"));
      setDirty(false);
      if (!quiet) toast("Đã lưu danh sách loại trừ");
      onSaved?.();
    } catch (err) {
      toast(errorText(err), "err");
    }
  };
  const body = (
    <>
      <p className="muted small">
        Mỗi dòng một luật — thư mục/file khớp sẽ không được quét (cache của app, thư mục cài đặt…). Ví dụ: <code>node_modules</code>,{" "}
        <code>User Data/Cache</code> (hai cấp liên tiếp), <code>*.tmp</code>. Không phân biệt hoa thường. MRM không xóa gì: file đã lập chỉ mục trong thư mục bị loại
        trừ chỉ ẩn khỏi danh sách ở lần quét sau.
      </p>
      <textarea
        className="exclude-box"
        value={text}
        spellCheck={false}
        rows={Math.min(12, Math.max(6, text.split("\n").length + 1))}
        onChange={(e) => {
          setText(e.target.value);
          setDirty(true);
        }}
        // trong hướng dẫn lần đầu: tự lưu khi rời ô (bấm "Tiếp tục" không mất thay đổi)
        onBlur={() => compact && dirty && save(true)}
      />
      <div className="row-actions">
        <button className="btn primary" onClick={() => save()} disabled={!dirty}>
          Lưu
        </button>
        <button
          className="btn"
          onClick={() => {
            setText(defaults.join("\n"));
            setDirty(true);
          }}
        >
          Khôi phục mặc định
        </button>
        {!compact && <span className="muted small">Áp dụng từ lần quét tiếp theo (bấm Quét trên thanh công cụ).</span>}
      </div>
    </>
  );
  if (compact) return body;
  return (
    <section className="panel">
      <h2>
        <Icon name="broom" size={14} /> Loại trừ khi quét
      </h2>
      {body}
    </section>
  );
}
