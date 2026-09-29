import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open, message } from "@tauri-apps/plugin-dialog";
import { AiProgress, AiStatus, api, AssetCounts, Library, ListView, Presence, ResourceSummary, ScanProgress, SortKey, Tag } from "./api";
import { errorText, formatNumber, formatSize } from "./format";
import { Sidebar } from "./components/Sidebar";
import { ResourceView } from "./components/ResourceView";
import { Inspector } from "./components/Inspector";
import { Dashboard } from "./components/Dashboard";
import { Matches } from "./components/Matches";
import { Settings } from "./components/Settings";
import { Icon } from "./components/Icon";
import { Cleanup } from "./components/Cleanup";
import { Rules } from "./components/Rules";
import { About } from "./components/About";
import { invalidateCovers } from "./components/Media";
import { BulkSuggest } from "./components/BulkSuggest";
import { MediaBrowser, MediaView } from "./components/MediaBrowser";
import { UpdateBanner } from "./components/Updates";
import { Onboarding } from "./components/Onboarding";
import "./styles.css";
import "./styles-media.css";

export type Route =
  | { page: "dashboard" }
  | { page: "resources"; view: ListView; tagId?: number; libraryId?: number }
  | { page: "media"; view: MediaView; resourceId?: number; resourceName?: string }
  | { page: "matches" }
  | { page: "cleanup" }
  | { page: "rules" }
  | { page: "about" }
  | { page: "settings" };

interface Toast {
  id: number;
  msg: string;
  kind: "ok" | "err";
}

function loadPref<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v ? (JSON.parse(v) as T) : fallback;
  } catch {
    return fallback;
  }
}
function savePref(key: string, v: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(v));
  } catch {
    /* ignore */
  }
}

export default function App() {
  const [route, setRoute] = useState<Route>({ page: "resources", view: "all" });
  const [tags, setTags] = useState<Tag[]>([]);
  const [libraries, setLibraries] = useState<Library[]>([]);
  const [items, setItems] = useState<ResourceSummary[]>([]);
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<SortKey>(() => loadPref("sort", "name"));
  const [desc, setDesc] = useState<boolean>(() => loadPref("desc", false));
  const [layout, setLayout] = useState<"grid" | "list">(() => loadPref("layout", "list"));
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [focusId, setFocusId] = useState<number | null>(null);
  const [anchor, setAnchor] = useState<number | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [counts, setCounts] = useState({ all: 0, unclassified: 0, review: 0, favorites: 0, missing: 0, matches: 0 });
  const [bulkSuggest, setBulkSuggest] = useState<{ ids: number[]; label: string } | null>(null);
  const [progress, setProgress] = useState<Record<number, ScanProgress>>({});
  const [scanning, setScanning] = useState<Set<number>>(new Set());
  const [toasts, setToasts] = useState<Toast[]>([]);
  const searchRef = useRef<HTMLInputElement>(null);
  const [ai, setAi] = useState<AiStatus | null>(null);
  const [aiProgress, setAiProgress] = useState<AiProgress | null>(null);
  const [searchSort, setSearchSort] = useState<SortKey>("relevance");
  const refreshAi = useCallback(() => {
    api.aiStatus().then((s) => {
      setAi(s);
      setAiProgress(s.progress);
    });
  }, []);
  const aiReady = !!ai && ai.enabled && ai.running && ai.embed_ready && ai.chat_ready;
  const [aiPending, setAiPending] = useState(0);
  useEffect(() => {
    api.aiPendingCount().then(setAiPending).catch(() => undefined);
  }, [reloadKey]);

  useEffect(() => savePref("sort", sort), [sort]);
  useEffect(() => savePref("desc", desc), [desc]);
  useEffect(() => savePref("layout", layout), [layout]);

  const toast = useCallback((msg: string, kind: "ok" | "err" = "ok") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, msg, kind }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), kind === "err" ? 6000 : 2500);
  }, []);

  const refresh = useCallback(() => setReloadKey((k) => k + 1), []);

  // Media Browser: số đếm + trạng thái plugin DaVinci; mediaKey chỉ đổi khi thư viện được quét lại
  const [mediaCounts, setMediaCounts] = useState<AssetCounts | null>(null);
  const [mediaKey, setMediaKey] = useState(0);
  const [presence, setPresence] = useState<Presence | null>(null);
  // Hướng dẫn lần đầu: chỉ khi chưa có library nào và chưa tắt
  const [onboarding, setOnboarding] = useState(false);
  const [onboardingChecked, setOnboardingChecked] = useState(false);
  const refreshMediaCounts = useCallback(() => {
    api.assetCounts().then(setMediaCounts).catch(() => undefined);
  }, []);
  useEffect(() => {
    refreshMediaCounts();
    const t = setInterval(refreshMediaCounts, 20000);
    return () => clearInterval(t);
  }, [refreshMediaCounts, reloadKey, mediaKey]);
  useEffect(() => {
    api.getPresence().then(setPresence).catch(() => undefined);
    const un = listen<Presence>("plugin-presence", (e) => setPresence(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, []);

  // Database từng bị hỏng và đã được tự khôi phục khi khởi động -> báo rõ cho người dùng
  useEffect(() => {
    api.takeDbNotice().then((n) => n && message(n, { title: "MRM – Khôi phục database", kind: "warning" })).catch(() => undefined);
  }, []);

  // Dữ liệu dùng chung: tags, libraries, số đếm sidebar
  useEffect(() => {
    api.listTags().then(setTags);
    api.listLibraries().then((libs) => {
      setLibraries(libs);
      if (!onboardingChecked) {
        setOnboardingChecked(true);
        if (libs.length === 0) api.getPref("show_onboarding").then((on) => on && setOnboarding(true)).catch(() => undefined);
      }
    });
    api.getDashboard().then((d) =>
      setCounts({ all: d.resources, unclassified: d.unclassified, review: d.review, favorites: d.favorites, missing: d.missing, matches: d.pending_matches }),
    );
  }, [reloadKey]);

  // Danh sách resource
  const searching = search.trim() !== "";
  const effectiveSort: SortKey = searching ? searchSort : sort === "relevance" ? "name" : sort;
  const query = useMemo(() => {
    if (route.page !== "resources") return null;
    return { view: route.view, tag_id: route.tagId ?? null, library_id: route.libraryId ?? null, search, sort: effectiveSort, desc };
  }, [route, search, effectiveSort, desc]);

  useEffect(() => {
    if (!query) return;
    let cancelled = false;
    const t = setTimeout(
      () =>
        api
          .queryResources(query)
          .then((r) => !cancelled && setItems(r))
          .catch((err) => toast(errorText(err), "err")),
      search ? 180 : 0,
    );
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [query, reloadKey, toast]); // eslint-disable-line react-hooks/exhaustive-deps

  // Bỏ chọn các mục không còn trong danh sách
  useEffect(() => {
    const ids = new Set(items.map((i) => i.id));
    setSelected((s) => {
      const next = new Set([...s].filter((id) => ids.has(id)));
      return next.size === s.size ? s : next;
    });
  }, [items]);

  // AI status + backend events
  useEffect(() => {
    refreshAi();
    const t = setInterval(refreshAi, 15000);
    const subs = [
      listen<AiProgress>("ai-progress", (e) => setAiProgress(e.payload)),
      listen("ai-index-done", () => refreshAi()),
      listen<number>("ai-analyzed", () => refresh()),
      listen("library-updated", () => {
        invalidateCovers();
        refresh();
        setMediaKey((k) => k + 1);
      }),
      listen("library-status", () => {
        refresh();
        setMediaKey((k) => k + 1);
      }),
    ];
    return () => {
      clearInterval(t);
      subs.forEach((p) => p.then((f) => f()));
    };
  }, [refreshAi, refresh]);

  // Scan progress events
  useEffect(() => {
    const un = listen<ScanProgress>("scan-progress", (e) => {
      setProgress((p) => {
        const next = { ...p };
        if (e.payload.phase === "done") delete next[e.payload.library_id];
        else next[e.payload.library_id] = e.payload;
        return next;
      });
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const scan = useCallback(
    async (id: number, full: boolean) => {
      setScanning((s) => new Set(s).add(id));
      try {
        const r = await api.scanLibrary(id, full);
        if (!r.online) toast("Library đang offline — metadata vẫn được giữ", "err");
        else
          toast(
            `Quét xong: ${formatNumber(r.found)} mục · +${r.added} mới · ${r.auto_merged} tự ghép · ${r.suggested} đề xuất${r.regrouped ? ` · ${r.regrouped} nhóm lại` : ""}${r.missing ? ` · ${r.missing} mất` : ""}`,
          );
      } catch (err) {
        toast(errorText(err), "err");
      } finally {
        setMediaKey((k) => k + 1);
        setScanning((s) => {
          const n = new Set(s);
          n.delete(id);
          return n;
        });
        invalidateCovers();
        refresh();
      }
    },
    [refresh, toast],
  );

  const scanAll = () => libraries.filter((l) => l.online).forEach((l) => scan(l.id, false));

  const addLibrary = async () => {
    try {
      const dir = await open({ directory: true, multiple: false, title: "Chọn thư mục tài nguyên" });
      if (typeof dir !== "string") return;
      const lib = await api.addLibrary(dir);
      refresh();
      setRoute({ page: "resources", view: "all" });
      scan(lib.id, false);
    } catch (err) {
      toast(errorText(err), "err");
    }
  };

  const navigate = (r: Route) => {
    setRoute(r);
    setSelected(new Set());
    setFocusId(null);
  };

  const select = (id: number, e: React.MouseEvent | { ctrlKey?: boolean; metaKey?: boolean; shiftKey?: boolean }) => {
    if (e.shiftKey && anchor !== null) {
      const a = items.findIndex((i) => i.id === anchor);
      const b = items.findIndex((i) => i.id === id);
      const [lo, hi] = a < b ? [a, b] : [b, a];
      setSelected(new Set(items.slice(lo, hi + 1).map((i) => i.id)));
      setFocusId(id);
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      setSelected((s) => {
        const n = new Set(s);
        if (n.has(id)) n.delete(id);
        else n.add(id);
        return n;
      });
      setFocusId(id);
      setAnchor(id);
      return;
    }
    setSelected(new Set([id]));
    setFocusId(id);
    setAnchor(id);
  };

  const toggleFavorite = async (r: ResourceSummary) => {
    await api.setFavorite([r.id], !r.favorite);
    refresh();
  };

  // Next unclassified: mục tiếp theo trong danh sách hiện tại
  const nextUnclassified = useMemo(() => {
    if (route.page !== "resources" || route.view !== "unclassified" || focusId === null || selected.size > 1) return null;
    return () => {
      const idx = items.findIndex((i) => i.id === focusId);
      const next = items[idx + 1] ?? items.find((i) => i.id !== focusId);
      if (next) select(next.id, {});
    };
  }, [route, focusId, items, selected.size]); // eslint-disable-line react-hooks/exhaustive-deps

  // Phím tắt
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      const typing = target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT";
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        if (route.page !== "resources") navigate({ page: "resources", view: "all" });
        setTimeout(() => searchRef.current?.focus(), 0);
        return;
      }
      if (typing || route.page !== "resources") return;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        if (items.length === 0) return;
        const idx = focusId === null ? -1 : items.findIndex((i) => i.id === focusId);
        const nextIdx = e.key === "ArrowDown" ? Math.min(items.length - 1, idx + 1) : Math.max(0, idx - 1);
        select(items[nextIdx].id, { shiftKey: e.shiftKey });
      } else if (e.key === "Escape") {
        setSelected(new Set());
        setFocusId(null);
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
        e.preventDefault();
        setSelected(new Set(items.map((i) => i.id)));
      } else if (e.key.toLowerCase() === "f" && focusId !== null) {
        const r = items.find((i) => i.id === focusId);
        if (r) toggleFavorite(r);
      } else if (e.key.toLowerCase() === "n" && nextUnclassified) {
        nextUnclassified();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const tagMap = useMemo(() => new Map(tags.map((t) => [t.id, t])), [tags]);
  const selection = useMemo(() => items.filter((i) => selected.has(i.id)), [items, selected]);

  const title = (() => {
    if (route.page !== "resources") return "";
    switch (route.view) {
      case "all":
        return "All Resources";
      case "unclassified":
        return "Unclassified";
      case "favorites":
        return "Favorites";
      case "missing":
        return "Unavailable";
      case "review":
        return "Cần rà soát";
      case "tag":
        return tagMap.get(route.tagId ?? -1)?.name ?? "Tag";
      case "library":
        return libraries.find((l) => l.id === route.libraryId)?.name ?? "Library";
    }
  })();

  const activeProgress = Object.values(progress);
  const showAiBar = !!aiProgress && !["idle", ""].includes(aiProgress.phase);
  const totalSize = items.reduce((a, r) => a + r.size, 0);

  return (
    <div className="app">
      <Sidebar route={route} onNavigate={navigate} tags={tags} libraries={libraries} counts={counts} mediaCounts={mediaCounts} pluginActive={!!presence?.plugin_active} />

      <main className="main">
        <UpdateBanner toast={toast} />
        {route.page === "resources" && (
          <>
            <header className="toolbar">
              <div className="toolbar-title">
                <h1>{title}</h1>
                <span className="muted">
                  {formatNumber(items.length)} resource · {formatSize(totalSize)}
                </span>
              </div>
              <div className="search">
                <Icon name="search" size={15} />
                <input
                  ref={searchRef}
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder={aiReady ? "Tìm theo ý nghĩa… (vd: plugin theo dõi người, app:resolve)" : "Tìm… (vd: light leak resolve, app:resolve type:plugin)"}
                  spellCheck={false}
                  onKeyDown={(e) => {
                    if (e.key === "Escape") {
                      setSearch("");
                      (e.target as HTMLInputElement).blur();
                    }
                  }}
                />
                {aiReady && (
                  <span className="ai-chip" title="Semantic search đang bật — kết hợp từ khóa và ý nghĩa">
                    <Icon name="sparkles" size={11} /> AI
                  </span>
                )}
                {search && (
                  <button className="icon-btn" onClick={() => setSearch("")} aria-label="Xóa tìm kiếm">
                    <Icon name="x" size={13} />
                  </button>
                )}
              </div>
              <div className="toolbar-actions">
                <select
                  value={effectiveSort}
                  onChange={(e) => (searching ? setSearchSort(e.target.value as SortKey) : setSort(e.target.value as SortKey))}
                  aria-label="Sắp xếp"
                >
                  {searching && <option value="relevance">Liên quan</option>}
                  <option value="name">Tên</option>
                  <option value="size">Dung lượng</option>
                  <option value="modified">Ngày sửa file</option>
                  <option value="created">Ngày thêm</option>
                  <option value="updated">Cập nhật gần đây</option>
                </select>
                <button className="icon-btn" onClick={() => setDesc((d) => !d)} title={desc ? "Giảm dần" : "Tăng dần"}>
                  <Icon name={desc ? "sortDesc" : "sortAsc"} size={16} />
                </button>
                <div className="segmented">
                  <button className={layout === "list" ? "on" : ""} onClick={() => setLayout("list")} aria-label="List view">
                    <Icon name="list" size={15} />
                  </button>
                  <button className={layout === "grid" ? "on" : ""} onClick={() => setLayout("grid")} aria-label="Grid view">
                    <Icon name="grid" size={15} />
                  </button>
                </div>
                {route.view === "review" && items.length > 0 && (
                  <button
                    className="btn"
                    title="Chấp nhận toàn bộ tag tự động của các mục đang hiển thị (thành tag của bạn)"
                    onClick={async () => {
                      if (!confirm(`Xác nhận toàn bộ tag tự động của ${items.length} resource?`)) return;
                      const n = await api.confirmAutoTags(items.map((i) => i.id));
                      toast(`Đã xác nhận ${n} tag`);
                      refresh();
                    }}
                  >
                    <Icon name="check" size={14} /> Xác nhận tất cả
                  </button>
                )}
                {items.length > 0 && (
                  <button
                    className="btn"
                    title="Tự gắn tag App / Type theo gợi ý cho nhiều resource cùng lúc"
                    onClick={() =>
                      setBulkSuggest(
                        selected.size > 1
                          ? { ids: [...selected], label: `${selected.size} mục đang chọn` }
                          : { ids: items.map((i) => i.id), label: `danh sách “${title}”` },
                      )
                    }
                  >
                    <Icon name="sparkles" size={14} /> Áp dụng gợi ý
                  </button>
                )}
                {aiPending > 0 && (
                  <button
                    className="btn primary"
                    title="Áp dụng toàn bộ tag + mô tả AI đã đề xuất. Chưa chuẩn thì chỉnh lại sau — tìm lại bằng is:ai"
                    onClick={async () => {
                      if (!confirm(`Áp dụng kết quả AI cho ${aiPending} resource?\n\nAI sẽ gắn App/Type/Function tag và thêm mô tả + cách cài vào Notes. Mục nào chưa chuẩn bạn chỉnh lại sau (tìm bằng is:ai).`)) return;
                      try {
                        const n = await api.aiApply();
                        toast(`Đã áp dụng kết quả AI cho ${n} resource`);
                      } catch (err) {
                        toast(errorText(err), "err");
                      }
                      refresh();
                    }}
                  >
                    <Icon name="check" size={14} /> Áp dụng AI ({aiPending})
                  </button>
                )}
                {aiReady && route.view === "unclassified" && items.length > 0 && (
                  <button
                    className="btn"
                    disabled={!!ai?.analyzing}
                    title="AI đề xuất mô tả + tag cho các mục chưa phân loại (tối đa 100 mục mỗi lần)"
                    onClick={() => {
                      const ids = items.slice(0, 100).map((i) => i.id);
                      toast(`AI bắt đầu phân tích ${ids.length} mục`);
                      api.aiAnalyze(ids).then(refreshAi, (err) => toast(errorText(err), "err"));
                      setTimeout(refreshAi, 300);
                    }}
                  >
                    <Icon name="sparkles" size={14} /> Phân tích AI
                  </button>
                )}
                <button className="btn" onClick={scanAll} disabled={libraries.length === 0 || scanning.size > 0} title="Quét lại tất cả library">
                  <Icon name="refresh" size={14} /> Quét
                </button>
              </div>
            </header>

            {libraries.length === 0 ? (
              <div className="empty-state big">
                <div className="brand-mark large">M</div>
                <h2>Chào mừng đến MRM</h2>
                <p>Thêm thư mục chứa plugin, LUT, SFX, template… để bắt đầu lập chỉ mục.</p>
                <p className="muted small">App chỉ đọc — không di chuyển, đổi tên hay xóa file thật.</p>
                <button className="btn primary" onClick={addLibrary}>
                  <Icon name="plus" size={14} /> Thêm Library
                </button>
              </div>
            ) : items.length === 0 ? (
              <div className="empty-state">
                <Icon name={search ? "search" : "inbox"} size={28} />
                <p>{search ? "Không tìm thấy resource nào." : route.view === "unclassified" ? "Tất cả đã được phân loại 🎉" : "Chưa có resource nào ở đây."}</p>
              </div>
            ) : (
              <ResourceView
                items={items}
                tagMap={tagMap}
                layout={layout}
                selected={selected}
                focusId={focusId}
                onSelect={select}
                onToggleFavorite={toggleFavorite}
              />
            )}
          </>
        )}
        {route.page === "media" && (
          <MediaBrowser
            view={route.view}
            resource={route.resourceId ? { id: route.resourceId, name: route.resourceName ?? "" } : null}
            onClearResource={() => setRoute({ page: "media", view: route.view })}
            libraries={libraries}
            reloadKey={mediaKey}
            presence={presence}
            pendingMeta={mediaCounts?.pending_meta ?? 0}
            aiReady={aiReady}
            onChanged={refreshMediaCounts}
            onOpenResource={(id) => {
              setRoute({ page: "resources", view: "all" });
              setSearch("");
              setSelected(new Set([id]));
              setFocusId(id);
            }}
            toast={toast}
          />
        )}
        {route.page === "dashboard" && <Dashboard reloadKey={reloadKey} onNavigate={navigate} />}
        {route.page === "matches" && <Matches reloadKey={reloadKey} onChanged={refresh} toast={toast} />}
        {route.page === "cleanup" && (
          <Cleanup
            reloadKey={reloadKey}
            onChanged={refresh}
            toast={toast}
            onOpen={(id) => {
              setRoute({ page: "resources", view: "all" });
              setSelected(new Set([id]));
              setFocusId(id);
            }}
          />
        )}
        {route.page === "about" && <About toast={toast} onShowOnboarding={() => setOnboarding(true)} />}
        {route.page === "rules" && <Rules tags={tags} reloadKey={reloadKey} onChanged={refresh} toast={toast} />}
        {route.page === "settings" && (
          <Settings
            libraries={libraries}
            scanning={scanning}
            reloadKey={reloadKey}
            onAddLibrary={addLibrary}
            onScan={scan}
            onChanged={refresh}
            toast={toast}
            ai={ai}
            refreshAi={refreshAi}
          />
        )}

        {(activeProgress.length > 0 || showAiBar) && (
          <div className="statusbar">
            {activeProgress.map((p) => {
              const lib = libraries.find((l) => l.id === p.library_id);
              const pct = p.total ? Math.round((p.done / p.total) * 100) : 0;
              const phase = { listing: "Đang liệt kê", inspecting: "Đang đọc metadata", saving: "Đang lưu", done: "" }[p.phase];
              return (
                <div key={p.library_id} className="scan-progress">
                  <span className="spinner" />
                  <span>
                    {lib?.name}: {phase} {p.total ? `${p.done}/${p.total}` : ""}
                  </span>
                  <span className="progress-track">
                    <span className="progress-fill" style={{ width: `${pct}%` }} />
                  </span>
                  <span className="muted truncate">{p.current}</span>
                </div>
              );
            })}
            {showAiBar && aiProgress && (
              <div className={`scan-progress ${aiProgress.phase === "error" ? "err" : ""}`}>
                {aiProgress.phase === "error" ? <Icon name="alert" size={12} /> : <span className="spinner" />}
                <span>
                  <Icon name="sparkles" size={11} /> {aiProgress.message}
                  {aiProgress.total > 0 && aiProgress.phase !== "downloading" && aiProgress.phase !== "pulling" ? ` ${aiProgress.done}/${aiProgress.total}` : ""}
                </span>
                <span className="progress-track">
                  <span className="progress-fill" style={{ width: `${aiProgress.total ? Math.round((aiProgress.done / aiProgress.total) * 100) : 0}%` }} />
                </span>
                {aiProgress.phase === "error" && (
                  <button className="link-btn" onClick={() => setAiProgress(null)}>
                    Đóng
                  </button>
                )}
              </div>
            )}
          </div>
        )}
      </main>

      {route.page === "resources" && (
        <Inspector
          focusId={focusId}
          selection={selection}
          tags={tags}
          reloadKey={reloadKey}
          onChanged={refresh}
          onFocus={(id) => {
            setSelected(new Set([id]));
            setFocusId(id);
          }}
          onNext={nextUnclassified}
          onOpenMedia={(view, resourceId, resourceName) => setRoute({ page: "media", view, resourceId, resourceName })}
          toast={toast}
          aiReady={aiReady}
        />
      )}

      {bulkSuggest && (
        <BulkSuggest ids={bulkSuggest.ids} scopeLabel={bulkSuggest.label} onClose={() => setBulkSuggest(null)} onDone={refresh} toast={toast} />
      )}

      {onboarding && (
        <Onboarding
          libraries={libraries}
          scanning={scanning}
          progress={progress}
          ai={ai}
          onScan={scan}
          onChanged={refresh}
          refreshAi={refreshAi}
          onOpenMedia={() => setRoute({ page: "media", view: "audio" })}
          onClose={() => setOnboarding(false)}
          toast={toast}
        />
      )}

      <div className="toasts" role="status">
        {toasts.map((t) => (
          <div key={t.id} className={`toast ${t.kind}`}>
            {t.msg}
          </div>
        ))}
      </div>
    </div>
  );
}
