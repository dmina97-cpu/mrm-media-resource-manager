import { useState } from "react";
import { AssetCounts, KIND_LABEL, Library, Tag, TagKind } from "../api";
import { Icon } from "./Icon";
import { AppLogo } from "./AppLogo";
import type { Route } from "../App";

interface Props {
  route: Route;
  onNavigate: (r: Route) => void;
  tags: Tag[];
  libraries: Library[];
  counts: { all: number; unclassified: number; review: number; favorites: number; missing: number; matches: number };
  mediaCounts: AssetCounts | null;
  pluginActive: boolean;
}

const GROUPS: TagKind[] = ["app", "type", "function", "personal", "status"];

export function Sidebar({ route, onNavigate, tags, libraries, counts, mediaCounts, pluginActive }: Props) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({ personal: false, status: true });
  const [showEmpty, setShowEmpty] = useState<Record<string, boolean>>({});

  const isActive = (r: Route) => JSON.stringify(r) === JSON.stringify(route);

  const Item = ({ r, icon, label, count, warn, logo }: { r: Route; icon: string; label: string; count?: number; warn?: boolean; logo?: boolean }) => (
    <button className={`nav-item ${isActive(r) ? "active" : ""}`} onClick={() => onNavigate(r)}>
      {logo ? <AppLogo name={label} size={16} /> : <Icon name={icon} size={15} />}
      <span className="nav-label">{label}</span>
      {count !== undefined && count > 0 && <span className={`nav-count ${warn ? "warn" : ""}`}>{count}</span>}
    </button>
  );

  return (
    <nav className="sidebar">
      <div className="brand">
        <div className="brand-mark">M</div>
        <div>
          <div className="brand-name">MRM</div>
          <div className="brand-sub">Media Resource Manager</div>
        </div>
      </div>

      <div className="nav-scroll">
        <div className="nav-section">
          <Item r={{ page: "dashboard" }} icon="dashboard" label="Dashboard" />
          <Item r={{ page: "resources", view: "all" }} icon="all" label="All Resources" count={counts.all} />
          <Item r={{ page: "resources", view: "unclassified" }} icon="inbox" label="Unclassified" count={counts.unclassified} warn />
          <Item r={{ page: "resources", view: "review" }} icon="check" label="Cần rà soát" count={counts.review} warn />
          <Item r={{ page: "resources", view: "favorites" }} icon="star" label="Favorites" count={counts.favorites} />
          <Item r={{ page: "matches" }} icon="link" label="Match suggestions" count={counts.matches} warn />
          {counts.missing > 0 && <Item r={{ page: "resources", view: "missing" }} icon="alert" label="Unavailable" count={counts.missing} />}
          <Item r={{ page: "cleanup" }} icon="broom" label="Dọn dẹp" />
          <Item r={{ page: "rules" }} icon="rule" label="Rules" />
        </div>

        {mediaCounts && mediaCounts.audio + mediaCounts.image + mediaCounts.video > 0 && (
          <div className="nav-section">
            <div className="nav-heading" title={pluginActive ? "Đang được sử dụng trong DaVinci" : undefined}>
              Media Browser {pluginActive && <span className="nav-badge">DaVinci</span>}
            </div>
            <Item r={{ page: "media", view: "audio" }} icon="music" label="Âm thanh" count={mediaCounts.audio} />
            <Item r={{ page: "media", view: "image" }} icon="image" label="Hình ảnh" count={mediaCounts.image} />
            <Item r={{ page: "media", view: "video" }} icon="film" label="Video" count={mediaCounts.video} />
            <Item r={{ page: "media", view: "favorites" }} icon="star" label="Media yêu thích" count={mediaCounts.favorites} />
            <Item r={{ page: "media", view: "recent" }} icon="refresh" label="Dùng gần đây" count={mediaCounts.recent} />
          </div>
        )}

        {GROUPS.map((kind) => {
          let list = tags.filter((t) => t.kind === kind);
          // Type: ẩn loại chưa dùng. Function: tag 0 resource thường là tag của file media (plugin DaVinci / Media Browser)
          const hidable = kind === "type" || kind === "function";
          const hidden = hidable && !showEmpty[kind] ? list.filter((t) => t.count === 0).length : 0;
          if (hidable && !showEmpty[kind]) list = list.filter((t) => t.count > 0);
          if (list.length === 0 && (kind === "function" || kind === "personal") && hidden === 0) {
            return (
              <div className="nav-section" key={kind}>
                <div className="nav-heading">{KIND_LABEL[kind]}</div>
                <div className="nav-hint">Tạo tag từ Inspector</div>
              </div>
            );
          }
          return (
            <div className="nav-section" key={kind}>
              <button className="nav-heading clickable" onClick={() => setCollapsed((c) => ({ ...c, [kind]: !c[kind] }))}>
                <span className={`caret ${collapsed[kind] ? "" : "open"}`}>›</span>
                {KIND_LABEL[kind]}
              </button>
              {!collapsed[kind] && (
                <>
                  {list.map((t) => (
                    <Item key={t.id} r={{ page: "resources", view: "tag", tagId: t.id }} icon="tag" label={t.name} count={t.count} logo={kind === "app"} />
                  ))}
                  {hidden > 0 && (
                    <button className="nav-more" onClick={() => setShowEmpty((s) => ({ ...s, [kind]: true }))}>
                      + {hidden} {kind === "type" ? "loại chưa dùng" : "tag chỉ dùng cho file media"}
                    </button>
                  )}
                  {(kind === "type" || kind === "function") && showEmpty[kind] && (
                    <button className="nav-more" onClick={() => setShowEmpty((s) => ({ ...s, [kind]: false }))}>
                      {kind === "type" ? "Ẩn loại chưa dùng" : "Ẩn tag của file media"}
                    </button>
                  )}
                </>
              )}
            </div>
          );
        })}

        <div className="nav-section">
          <div className="nav-heading">Libraries</div>
          {libraries.map((l) => (
            <button
              key={l.id}
              className={`nav-item ${isActive({ page: "resources", view: "library", libraryId: l.id }) ? "active" : ""}`}
              onClick={() => onNavigate({ page: "resources", view: "library", libraryId: l.id })}
              title={l.path}
            >
              <Icon name="hdd" size={15} />
              <span className="nav-label">{l.name}</span>
              <span className={`dot ${l.online ? "on" : "off"}`} title={l.online ? "Online" : "Offline"} />
            </button>
          ))}
        </div>
      </div>

      <div className="sidebar-footer">
        <Item r={{ page: "about" }} icon="info" label="Giới thiệu & Hướng dẫn" />
        <Item r={{ page: "settings" }} icon="settings" label="Libraries & Settings" />
      </div>
    </nav>
  );
}
