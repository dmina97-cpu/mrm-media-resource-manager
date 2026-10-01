import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { AiStatus, api, BackupInfo, Library, ScanOverride } from "../api";
import { errorText, formatDateTime, formatNumber, formatSize } from "../format";
import { Icon } from "./Icon";
import { AiSettings, StorageSettings } from "./AiSettings";
import { ExcludeSettings, UpdateSettings } from "./Updates";
import { RelinkLibraryDialog, RemoveLibraryDialog } from "./LibraryDialogs";

interface Props {
  libraries: Library[];
  scanning: Set<number>;
  reloadKey: number;
  onAddLibrary: () => void;
  onScan: (id: number, full: boolean) => void;
  onChanged: () => void;
  toast: (msg: string, kind?: "ok" | "err") => void;
  ai: AiStatus | null;
  refreshAi: () => void;
}

export function Settings({ libraries, scanning, reloadKey, onAddLibrary, onScan, onChanged, toast, ai, refreshAi }: Props) {
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [dataDir, setDataDir] = useState("");
  const loadBackups = () => api.listBackups().then(setBackups);
  useEffect(() => {
    loadBackups();
    api.getDataDir().then(setDataDir);
  }, [reloadKey]);

  const run = async (fn: () => Promise<unknown>, ok?: string) => {
    try {
      await fn();
      if (ok) toast(ok);
    } catch (err) {
      toast(errorText(err), "err");
    }
  };

  return (
    <div className="page settings">
      <div className="page-head">
        <h1>Libraries & Settings</h1>
        <button className="btn primary" onClick={onAddLibrary}>
          <Icon name="plus" size={14} /> Thêm Library
        </button>
      </div>

      <section className="panel">
        <h2>Libraries</h2>
        <p className="muted small">
          MRM chỉ đọc thư mục để lập chỉ mục. App không bao giờ tự di chuyển, đổi tên hay xóa file thật.
        </p>
        {libraries.length === 0 && <p className="muted">Chưa có library nào.</p>}
        {libraries.map((l) => (
          <LibraryRow key={l.id} lib={l} scanning={scanning.has(l.id)} onScan={onScan} onChanged={onChanged} run={run} />
        ))}
      </section>

      <ExcludeSettings toast={toast} />
      <AiSettings ai={ai} refreshAi={refreshAi} toast={toast} />
      <StorageSettings toast={toast} />
      <UpdateSettings toast={toast} />
      <section className="panel">
        <h2>
          <Icon name="alert" size={14} /> Báo lỗi
        </h2>
        <p className="muted small">
          Gặp lỗi? Xuất file chẩn đoán rồi gửi cho nhà phát triển. File chỉ gồm phiên bản, số đếm, trạng thái AI và lỗi gần đây — không chứa đường dẫn, tên thư viện, tên
          file hay ghi chú của bạn.
        </p>
        <div className="row-actions">
          <button
            className="btn"
            onClick={async () => {
              const p = await save({
                title: "Lưu file chẩn đoán MRM",
                defaultPath: `MRM-diagnostics-${new Date().toISOString().slice(0, 10)}.json`,
                filters: [{ name: "JSON", extensions: ["json"] }],
              });
              if (p) run(() => api.exportDiagnostics(p), "Đã xuất file chẩn đoán");
            }}
          >
            <Icon name="file" size={14} /> Xuất chẩn đoán
          </button>
        </div>
      </section>

      <section className="panel">
        <h2>Backup & Export</h2>
        <p className="muted small">
          Database được tự động backup mỗi ngày (giữ 10 bản). Dữ liệu app: <code>{dataDir}</code>
        </p>
        <div className="row-actions">
          <button className="btn" onClick={() => run(async () => { await api.backupNow(); await loadBackups(); }, "Đã tạo backup")}>
            <Icon name="database" size={14} /> Backup ngay
          </button>
          <button
            className="btn"
            onClick={() =>
              run(async () => {
                const path = await save({
                  defaultPath: `nink-vault-export-${new Date().toISOString().slice(0, 10)}.json`,
                  filters: [{ name: "JSON", extensions: ["json"] }],
                });
                if (path) {
                  await api.exportJson(path);
                  toast("Đã export JSON");
                }
              })
            }
          >
            Export JSON
          </button>
          <button
            className="btn"
            onClick={() =>
              run(async () => {
                const path = await open({ filters: [{ name: "MRM DB", extensions: ["db"] }], multiple: false });
                if (typeof path === "string" && confirm("Khôi phục database từ file này? Bản hiện tại sẽ được backup trước khi ghi đè.")) {
                  await api.restoreBackup(path);
                  onChanged();
                  await loadBackups();
                  toast("Đã khôi phục database");
                }
              })
            }
          >
            Khôi phục từ file…
          </button>
          <button className="btn" onClick={() => run(api.openDataDir)}>
            <Icon name="folder" size={14} /> Mở thư mục dữ liệu
          </button>
        </div>
        {backups.length > 0 && (
          <table className="table">
            <thead>
              <tr>
                <th>Backup</th>
                <th>Thời gian</th>
                <th>Dung lượng</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {backups.slice(0, 12).map((b) => (
                <tr key={b.path}>
                  <td>{b.name}</td>
                  <td>{formatDateTime(b.modified)}</td>
                  <td>{formatSize(b.size)}</td>
                  <td>
                    <button
                      className="btn tiny"
                      onClick={() =>
                        confirm(`Khôi phục database từ ${b.name}? Bản hiện tại sẽ được backup trước.`) &&
                        run(async () => {
                          await api.restoreBackup(b.path);
                          onChanged();
                          await loadBackups();
                        }, "Đã khôi phục database")
                      }
                    >
                      Khôi phục
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="panel">
        <h2>Cú pháp tìm kiếm</h2>
        <table className="table syntax">
          <tbody>
            <tr><td><code>light leak resolve</code></td><td>Tìm theo tên, tên file/folder, đường dẫn, tag và notes</td></tr>
            <tr><td><code>app:resolve</code></td><td>Lọc theo Application</td></tr>
            <tr><td><code>type:plugin</code> · <code>type:"stock video"</code></td><td>Lọc theo Resource Type</td></tr>
            <tr><td><code>tag:tracking</code></td><td>Lọc theo Function/Personal tag</td></tr>
            <tr><td><code>status:installed</code></td><td>Lọc theo trạng thái</td></tr>
            <tr><td><code>is:fav</code> · <code>is:missing</code> · <code>is:matched</code> · <code>is:unclassified</code></td><td>Bộ lọc nhanh</td></tr>
            <tr><td><code>lib:plugins</code></td><td>Lọc theo tên Library</td></tr>
          </tbody>
        </table>
        <p className="muted small">Phím tắt: <kbd>Ctrl</kbd>+<kbd>F</kbd> tìm kiếm · <kbd>↑</kbd><kbd>↓</kbd> chọn · <kbd>F</kbd> favorite · <kbd>N</kbd> mục chưa phân loại tiếp theo · <kbd>Esc</kbd> bỏ chọn</p>
      </section>
    </div>
  );
}

function LibraryRow({
  lib,
  scanning,
  onScan,
  onChanged,
  run,
}: {
  lib: Library;
  scanning: boolean;
  onScan: (id: number, full: boolean) => void;
  onChanged: () => void;
  run: (fn: () => Promise<unknown>, ok?: string) => void;
}) {
  const [name, setName] = useState(lib.name);
  const [depth, setDepth] = useState(lib.scan_depth);
  const [overrides, setOverrides] = useState<ScanOverride[]>([]);
  const [dialog, setDialog] = useState<"remove" | "relink" | null>(null);
  const loadOverrides = () => api.listScanOverrides(lib.id).then(setOverrides);
  useEffect(() => {
    loadOverrides();
  }, [lib.id, lib.last_scan_at]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    setName(lib.name);
    setDepth(lib.scan_depth);
  }, [lib.name, lib.scan_depth]);
  const dirty = name !== lib.name || depth !== lib.scan_depth;

  return (
    <div className="lib-row">
      <div className="lib-main">
        <div className="lib-name">
          <span className={`dot ${lib.online ? "on" : "off"}`} />
          <input value={name} onChange={(e) => setName(e.target.value)} aria-label="Tên library" />
          {!lib.online && <span className="badge warn">offline</span>}
        </div>
        {!lib.online && (
          <div className="muted small">
            Không thấy thư mục. Đã di chuyển / đổi tên?{" "}
            <button className="link-btn" onClick={() => setDialog("relink")}>
              Tìm lại vị trí mới
            </button>
          </div>
        )}
        <div className="muted small">{lib.path}</div>
        <div className="muted small">
          {formatNumber(lib.source_count)} nguồn · {formatSize(lib.size)} · quét lần cuối {formatDateTime(lib.last_scan_at)}
        </div>
        {overrides.length > 0 && (
          <div className="overrides">
            <span className="muted small">Tách/gộp thủ công:</span>
            {overrides.map((o) => (
              <span key={o.rel_path} className="chip small" title={o.rel_path}>
                {o.mode === "pack" ? "Gộp" : "Tách"}: {o.rel_path.split(/[\\/]/).pop()}
                <button
                  className="chip-x"
                  aria-label="Bỏ"
                  onClick={() =>
                    run(async () => {
                      await api.setScanOverride(lib.id, o.rel_path, null);
                      await loadOverrides();
                      onScan(lib.id, false);
                    })
                  }
                >
                  ✕
                </button>
              </span>
            ))}
          </div>
        )}
      </div>
      <label className="lib-depth" title="Tự động: MRM tự nhận ra thư mục nào là nhóm phân loại (đi vào trong) và thư mục nào là một gói tài nguyên — hợp với thư viện sắp xếp nhiều tầng lẫn lộn. Độ sâu cố định: mọi thứ ở đúng tầng đó là một resource.">
        Cách quét
        <select value={depth} onChange={(e) => setDepth(Number(e.target.value))}>
          <option value={0}>Tự động (khuyên dùng)</option>
          {[1, 2, 3, 4, 5].map((d) => (
            <option key={d} value={d}>Độ sâu cố định {d}</option>
          ))}
        </select>
      </label>
      <div className="row-actions">
        {dirty && (
          <button className="btn tiny primary" onClick={() => run(async () => { await api.updateLibrary(lib.id, name, depth); onChanged(); }, "Đã lưu")}>
            Lưu
          </button>
        )}
        <button className="btn tiny" disabled={scanning || !lib.online} onClick={() => onScan(lib.id, false)}>
          <Icon name="refresh" size={12} /> {scanning ? "Đang quét…" : "Quét"}
        </button>
        <button className="btn tiny" disabled={scanning || !lib.online} onClick={() => onScan(lib.id, true)} title="Đọc lại toàn bộ metadata, bỏ qua cache">
          Quét lại toàn bộ
        </button>
        <button className="btn tiny" disabled={scanning} onClick={() => setDialog("relink")} title="Thư mục gốc đã di chuyển / đổi tên / sang ổ khác — giữ nguyên tag, ghi chú">
          <Icon name="folder" size={12} /> Đổi đường dẫn
        </button>
        <button className="btn tiny danger" disabled={scanning} onClick={() => setDialog("remove")}>
          Gỡ
        </button>
      </div>
      {dialog === "remove" && (
        <RemoveLibraryDialog
          lib={lib}
          onClose={() => setDialog(null)}
          onRelink={() => setDialog("relink")}
          onRemoved={() => {
            setDialog(null);
            onChanged();
            run(async () => {}, "Đã gỡ library");
          }}
        />
      )}
      {dialog === "relink" && (
        <RelinkLibraryDialog
          lib={lib}
          onClose={() => setDialog(null)}
          onDone={(l) => {
            setDialog(null);
            onChanged();
            onScan(l.id, false);
            run(async () => {}, `Đã đổi đường dẫn — đang quét ${l.path}`);
          }}
        />
      )}
    </div>
  );
}
