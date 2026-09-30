import { useEffect, useState } from "react";
import { api, DuplicateGroup, ExistingHit, VersionItem } from "../api";
import { errorText, FILE_MANAGER, formatSize } from "../format";
import { Icon } from "./Icon";

interface Props {
  reloadKey: number;
  onChanged: () => void;
  onOpen: (id: number) => void;
  toast: (msg: string, kind?: "ok" | "err") => void;
}

type Tab = "duplicates" | "versions" | "check";

export function Cleanup({ reloadKey, onChanged, onOpen, toast }: Props) {
  const [tab, setTab] = useState<Tab>("duplicates");
  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Dọn dẹp thư viện</h1>
          <p className="muted">Tìm tài nguyên trùng, các phiên bản của cùng một gói, và kiểm tra trước khi tải thêm. Không file thật nào bị xóa hay di chuyển.</p>
        </div>
        <div className="segmented">
          <button className={tab === "duplicates" ? "on" : ""} onClick={() => setTab("duplicates")}>Trùng lặp</button>
          <button className={tab === "versions" ? "on" : ""} onClick={() => setTab("versions")}>Phiên bản</button>
          <button className={tab === "check" ? "on" : ""} onClick={() => setTab("check")}>Đã có chưa?</button>
        </div>
      </div>
      {tab === "duplicates" && <Duplicates reloadKey={reloadKey} onChanged={onChanged} onOpen={onOpen} toast={toast} />}
      {tab === "versions" && <Versions reloadKey={reloadKey} onChanged={onChanged} onOpen={onOpen} toast={toast} />}
      {tab === "check" && <CheckExisting onOpen={onOpen} />}
    </div>
  );
}

function Duplicates({ reloadKey, onChanged, onOpen, toast }: Props) {
  const [groups, setGroups] = useState<DuplicateGroup[] | null>(null);
  const load = () => api.findDuplicates().then(setGroups).catch((err) => toast(errorText(err), "err"));
  useEffect(() => {
    load();
  }, [reloadKey]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!groups) return <div className="muted"><span className="spinner" /> Đang tìm…</div>;
  if (groups.length === 0)
    return (
      <div className="empty-state">
        <Icon name="check" size={28} />
        <p>Không phát hiện tài nguyên trùng.</p>
      </div>
    );
  return (
    <div className="dup-list">
      {groups.map((g, i) => {
        const ids = g.resources.map((r) => r.id);
        const wasted = g.resources.reduce((a, r) => a + r.size, 0) - Math.max(...g.resources.map((r) => r.size));
        return (
          <section key={i} className="panel dup-group">
            <div className="dup-head">
              <span className={`badge ${g.reason === "content" ? "warn" : ""}`}>
                {g.reason === "content" ? "Nội dung giống hệt" : "Cùng tên + phiên bản"}
              </span>
              {wasted > 0 && <span className="muted small">~{formatSize(wasted)} có thể trùng</span>}
              <div className="row-actions">
                <button
                  className="btn tiny primary"
                  title="Gộp thành một resource có nhiều nguồn (giữ nguyên file thật)"
                  onClick={async () => {
                    try {
                      const keep = await api.mergeResources(ids);
                      toast("Đã gộp");
                      onChanged();
                      onOpen(keep);
                    } catch (err) {
                      toast(errorText(err), "err");
                    }
                  }}
                >
                  <Icon name="merge" size={12} /> Gộp làm một
                </button>
                <button
                  className="btn tiny"
                  onClick={async () => {
                    await api.dismissDuplicates(ids);
                    load();
                  }}
                >
                  Không phải trùng
                </button>
              </div>
            </div>
            {g.resources.map((r) => (
              <button key={r.id} className="dup-item" onClick={() => onOpen(r.id)}>
                <div className="dup-name">
                  <span className="truncate">{r.name}</span>
                  {r.version && <span className="badge">v{r.version}</span>}
                  <span className="muted small">{formatSize(r.size)}</span>
                </div>
                {r.locations.map((l) => (
                  <div key={l} className="muted small truncate">{l}</div>
                ))}
              </button>
            ))}
          </section>
        );
      })}
      <p className="muted small">Muốn xóa bản thừa? Dùng “Mở vị trí” trong Inspector rồi tự xóa trong {FILE_MANAGER} — MRM không bao giờ tự xóa file.</p>
    </div>
  );
}

function Versions({ reloadKey, onChanged, onOpen, toast }: Props) {
  const [fams, setFams] = useState<VersionItem[][] | null>(null);
  useEffect(() => {
    api.listVersionFamilies().then(setFams);
  }, [reloadKey]);
  if (!fams) return <div className="muted"><span className="spinner" /> Đang phân tích…</div>;
  return (
    <>
      <div className="row-actions" style={{ marginBottom: 12 }}>
        <button
          className="btn"
          disabled={fams.length === 0}
          onClick={async () => {
            const n = await api.markOutdatedVersions();
            toast(`Đã gắn “Update Available” cho ${n} bản cũ`);
            onChanged();
          }}
          title="Gắn status Update Available cho các bản cũ, gỡ khỏi bản mới nhất"
        >
          Đánh dấu bản cũ
        </button>
      </div>
      {fams.length === 0 ? (
        <div className="empty-state">
          <Icon name="git" size={28} />
          <p>Chưa có gói nào có nhiều phiên bản.</p>
        </div>
      ) : (
        <div className="dup-list">
          {fams.map((f) => (
            <section key={f[0].id} className="panel dup-group">
              {f.map((v) => (
                <button key={v.id} className="dup-item row" onClick={() => onOpen(v.id)}>
                  <Icon name="git" size={14} />
                  <span className="truncate">{v.name}</span>
                  <span className={`badge ${v.latest ? "ok" : ""}`}>
                    {v.version ? `v${v.version}` : "không rõ"}
                    {v.latest ? " · mới nhất" : ""}
                  </span>
                </button>
              ))}
            </section>
          ))}
        </div>
      )}
    </>
  );
}

function CheckExisting({ onOpen }: { onOpen: (id: number) => void }) {
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<ExistingHit[] | null>(null);
  useEffect(() => {
    if (!q.trim()) {
      setHits(null);
      return;
    }
    const t = setTimeout(() => api.checkExisting(q).then(setHits), 200);
    return () => clearTimeout(t);
  }, [q]);
  return (
    <div className="panel">
      <h2>Trước khi tải, kiểm tra xem đã có chưa</h2>
      <p className="muted small">Dán tên file/gói (vd: “Motion Bro 4.6 Full.zip”) — so khớp với tên đã chuẩn hóa của toàn bộ thư viện.</p>
      <div className="search big">
        <Icon name="search" size={15} />
        <input autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder="Tên gói định tải…" />
      </div>
      {hits && hits.length === 0 && (
        <p className="ok-text">
          <Icon name="check" size={14} /> Chưa có gì tương tự trong thư viện.
        </p>
      )}
      {hits && hits.length > 0 && (
        <div className="dup-list" style={{ marginTop: 12 }}>
          {hits.map((h) => (
            <button key={h.id} className="dup-item row" onClick={() => onOpen(h.id)}>
              <span className={`score ${h.score >= 95 ? "high" : ""}`}>{h.score.toFixed(0)}%</span>
              <span className="truncate">{h.name}</span>
              {h.version && <span className="badge">v{h.version}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
