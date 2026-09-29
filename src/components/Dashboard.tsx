import { useEffect, useState } from "react";
import { api, Bucket, Dashboard as Data } from "../api";
import { formatNumber, formatSize } from "../format";
import { AppLogo } from "./AppLogo";
import type { Route } from "../App";

interface Props {
  reloadKey: number;
  onNavigate: (r: Route) => void;
}

export function Dashboard({ reloadKey, onNavigate }: Props) {
  const [d, setD] = useState<Data | null>(null);
  const [measure, setMeasure] = useState<"count" | "size">("count");
  useEffect(() => {
    api.getDashboard().then(setD);
  }, [reloadKey]);
  if (!d) return <div className="page" />;

  const tiles: { label: string; value: string; hint?: string; route?: Route; warn?: boolean }[] = [
    { label: "Resources", value: formatNumber(d.resources), route: { page: "resources", view: "all" } },
    { label: "Tổng dung lượng", value: formatSize(d.total_size) },
    { label: "ZIP + Folder đã ghép", value: formatNumber(d.matched), hint: `${formatNumber(d.archives)} file nén · ${formatNumber(d.folders)} folder` },
    { label: "Unclassified", value: formatNumber(d.unclassified), route: { page: "resources", view: "unclassified" }, warn: d.unclassified > 0 },
    { label: "Favorites", value: formatNumber(d.favorites), route: { page: "resources", view: "favorites" } },
    { label: "Chờ xác nhận ghép", value: formatNumber(d.pending_matches), route: { page: "matches" }, warn: d.pending_matches > 0 },
  ];

  return (
    <div className="page dashboard">
      <div className="page-head">
        <h1>Dashboard</h1>
        <div className="segmented" role="group" aria-label="Thước đo">
          <button className={measure === "count" ? "on" : ""} onClick={() => setMeasure("count")}>Số lượng</button>
          <button className={measure === "size" ? "on" : ""} onClick={() => setMeasure("size")}>Dung lượng</button>
        </div>
      </div>
      <div className="tiles">
        {tiles.map((t) => (
          <button
            key={t.label}
            className={`tile ${t.route ? "clickable" : ""}`}
            onClick={() => t.route && onNavigate(t.route)}
            disabled={!t.route}
          >
            <div className="tile-label">{t.label}</div>
            <div className="tile-value">
              {t.value}
              {t.warn && <span className="tile-flag" aria-label="Cần xử lý">●</span>}
            </div>
            {t.hint && <div className="tile-hint">{t.hint}</div>}
          </button>
        ))}
      </div>
      {d.missing > 0 && (
        <button className="notice" onClick={() => onNavigate({ page: "resources", view: "missing" })}>
          ⚠ {d.missing} resource có nguồn không truy cập được (ổ ngoài offline hoặc file đã bị di chuyển). Metadata và notes vẫn được giữ.
        </button>
      )}
      <div className="breakdowns">
        <Breakdown title="Theo Application" logos data={d.by_app} measure={measure} onClick={(b) => onNavigate({ page: "resources", view: "tag", tagId: b.id })} />
        <Breakdown title="Theo Resource Type" data={d.by_type} measure={measure} onClick={(b) => onNavigate({ page: "resources", view: "tag", tagId: b.id })} />
        <Breakdown title="Theo trạng thái" data={d.by_status} measure={measure} onClick={(b) => onNavigate({ page: "resources", view: "tag", tagId: b.id })} />
        <Breakdown title="Theo Library" data={d.by_library} measure={measure} onClick={(b) => onNavigate({ page: "resources", view: "library", libraryId: b.id })} />
      </div>
    </div>
  );
}

function Breakdown({ title, data, measure, onClick, logos }: { title: string; data: Bucket[]; measure: "count" | "size"; onClick: (b: Bucket) => void; logos?: boolean }) {
  const sorted = [...data].sort((a, b) => (measure === "count" ? b.count - a.count : b.size - a.size)).slice(0, 12);
  const max = Math.max(1, ...sorted.map((b) => (measure === "count" ? b.count : b.size)));
  return (
    <section className="breakdown">
      <h2>{title}</h2>
      {sorted.length === 0 ? (
        <p className="muted small">Chưa có dữ liệu — gắn tag cho resource để xem thống kê.</p>
      ) : (
        <div className="bars">
          {sorted.map((b) => {
            const v = measure === "count" ? b.count : b.size;
            return (
              <button
                key={b.id}
                className="bar-row"
                onClick={() => onClick(b)}
                title={`${b.name}: ${formatNumber(b.count)} resource · ${formatSize(b.size)}`}
              >
                <span className="bar-label truncate">
                  {logos && <AppLogo name={b.name} size={13} />} {b.name}
                </span>
                <span className="bar-track">
                  <span className="bar-fill" style={{ width: `${Math.max(1, (v / max) * 100)}%` }} />
                </span>
                <span className="bar-value">{measure === "count" ? formatNumber(b.count) : formatSize(b.size)}</span>
              </button>
            );
          })}
        </div>
      )}
    </section>
  );
}
