import { useEffect, useState } from "react";
import { api, SuggestApplyResult } from "../api";
import { errorText, formatNumber } from "../format";
import { Icon } from "./Icon";

const SOURCES: { id: string; label: string; hint: string; def: boolean }[] = [
  { id: "content", label: "Nội dung file", hint: ".cube → LUT, .drfx → DaVinci Resolve…", def: true },
  { id: "path", label: "Tên thư mục chứa", hint: "nằm trong “Luts”, “DR plugin”, “Preset LR”…", def: true },
  { id: "rule", label: "Rules", hint: "các rule bạn đã tạo", def: true },
  { id: "learned", label: "Học từ bạn", hint: "tên giống các mục bạn đã gắn tag", def: true },
  { id: "ai", label: "AI (gần nghĩa)", hint: "dễ sai hơn — nên rà soát kỹ", def: false },
];
const KINDS: { id: string; label: string; def: boolean }[] = [
  { id: "app", label: "Application", def: true },
  { id: "type", label: "Resource Type", def: true },
  { id: "function", label: "Function tag", def: false },
];

interface Props {
  ids: number[];
  scopeLabel: string;
  onClose: () => void;
  onDone: () => void;
  toast: (msg: string, kind?: "ok" | "err") => void;
}

export function BulkSuggest({ ids, scopeLabel, onClose, onDone, toast }: Props) {
  const [sources, setSources] = useState(() => SOURCES.filter((s) => s.def).map((s) => s.id));
  const [kinds, setKinds] = useState(() => KINDS.filter((k) => k.def).map((k) => k.id));
  const [minScore, setMinScore] = useState(0.7);
  const [preview, setPreview] = useState<SuggestApplyResult | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setPreview(null);
    const t = setTimeout(
      () =>
        api
          .applySuggestions(ids, { sources, kinds, min_score: minScore, dry_run: true })
          .then(setPreview)
          .catch((err) => toast(errorText(err), "err")),
      150,
    );
    return () => clearTimeout(t);
  }, [ids, sources, kinds, minScore]); // eslint-disable-line react-hooks/exhaustive-deps

  const toggle = (list: string[], set: (v: string[]) => void, id: string) =>
    set(list.includes(id) ? list.filter((x) => x !== id) : [...list, id]);

  const apply = async () => {
    setBusy(true);
    try {
      const r = await api.applySuggestions(ids, { sources, kinds, min_score: minScore, dry_run: false });
      toast(`Đã tự gắn ${formatNumber(r.tags)} tag cho ${formatNumber(r.resources)} resource — xem lại ở mục “Cần rà soát”`);
      onDone();
      onClose();
    } catch (err) {
      toast(errorText(err), "err");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="dialog">
        <header className="preview-head">
          <Icon name="sparkles" size={16} />
          <div className="preview-title">Áp dụng gợi ý hàng loạt</div>
          <button className="icon-btn" onClick={onClose} aria-label="Đóng">
            <Icon name="x" size={16} />
          </button>
        </header>
        <div className="dialog-body">
          <p className="muted small">
            Phạm vi: <b>{scopeLabel}</b> ({formatNumber(ids.length)} resource). Tag được gắn sẽ đánh dấu <span className="chip small auto-demo">tự động</span> kèm lý do —
            bạn xác nhận hoặc gỡ sau trong mục <b>Cần rà soát</b>. Tag bạn đã gỡ sẽ không bị gắn lại.
          </p>
          <div className="dialog-grid">
            <div>
              <div className="insp-label">Lấy gợi ý từ</div>
              {SOURCES.map((s) => (
                <label key={s.id} className="check-label block">
                  <input type="checkbox" checked={sources.includes(s.id)} onChange={() => toggle(sources, setSources, s.id)} />
                  <span>
                    {s.label} <span className="muted small">— {s.hint}</span>
                  </span>
                </label>
              ))}
            </div>
            <div>
              <div className="insp-label">Loại tag</div>
              {KINDS.map((k) => (
                <label key={k.id} className="check-label block">
                  <input type="checkbox" checked={kinds.includes(k.id)} onChange={() => toggle(kinds, setKinds, k.id)} />
                  {k.label}
                </label>
              ))}
              <div className="insp-label" style={{ marginTop: 12 }}>
                Độ tin cậy tối thiểu
              </div>
              <select value={minScore} onChange={(e) => setMinScore(Number(e.target.value))}>
                <option value={0.6}>60% — gắn nhiều hơn</option>
                <option value={0.7}>70% — cân bằng</option>
                <option value={0.8}>80% — chắc chắn hơn</option>
              </select>
              <p className="muted tiny-text">Mỗi resource tối đa 2 tag cho mỗi loại.</p>
            </div>
          </div>
          <div className="rule-test">
            {preview ? (
              <>
                Sẽ gắn <b>{formatNumber(preview.tags)}</b> tag cho <b>{formatNumber(preview.resources)}</b> resource
                {Object.keys(preview.by_kind).length > 0 && (
                  <span className="muted">
                    {" "}
                    ({Object.entries(preview.by_kind)
                      .map(([k, n]) => `${KINDS.find((x) => x.id === k)?.label ?? k}: ${n}`)
                      .join(" · ")})
                  </span>
                )}
              </>
            ) : (
              <span className="muted">
                <span className="spinner" /> Đang tính…
              </span>
            )}
          </div>
        </div>
        <footer className="dialog-foot">
          <button className="btn" onClick={onClose}>
            Hủy
          </button>
          <button className="btn primary" disabled={busy || !preview || preview.tags === 0} onClick={apply}>
            <Icon name="check" size={14} /> Áp dụng
          </button>
        </footer>
      </div>
    </div>
  );
}
