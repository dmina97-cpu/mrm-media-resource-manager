import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ResourceSummary, Tag } from "../api";
import { formatDate, formatSize } from "../format";
import { Icon, KindIcon } from "./Icon";
import { CoverThumb } from "./Media";
import { AppLogo } from "./AppLogo";

interface Props {
  items: ResourceSummary[];
  tagMap: Map<number, Tag>;
  layout: "grid" | "list";
  selected: Set<number>;
  focusId: number | null;
  onSelect: (id: number, e: React.MouseEvent) => void;
  onToggleFavorite: (r: ResourceSummary) => void;
}

const ROW_H = 38;
const CARD_W = 220;
const CARD_H = 250;
const GAP = 12;
const PAD = 16;

function useViewport(ref: React.RefObject<HTMLDivElement | null>) {
  const [size, setSize] = useState({ width: 800, height: 600, scrollTop: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => setSize({ width: el.clientWidth, height: el.clientHeight, scrollTop: el.scrollTop });
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    el.addEventListener("scroll", update, { passive: true });
    return () => {
      ro.disconnect();
      el.removeEventListener("scroll", update);
    };
  }, [ref]);
  return size;
}

function tagsOf(r: ResourceSummary, tagMap: Map<number, Tag>, kind: string) {
  return r.tag_ids.map((id) => tagMap.get(id)).filter((t): t is Tag => !!t && t.kind === kind);
}

export function ResourceView(props: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const vp = useViewport(ref);
  const { items, layout, focusId } = props;

  // Giữ item đang focus trong tầm nhìn khi điều hướng bằng bàn phím
  useEffect(() => {
    const el = ref.current;
    if (!el || focusId === null) return;
    const idx = items.findIndex((i) => i.id === focusId);
    if (idx < 0) return;
    let top: number, h: number;
    if (layout === "list") {
      top = idx * ROW_H + ROW_H;
      h = ROW_H;
    } else {
      const cols = Math.max(1, Math.floor((vp.width - PAD * 2 + GAP) / (CARD_W + GAP)));
      top = PAD + Math.floor(idx / cols) * (CARD_H + GAP);
      h = CARD_H;
    }
    if (top < el.scrollTop) el.scrollTop = top - (layout === "list" ? ROW_H : 0);
    else if (top + h > el.scrollTop + el.clientHeight) el.scrollTop = top + h - el.clientHeight + 8;
  }, [focusId]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <div className="resource-scroll" ref={ref}>
      {layout === "list" ? <ListBody {...props} vp={vp} /> : <GridBody {...props} vp={vp} />}
    </div>
  );
}

type BodyProps = Props & { vp: { width: number; height: number; scrollTop: number } };

function ListBody({ items, tagMap, selected, onSelect, onToggleFavorite, vp }: BodyProps) {
  const start = Math.max(0, Math.floor(vp.scrollTop / ROW_H) - 10);
  const end = Math.min(items.length, Math.ceil((vp.scrollTop + vp.height) / ROW_H) + 10);
  return (
    <div className="list" style={{ height: (items.length + 1) * ROW_H }}>
      <div className="list-row list-head">
        <span className="c-fav" />
        <span className="c-name">Name</span>
        <span className="c-type">Type</span>
        <span className="c-app">Application</span>
        <span className="c-status">Status</span>
        <span className="c-tags">Tags</span>
        <span className="c-size">Size</span>
        <span className="c-date">Modified</span>
      </div>
      {items.slice(start, end).map((r, i) => {
        const idx = start + i;
        const tagsText = [...tagsOf(r, tagMap, "function"), ...tagsOf(r, tagMap, "personal")].map((t) => t.name).join(", ");
        return (
          <div
            key={r.id}
            className={`list-row ${selected.has(r.id) ? "selected" : ""}`}
            style={{ transform: `translateY(${(idx + 1) * ROW_H}px)` }}
            onClick={(e) => onSelect(r.id, e)}
          >
            <span className="c-fav">
              <button
                className={`fav-btn ${r.favorite ? "on" : ""}`}
                onClick={(e) => {
                  e.stopPropagation();
                  onToggleFavorite(r);
                }}
                aria-label="Favorite"
              >
                <Icon name="star" size={14} filled={r.favorite} />
              </button>
            </span>
            <span className="c-name">
              <span className="kind-ico"><KindIcon kinds={r.kinds} size={15} /></span>
              <span className="truncate">{r.name}</span>
              {r.unavailable_count > 0 && <span className="badge warn" title="Có nguồn không truy cập được">offline</span>}
              {r.has_notes && <span className="note-dot" title="Có ghi chú" />}
            </span>
            <span className="c-type truncate">{tagsOf(r, tagMap, "type").map((t) => t.name).join(", ") || <em>—</em>}</span>
            <span className="c-app truncate">{tagsOf(r, tagMap, "app").map((t) => t.name).join(", ") || <em>—</em>}</span>
            <span className="c-status truncate">{tagsOf(r, tagMap, "status").map((t) => t.name).join(", ")}</span>
            <span className="c-tags truncate">{tagsText}</span>
            <span className="c-size">{formatSize(r.size)}</span>
            <span className="c-date">{formatDate(r.modified_at)}</span>
          </div>
        );
      })}
    </div>
  );
}

function GridBody({ items, tagMap, selected, onSelect, onToggleFavorite, vp }: BodyProps) {
  const cols = Math.max(1, Math.floor((vp.width - PAD * 2 + GAP) / (CARD_W + GAP)));
  const cardW = (vp.width - PAD * 2 - GAP * (cols - 1)) / cols;
  const rows = Math.ceil(items.length / cols);
  const rowH = CARD_H + GAP;
  const startRow = Math.max(0, Math.floor((vp.scrollTop - PAD) / rowH) - 3);
  const endRow = Math.min(rows, Math.ceil((vp.scrollTop + vp.height) / rowH) + 3);
  const visible = items.slice(startRow * cols, endRow * cols);

  return (
    <div className="grid" style={{ height: PAD * 2 + rows * rowH }}>
      {visible.map((r, i) => {
        const idx = startRow * cols + i;
        const row = Math.floor(idx / cols);
        const col = idx % cols;
        const types = tagsOf(r, tagMap, "type");
        const apps = tagsOf(r, tagMap, "app");
        return (
          <div
            key={r.id}
            className={`card ${selected.has(r.id) ? "selected" : ""}`}
            style={{ width: cardW, height: CARD_H, transform: `translate(${PAD + col * (cardW + GAP)}px, ${PAD + row * rowH}px)` }}
            onClick={(e) => onSelect(r.id, e)}
          >
            <CoverThumb resourceId={r.id} fallback={<KindIcon kinds={r.kinds} size={34} />} />
            <button
              className={`fav-btn card-fav ${r.favorite ? "on" : ""}`}
              onClick={(e) => {
                e.stopPropagation();
                onToggleFavorite(r);
              }}
              aria-label="Favorite"
            >
              <Icon name="star" size={14} filled={r.favorite} />
            </button>
            <div className="card-name" title={r.name}>
              <span className="card-kind"><KindIcon kinds={r.kinds} size={14} /></span> {r.name}
            </div>
            <div className="card-chips">
              {types.slice(0, 2).map((t) => (
                <span key={t.id} className={`chip small k-type ${r.auto_tag_ids.includes(t.id) ? "auto" : ""}`}>{t.name}</span>
              ))}
              {apps.slice(0, 2).map((t) => (
                <span key={t.id} className={`chip small k-app ${r.auto_tag_ids.includes(t.id) ? "auto" : ""}`} title={t.name}>
                  <AppLogo name={t.name} size={11} /> {t.name}
                </span>
              ))}
              {types.length + apps.length === 0 && <span className="chip small ghost">Unclassified</span>}
            </div>
            <div className="card-meta">
              <span>{formatSize(r.size)}</span>
              {r.source_count > 1 && <span className="badge">{r.source_count} nguồn</span>}
              {r.unavailable_count > 0 && <span className="badge warn">offline</span>}
              {r.has_notes && <span className="note-dot" title="Có ghi chú" />}
            </div>
          </div>
        );
      })}
    </div>
  );
}
