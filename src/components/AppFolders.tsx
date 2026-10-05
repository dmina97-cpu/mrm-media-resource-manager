import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, Library, Rule, Tag } from "../api";
import { errorText, formatNumber } from "../format";
import { Icon } from "./Icon";

/** Chuẩn so khớp của Rules: chữ thường, "/" (Rules tự chuẩn hóa giá trị điều kiện như vậy). */
const norm = (p: string) => p.trim().replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
const RULE_PREFIX = "Thư mục: ";

/**
 * Gán cả một thư mục cho một / nhiều ứng dụng (tag loại app). Lưu thành rule tự áp dụng nên resource quét sau
 * cũng được gắn. Một thư mục gán được nhiều app; một app có nhiều thư mục. Gỡ gán -> gỡ luôn tag đã gắn theo nó.
 */
export function AppFolderDialog({
  libraries,
  tags,
  initialAppId,
  onClose,
  onChanged,
  toast,
}: {
  libraries: Library[];
  tags: Tag[];
  initialAppId?: number | null;
  onClose: () => void;
  onChanged: () => void;
  toast: (msg: string, kind?: "ok" | "err") => void;
}) {
  const [rules, setRules] = useState<Rule[]>([]);
  const [folder, setFolder] = useState<string | null>(null);
  const [apps, setApps] = useState<Set<number>>(new Set(initialAppId ? [initialAppId] : []));
  const [newApp, setNewApp] = useState("");
  const [extraApps, setExtraApps] = useState<Tag[]>([]);
  const [busy, setBusy] = useState(false);

  const appTags = useMemo(
    () => [...tags.filter((t) => t.kind === "app"), ...extraApps].sort((a, b) => a.name.localeCompare(b.name, "vi")),
    [tags, extraApps],
  );
  const appName = (id: number) => appTags.find((t) => t.id === id)?.name ?? `#${id}`;
  const load = () => api.listRules().then((r) => setRules(r.filter((x) => x.folder_app)));
  useEffect(() => {
    load().catch((e) => toast(errorText(e), "err"));
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const libOf = (dir: string) => libraries.find((l) => norm(dir) === norm(l.path) || norm(dir).startsWith(norm(l.path) + "/"));
  const existing = folder ? rules.find((r) => r.name === RULE_PREFIX + folder) : undefined;

  const pick = async () => {
    const dir = await open({ directory: true, title: "Chọn thư mục để gán cho ứng dụng" });
    if (typeof dir !== "string") return;
    if (!libOf(dir)) {
      toast("Thư mục này chưa nằm trong library nào — thêm library chứa nó trước (Libraries & Settings).", "err");
      return;
    }
    setFolder(dir);
    const r = rules.find((x) => x.name === RULE_PREFIX + dir);
    if (r) setApps(new Set(r.tag_ids));
  };

  const addApp = async () => {
    const name = newApp.trim();
    if (!name) return;
    const found = appTags.find((t) => t.name.toLowerCase() === name.toLowerCase());
    try {
      const id = found ? found.id : await api.createTag("app", name);
      if (!found) setExtraApps((x) => [...x, { id, kind: "app", name, color: null, count: 0 }]);
      setApps((s) => new Set(s).add(id));
      setNewApp("");
    } catch (e) {
      toast(errorText(e), "err");
    }
  };

  const save = async () => {
    if (!folder || apps.size === 0) return;
    setBusy(true);
    try {
      const f = norm(folder);
      const id = await api.saveRule({
        id: existing?.id ?? 0,
        name: RULE_PREFIX + folder,
        enabled: true,
        auto_apply: true,
        // chính thư mục đó HOẶC bên trong nó (không dính "LUTs2" khi chọn "LUTs")
        match_all: false,
        conditions: [
          { field: "path", op: "equals", value: f },
          { field: "path", op: "starts_with", value: f + "/" },
        ],
        tag_ids: [...apps],
        folder_app: true,
      });
      const n = await api.applyRuleNow(id);
      toast(`Đã gán ${[...apps].map(appName).join(", ")} cho ${formatNumber(n)} resource trong thư mục`);
      setFolder(null);
      setApps(new Set(initialAppId ? [initialAppId] : []));
      await load();
      onChanged();
    } catch (e) {
      toast(errorText(e), "err");
    } finally {
      setBusy(false);
    }
  };

  const remove = async (r: Rule) => {
    if (!confirm(`Gỡ gán "${r.name.slice(RULE_PREFIX.length)}"?\n\nTag ứng dụng đã gắn theo thư mục này sẽ được gỡ (tag bạn tự gắn tay giữ nguyên).`)) return;
    try {
      await api.deleteRule(r.id);
      await load();
      onChanged();
    } catch (e) {
      toast(errorText(e), "err");
    }
  };

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <div className="dialog lib-dialog">
        <header className="preview-head">
          <Icon name="folder" size={16} />
          <div className="preview-title">Gán thư mục cho ứng dụng</div>
          <button className="icon-btn" onClick={onClose} aria-label="Đóng" disabled={busy}>
            <Icon name="x" size={16} />
          </button>
        </header>
        <div className="dialog-body">
          <p className="muted small">
            Mọi resource trong thư mục (kể cả resource quét sau này) được gắn ứng dụng đã chọn. Một thư mục gán được nhiều ứng dụng, một ứng dụng có thể có nhiều thư mục — không
            cần thêm thư mục làm library lần nữa.
          </p>
          <div className="lib-paths">
            <div>
              <span className="muted small">Thư mục</span>
              <div className="row-actions">
                <code style={{ flex: 1 }}>{folder ?? "— chưa chọn —"}</code>
                <button className="btn tiny" onClick={pick} disabled={busy}>
                  <Icon name="folder" size={12} /> Chọn thư mục…
                </button>
              </div>
              {existing && <span className="muted small">Thư mục này đã được gán — sửa danh sách ứng dụng rồi bấm Lưu.</span>}
            </div>
          </div>
          <div>
            <div className="insp-label">Ứng dụng</div>
            <div className="app-pick">
              {appTags.map((t) => (
                <label key={t.id} className={`chip small ${apps.has(t.id) ? "on" : ""}`}>
                  <input
                    type="checkbox"
                    checked={apps.has(t.id)}
                    onChange={() =>
                      setApps((s) => {
                        const n = new Set(s);
                        if (n.has(t.id)) n.delete(t.id);
                        else n.add(t.id);
                        return n;
                      })
                    }
                  />
                  {t.name}
                </label>
              ))}
            </div>
            <div className="row-actions" style={{ marginTop: 8 }}>
              <input value={newApp} onChange={(e) => setNewApp(e.target.value)} onKeyDown={(e) => e.key === "Enter" && addApp()} placeholder="Ứng dụng khác (vd: Final Cut Pro)…" />
              <button className="btn tiny" onClick={addApp} disabled={!newApp.trim()}>
                <Icon name="plus" size={12} /> Thêm
              </button>
            </div>
          </div>
          {rules.length > 0 && (
            <div className="lib-alts">
              <div className="insp-label">Thư mục đã gán</div>
              {rules.map((r) => (
                <div key={r.id} className="lib-alt">
                  <Icon name="folder" size={15} />
                  <div>
                    <b className="truncate" title={r.name.slice(RULE_PREFIX.length)}>
                      {r.name.slice(RULE_PREFIX.length)}
                    </b>
                    <div className="muted small">{r.tag_ids.map(appName).join(", ")}</div>
                  </div>
                  <button className="btn tiny" onClick={() => (setFolder(r.name.slice(RULE_PREFIX.length)), setApps(new Set(r.tag_ids)))}>
                    Sửa
                  </button>
                  <button className="btn tiny danger" onClick={() => remove(r)}>
                    Gỡ
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
        <footer className="dialog-foot">
          <button className="btn" onClick={onClose} disabled={busy}>
            Đóng
          </button>
          <button className="btn primary" onClick={save} disabled={busy || !folder || apps.size === 0}>
            {busy ? "Đang gán…" : existing ? "Lưu" : "Gán"}
          </button>
        </footer>
      </div>
    </div>
  );
}
