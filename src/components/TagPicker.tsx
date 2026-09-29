import { useEffect, useMemo, useRef, useState } from "react";
import { Tag, TagKind } from "../api";
import { Icon } from "./Icon";
import { AppLogo } from "./AppLogo";

interface Props {
  kind: TagKind | "any";
  tags: Tag[];
  selected: number[];
  onToggle: (tag: Tag, on: boolean) => void;
  onCreate: (kind: TagKind, name: string) => void;
  label?: string;
  allowCreate?: boolean;
}

/** Nút "+" mở dropdown tìm/chọn/tạo tag của một nhóm. */
export function TagPicker({ kind, tags, selected, onToggle, onCreate, label = "Thêm", allowCreate = true }: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open]);

  const options = useMemo(() => {
    const q = query.trim().toLowerCase();
    return tags
      .filter((t) => (kind === "any" ? t.kind === "function" || t.kind === "personal" : t.kind === kind))
      .filter((t) => !q || t.name.toLowerCase().includes(q));
  }, [tags, kind, query]);

  const exact = options.some((t) => t.name.toLowerCase() === query.trim().toLowerCase());
  const createKind: TagKind = kind === "any" ? "function" : kind;
  const canCreate = allowCreate && query.trim() !== "" && !exact;

  useEffect(() => setActive(0), [query, open]);

  const choose = (t: Tag) => {
    onToggle(t, !selected.includes(t.id));
  };

  return (
    <div className="picker" ref={ref}>
      <button className="chip chip-add" onClick={() => setOpen((o) => !o)} title={label}>
        <Icon name="plus" size={12} /> {label}
      </button>
      {open && (
        <div className="picker-pop">
          <input
            autoFocus
            className="picker-input"
            placeholder="Tìm hoặc tạo mới…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setOpen(false);
              else if (e.key === "ArrowDown") {
                e.preventDefault();
                setActive((a) => Math.min(a + 1, options.length - (canCreate ? 0 : 1)));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setActive((a) => Math.max(a - 1, 0));
              } else if (e.key === "Enter") {
                e.preventDefault();
                if (active < options.length) choose(options[active]);
                else if (canCreate) {
                  onCreate(createKind, query.trim());
                  setQuery("");
                }
              }
            }}
          />
          <div className="picker-list">
            {options.map((t, i) => (
              <button
                key={t.id}
                className={`picker-item ${i === active ? "active" : ""}`}
                onMouseEnter={() => setActive(i)}
                onClick={() => choose(t)}
              >
                <span className={`check ${selected.includes(t.id) ? "on" : ""}`}>
                  {selected.includes(t.id) && <Icon name="check" size={12} />}
                </span>
                {t.kind === "app" && <AppLogo name={t.name} size={14} />}
                <span className="picker-name">{t.name}</span>
                {kind === "any" && <span className="picker-kind">{t.kind}</span>}
                <span className="picker-count">{t.count}</span>
              </button>
            ))}
            {canCreate && (
              <button
                className={`picker-item create ${active === options.length ? "active" : ""}`}
                onClick={() => {
                  onCreate(createKind, query.trim());
                  setQuery("");
                }}
              >
                <Icon name="plus" size={12} /> Tạo “{query.trim()}”
              </button>
            )}
            {options.length === 0 && !canCreate && <div className="picker-empty">Không có mục nào</div>}
          </div>
        </div>
      )}
    </div>
  );
}
