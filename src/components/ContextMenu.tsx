import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Icon } from "./Icon";

export type MenuItem = { label: string; icon?: string; onClick: () => void; danger?: boolean; disabled?: boolean } | "sep";

export interface MenuState {
  x: number;
  y: number;
  items: MenuItem[];
}

/** Menu chuột phải: đóng khi bấm ra ngoài, Esc, cuộn hoặc đổi kích thước cửa sổ. */
export function ContextMenu({ menu, onClose }: { menu: MenuState | null; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x: 0, y: 0 });

  useLayoutEffect(() => {
    if (!menu || !ref.current) return;
    const r = ref.current.getBoundingClientRect();
    setPos({ x: Math.min(menu.x, window.innerWidth - r.width - 6), y: Math.min(menu.y, window.innerHeight - r.height - 6) });
  }, [menu]);

  useEffect(() => {
    if (!menu) return;
    const close = (e: Event) => {
      if (e instanceof MouseEvent && ref.current?.contains(e.target as Node)) return;
      onClose();
    };
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("mousedown", close, true);
    window.addEventListener("wheel", close, true);
    window.addEventListener("resize", close);
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("mousedown", close, true);
      window.removeEventListener("wheel", close, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("keydown", key);
    };
  }, [menu, onClose]);

  if (!menu) return null;
  return (
    <div ref={ref} className="ctx-menu" style={{ left: pos.x || menu.x, top: pos.y || menu.y }} role="menu" onContextMenu={(e) => e.preventDefault()}>
      {menu.items.map((it, i) =>
        it === "sep" ? (
          <div key={i} className="ctx-sep" />
        ) : (
          <button
            key={i}
            role="menuitem"
            className={`ctx-item ${it.danger ? "danger" : ""}`}
            disabled={it.disabled}
            onClick={() => {
              onClose();
              it.onClick();
            }}
          >
            <span className="ctx-icon">{it.icon && <Icon name={it.icon} size={14} />}</span>
            {it.label}
          </button>
        ),
      )}
    </div>
  );
}
