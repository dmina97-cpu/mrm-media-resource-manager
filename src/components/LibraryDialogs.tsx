import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, Library, LibraryImpact, RelinkPreview } from "../api";
import { errorText, formatNumber } from "../format";
import { Icon } from "./Icon";

/** Gỡ library: cho biết sẽ mất gì và gợi ý cách khác (đổi đường dẫn, quét lại, gộp) trước khi xóa. */
export function RemoveLibraryDialog({
  lib,
  onClose,
  onRelink,
  onRemoved,
}: {
  lib: Library;
  onClose: () => void;
  onRelink: () => void;
  onRemoved: () => void;
}) {
  const [impact, setImpact] = useState<LibraryImpact | null>(null);
  const [ack, setAck] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  useEffect(() => {
    api.libraryImpact(lib.id).then(setImpact, (e) => setErr(errorText(e)));
  }, [lib.id]);
  const losing = impact ? impact.curated_resources + impact.curated_media : 0;

  const remove = async () => {
    setBusy(true);
    try {
      await api.removeLibrary(lib.id);
      onRemoved();
    } catch (e) {
      setErr(errorText(e));
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <div className="dialog lib-dialog">
        <header className="preview-head">
          <Icon name="alert" size={16} />
          <div className="preview-title">Gỡ library “{lib.name}”?</div>
          <button className="icon-btn" onClick={onClose} aria-label="Đóng" disabled={busy}>
            <Icon name="x" size={16} />
          </button>
        </header>
        <div className="dialog-body">
          <p className="muted small">{lib.path}</p>
          {!impact && !err && <p className="muted small">Đang kiểm tra dữ liệu…</p>}
          {impact && (
            <>
              {losing > 0 ? (
                <div className="lib-warn">
                  <b>Sẽ mất dữ liệu bạn đã nhập:</b>
                  <ul>
                    {impact.curated_resources > 0 && (
                      <li>
                        <b>{formatNumber(impact.curated_resources)}</b> resource có tag, ghi chú, yêu thích, ảnh bìa hoặc nằm trong collection
                      </li>
                    )}
                    {impact.curated_media > 0 && (
                      <li>
                        <b>{formatNumber(impact.curated_media)}</b> file âm thanh / hình ảnh / video có tag hoặc yêu thích (dùng chung với plugin DaVinci)
                      </li>
                    )}
                  </ul>
                </div>
              ) : (
                <p className="small">Library này chưa có tag, ghi chú hay yêu thích nào — gỡ không mất dữ liệu bạn đã nhập.</p>
              )}
              <p className="muted small">
                Gỡ {formatNumber(impact.resources)} resource và {formatNumber(impact.media)} file media khỏi MRM
                {impact.shared_resources > 0 && <> ({formatNumber(impact.shared_resources)} resource còn nguồn ở library khác được giữ lại)</>}. File thật trên ổ đĩa{" "}
                <b>không</b> bị ảnh hưởng. MRM tự sao lưu database trước khi gỡ.
              </p>
            </>
          )}

          <div className="lib-alts">
            <div className="insp-label">Bạn chỉ đang sắp xếp lại thư mục? Không cần gỡ:</div>
            <div className="lib-alt">
              <Icon name="folder" size={15} />
              <div>
                <b>Đã di chuyển, đổi tên thư mục gốc hoặc chuyển sang ổ khác</b>
                <div className="muted small">Đổi đường dẫn — giữ nguyên toàn bộ tag, ghi chú, yêu thích.</div>
              </div>
              <button className="btn tiny primary" onClick={onRelink} disabled={busy}>
                Đổi đường dẫn…
              </button>
            </div>
            <div className="lib-alt">
              <Icon name="refresh" size={15} />
              <div>
                <b>Sắp xếp lại thư mục con bên trong</b>
                <div className="muted small">Chỉ cần bấm Quét — file di chuyển trong library tự được nối lại, giữ tag và yêu thích.</div>
              </div>
            </div>
            <div className="lib-alt">
              <Icon name="merge" size={15} />
              <div>
                <b>Gom sang thư mục / library khác</b>
                <div className="muted small">
                  Thêm và quét library mới <b>trước</b>, rồi mới gỡ library này: file media đã chuyển sang sẽ mang theo tag và yêu thích. Với gói tài nguyên: chọn bản cũ + bản mới trong
                  All Resources → <b>Gộp</b> để chuyển tag, ghi chú.
                </div>
              </div>
            </div>
            <div className="lib-alt">
              <Icon name="rule" size={15} />
              <div>
                <b>Chỉ muốn bỏ một thư mục con hoặc một loại file</b>
                <div className="muted small">Dùng Loại trừ khi quét (bên dưới trang này) hoặc Loại quét trong plugin DaVinci — dữ liệu được giữ, chỉ ẩn đi.</div>
              </div>
            </div>
          </div>
          {err && <p className="lib-err">{err}</p>}
        </div>
        <footer className="dialog-foot lib-foot">
          {losing > 0 && (
            <label className="check-label">
              <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} /> Tôi hiểu sẽ mất dữ liệu ở trên
            </label>
          )}
          <span style={{ flex: 1 }} />
          <button className="btn" onClick={onClose} disabled={busy}>
            Hủy
          </button>
          <button className="btn danger" onClick={remove} disabled={busy || !impact || (losing > 0 && !ack)}>
            {busy ? "Đang gỡ…" : "Gỡ library"}
          </button>
        </footer>
      </div>
    </div>
  );
}

/** Đổi đường dẫn gốc: chọn thư mục mới -> kiểm tra mẫu -> xác nhận -> quét lại. */
export function RelinkLibraryDialog({ lib, onClose, onDone }: { lib: Library; onClose: () => void; onDone: (lib: Library) => void }) {
  const [preview, setPreview] = useState<RelinkPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const pick = async () => {
    setErr("");
    const dir = await open({ directory: true, title: `Chọn vị trí mới của "${lib.name}"` });
    if (typeof dir !== "string") return;
    setBusy(true);
    try {
      setPreview(await api.previewRelinkLibrary(lib.id, dir));
    } catch (e) {
      setPreview(null);
      setErr(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    pick();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const apply = async () => {
    if (!preview) return;
    setBusy(true);
    try {
      onDone(await api.relinkLibrary(lib.id, preview.path));
    } catch (e) {
      setErr(errorText(e));
      setBusy(false);
    }
  };

  const ratio = preview && preview.checked > 0 ? preview.found / preview.checked : 0;
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <div className="dialog lib-dialog">
        <header className="preview-head">
          <Icon name="folder" size={16} />
          <div className="preview-title">Đổi đường dẫn “{lib.name}”</div>
          <button className="icon-btn" onClick={onClose} aria-label="Đóng" disabled={busy}>
            <Icon name="x" size={16} />
          </button>
        </header>
        <div className="dialog-body">
          <p className="small">
            Dùng khi thư mục gốc đã được <b>di chuyển, đổi tên hoặc chuyển sang ổ khác</b>. MRM trỏ library sang vị trí mới và giữ nguyên tag, ghi chú, yêu thích, ảnh bìa, collection — cả
            dữ liệu dùng chung với plugin DaVinci.
          </p>
          <div className="lib-paths">
            <div>
              <span className="muted small">Hiện tại</span>
              <code>{lib.path}</code>
            </div>
            <div>
              <span className="muted small">Vị trí mới</span>
              <code>{preview?.path ?? "—"}</code>
            </div>
          </div>
          {busy && !preview && <p className="muted small">Đang kiểm tra thư mục…</p>}
          {preview && preview.checked === 0 && <p className="small">Library chưa có dữ liệu để đối chiếu — cứ đổi, MRM sẽ quét thư mục mới.</p>}
          {preview && preview.checked > 0 && (
            <div className={ratio >= 0.7 ? "lib-ok" : "lib-warn"}>
              {ratio >= 0.7 ? (
                <>
                  <Icon name="check" size={14} /> Khớp: tìm thấy <b>{formatNumber(preview.found)}</b>/{formatNumber(preview.checked)} mục mẫu ở vị trí mới — đúng là thư mục đã di
                  chuyển.
                </>
              ) : (
                <>
                  Chỉ tìm thấy <b>{formatNumber(preview.found)}</b>/{formatNumber(preview.checked)} mục mẫu — cấu trúc bên trong đã khác nhiều hoặc chọn nhầm thư mục. Vẫn đổi được: phần
                  không khớp sẽ được đánh dấu “không có sẵn” (giữ tag) sau khi quét, gói tài nguyên có thể Gộp lại sau.
                </>
              )}
            </div>
          )}
          {err && <p className="lib-err">{err}</p>}
        </div>
        <footer className="dialog-foot">
          <button className="btn" onClick={pick} disabled={busy}>
            Chọn thư mục khác…
          </button>
          <span style={{ flex: 1 }} />
          <button className="btn" onClick={onClose} disabled={busy}>
            Hủy
          </button>
          <button className="btn primary" onClick={apply} disabled={busy || !preview}>
            Đổi đường dẫn &amp; quét
          </button>
        </footer>
      </div>
    </div>
  );
}
