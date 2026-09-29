import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { api, AssetAiStatus, AssetDetail, AssetFacets, AssetItem, AssetQuery, AssetSort, Library, MediaType, Presence, VisionStatus } from "../api";
import { errorText, formatDateTime, formatNumber, formatSize } from "../format";
import { Icon } from "./Icon";
import { CollectionPicker } from "./Collections";
import { invalidateCovers } from "./Media";

export type MediaView = MediaType | "favorites" | "recent";

export const MEDIA_VIEW_LABEL: Record<MediaView, string> = {
  audio: "Âm thanh",
  image: "Hình ảnh",
  video: "Video",
  favorites: "Media yêu thích",
  recent: "Dùng gần đây",
};

const PAGE = 200;
/** WebView2 phát trực tiếp được; định dạng khác đi qua preview_file/proxy. */
const AUDIO_NATIVE = new Set(["wav", "mp3", "flac", "ogg", "m4a", "aac"]);
const IMAGE_NATIVE = new Set(["jpg", "jpeg", "png", "gif", "webp", "bmp"]);

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

export function formatDuration(s: number | null | undefined) {
  if (!s || s <= 0) return "";
  if (s < 10) return `${s.toFixed(1)}s`;
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return `${m}:${sec.toString().padStart(2, "0")}`;
}

function joinPath(root: string, rel: string) {
  return /[\\/]$/.test(root) ? root + rel : `${root}\\${rel}`;
}

/** Dòng phụ: resource + thư mục chứa gần nhất (nhiều pack đặt tên file giống nhau: 01.wav…). */
function contextOf(it: AssetItem) {
  const dir = folderOf(it.rel_path);
  const leaf = dir.split(/[\\/]/).pop() ?? "";
  if (!it.resource_name) return dir;
  return leaf && leaf !== it.resource_name ? `${it.resource_name} · ${leaf}` : it.resource_name;
}

function folderOf(rel: string) {
  const i = Math.max(rel.lastIndexOf("\\"), rel.lastIndexOf("/"));
  return i > 0 ? rel.slice(0, i) : "";
}

// ---------------------------------------------------------------- thumbnails (cache dùng chung)

const thumbCache = new Map<number, string | null>();
const thumbPending = new Map<number, Promise<string | null>>();
function loadThumb(id: number): Promise<string | null> {
  if (thumbCache.has(id)) return Promise.resolve(thumbCache.get(id)!);
  let p = thumbPending.get(id);
  if (!p) {
    p = api
      .assetThumbnail(id, 360)
      .catch(() => null)
      .then((t) => {
        thumbCache.set(id, t);
        thumbPending.delete(id);
        return t;
      });
    thumbPending.set(id, p);
  }
  return p;
}

const spriteCache = new Map<number, string | null>();

function Thumb({ item }: { item: AssetItem }) {
  const ref = useRef<HTMLDivElement>(null);
  const [thumb, setThumb] = useState<string | null | undefined>(thumbCache.get(item.id));
  const [sprite, setSprite] = useState<string | null>(null);
  const [frame, setFrame] = useState(-1);

  useEffect(() => {
    if (thumb !== undefined || !item.available) return;
    const el = ref.current;
    if (!el) return;
    let alive = true;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          io.disconnect();
          loadThumb(item.id).then((t) => alive && setThumb(t));
        }
      },
      { rootMargin: "300px" },
    );
    io.observe(el);
    return () => {
      alive = false;
      io.disconnect();
    };
  }, [item.id, item.available, thumb]);

  const onEnter = () => {
    if (item.media_type !== "video" || !item.available) return;
    if (spriteCache.has(item.id)) return setSprite(spriteCache.get(item.id)!);
    api
      .assetSprite(item.id)
      .catch(() => null)
      .then((s) => {
        spriteCache.set(item.id, s);
        setSprite(s);
      });
  };

  return (
    <div
      ref={ref}
      className="mb-thumb"
      onMouseEnter={onEnter}
      onMouseMove={(e) => {
        if (!sprite) return;
        const r = e.currentTarget.getBoundingClientRect();
        setFrame(Math.min(9, Math.max(0, Math.floor(((e.clientX - r.left) / r.width) * 10))));
      }}
      onMouseLeave={() => setFrame(-1)}
    >
      {sprite && frame >= 0 ? (
        <div className="mb-sprite" style={{ backgroundImage: `url("${convertFileSrc(sprite)}")`, backgroundPosition: `${(frame / 9) * 100}% 0` }} />
      ) : thumb ? (
        <img src={convertFileSrc(thumb)} alt="" loading="lazy" draggable={false} />
      ) : (
        <Icon name={item.media_type === "video" ? "film" : "image"} size={28} />
      )}
      {item.seq_count ? <span className="mb-badge">Chuỗi {item.seq_count} khung</span> : null}
      {item.duration && item.media_type === "video" ? <span className="mb-badge right">{formatDuration(item.duration)}</span> : null}
    </div>
  );
}

// ---------------------------------------------------------------- waveform

function Waveform({ peaks, progress, onSeek }: { peaks: number[] | null; progress: number; onSeek: (r: number) => void }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const c = ref.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    const w = c.clientWidth;
    const h = c.clientHeight;
    c.width = w * dpr;
    c.height = h * dpr;
    const ctx = c.getContext("2d")!;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, w, h);
    if (!peaks || peaks.length === 0) return;
    const styles = getComputedStyle(c);
    const played = styles.getPropertyValue("--accent").trim() || "#6d8cff";
    const rest = styles.getPropertyValue("--line-2").trim() || "#444";
    const bars = Math.max(1, Math.min(peaks.length, Math.floor(w / 3)));
    const step = peaks.length / bars;
    for (let i = 0; i < bars; i++) {
      let m = 0;
      for (let j = Math.floor(i * step); j < Math.floor((i + 1) * step); j++) m = Math.max(m, peaks[j] ?? 0);
      const bh = Math.max(2, m * (h - 4));
      ctx.fillStyle = i / bars <= progress ? played : rest;
      ctx.fillRect(i * 3, (h - bh) / 2, 2, bh);
    }
  }, [peaks, progress]);
  return (
    <canvas
      ref={ref}
      className={`waveform ${peaks ? "" : "placeholder"}`}
      onClick={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        onSeek((e.clientX - r.left) / r.width);
      }}
    />
  );
}

// ---------------------------------------------------------------- main

interface Props {
  view: MediaView;
  resource: { id: number; name: string } | null;
  onClearResource: () => void;
  collection: { id: number; name: string } | null;
  onClearCollection: () => void;
  libraries: Library[];
  reloadKey: number;
  presence: Presence | null;
  pendingMeta: number;
  aiReady: boolean;
  onChanged: () => void;
  onOpenResource: (id: number) => void;
  toast: (msg: string, kind?: "ok" | "err") => void;
}

export function MediaBrowser(props: Props) {
  const { presence } = props;
  if (presence?.plugin_active) {
    return (
      <div className="mb-locked">
        <Icon name="lock" size={34} />
        <h2>Đang được sử dụng trong DaVinci</h2>
        <p>
          HNH SFX Finder {presence.version ? `v${presence.version} ` : ""}đang mở trong DaVinci Resolve. Media Browser của MRM tạm tắt để tránh
          xung đột; các tác vụ nặng (đọc metadata, AI) cũng tạm dừng và VRAM đã được nhả cho Resolve.
        </p>
        <p className="muted small">Đóng plugin trong DaVinci — Media Browser sẽ tự mở lại sau vài giây.</p>
      </div>
    );
  }
  return <Browser {...props} />;
}

function Browser({ view, resource, onClearResource, collection, onClearCollection, libraries, reloadKey, pendingMeta, aiReady, onChanged, onOpenResource, toast }: Props) {
  const mediaType: MediaType | null = view === "favorites" || view === "recent" ? null : view;
  const [text, setText] = useState("");
  const [debounced, setDebounced] = useState("");
  const [ext, setExt] = useState<string>("");
  const [tagId, setTagId] = useState<number | null>(null);
  const [aiCat, setAiCat] = useState<string>("");
  const [aiStatus, setAiStatus] = useState<AssetAiStatus | null>(null);
  const [vision, setVision] = useState<VisionStatus | null>(null);
  const [visionHint, setVisionHint] = useState<boolean>(() => !loadPref("mb-vision-hint-hidden", false));
  /** file mở từ mục "Tương tự" — có thể không nằm trong danh sách hiện tại */
  const [outside, setOutside] = useState<AssetItem | null>(null);
  const [libraryId, setLibraryId] = useState<number | null>(null);
  const [sort, setSort] = useState<AssetSort>(view === "recent" ? "recent" : "name");
  const [desc, setDesc] = useState(view === "recent");
  const [layout, setLayout] = useState<"grid" | "list">(() => loadPref(`mb-layout-${view}`, view === "audio" || view === "recent" ? "list" : "grid"));
  const [autoPlay, setAutoPlay] = useState<boolean>(() => loadPref("mb-autoplay", true));
  const [items, setItems] = useState<AssetItem[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [facets, setFacets] = useState<AssetFacets | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [focusId, setFocusId] = useState<number | null>(null);
  const [anchor, setAnchor] = useState<number | null>(null);
  const [playing, setPlaying] = useState<number | null>(null);
  const [paused, setPaused] = useState(true);
  const [progress, setProgress] = useState(0);
  const audioRef = useRef<HTMLAudioElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const token = useRef(0);
  const dragIcon = useRef<string | null>(null);

  const libPath = useMemo(() => new Map(libraries.map((l) => [l.id, l.path])), [libraries]);
  const pathOf = useCallback((i: AssetItem) => joinPath(libPath.get(i.library_id) ?? "", i.rel_path), [libPath]);

  // đổi view -> đặt lại bộ lọc
  useEffect(() => {
    setExt("");
    setTagId(null);
    setAiCat("");
    setOutside(null);
    setSort(view === "recent" ? "recent" : "name");
    setDesc(view === "recent");
    setLayout(loadPref(`mb-layout-${view}`, view === "audio" || view === "recent" ? "list" : "grid"));
    setSelected(new Set());
    setFocusId(null);
  }, [view]);
  useEffect(() => savePref(`mb-layout-${view}`, layout), [layout, view]);
  useEffect(() => savePref("mb-autoplay", autoPlay), [autoPlay]);
  useEffect(() => {
    const t = setTimeout(() => setDebounced(text.trim()), aiReady ? 300 : 180);
    return () => clearTimeout(t);
  }, [text, aiReady]);
  // đang tìm + có AI -> xếp theo mức liên quan; bỏ tìm -> về sắp xếp thường
  useEffect(() => {
    if (debounced && aiReady) setSort((s) => (s === "name" ? "relevance" : s));
    else setSort((s) => (s === "relevance" ? (view === "recent" ? "recent" : "name") : s));
  }, [debounced, aiReady, view]);
  useEffect(() => {
    if (!aiReady) return setAiStatus(null);
    const load = () => {
      api.assetAiStatus().then(setAiStatus).catch(() => undefined);
      api.visionStatus().then(setVision).catch(() => undefined);
    };
    load();
    const t = setInterval(load, 15000);
    return () => clearInterval(t);
  }, [aiReady]);

  const query: AssetQuery = useMemo(
    () => ({
      media_type: mediaType,
      text: debounced,
      ext: ext || null,
      tag_id: tagId,
      ai_category: aiCat || null,
      semantic: aiReady,
      library_id: libraryId,
      resource_id: resource?.id ?? null,
      collection_id: collection?.id ?? null,
      favorites: view === "favorites",
      recent: view === "recent",
      sort,
      desc,
      limit: PAGE,
    }),
    [mediaType, debounced, ext, tagId, aiCat, aiReady, libraryId, resource, collection, view, sort, desc],
  );

  const load = useCallback(
    (offset: number) => {
      const my = ++token.current;
      setLoading(true);
      api
        .queryAssets({ ...query, offset })
        .then((page) => {
          if (my !== token.current) return;
          setTotal(page.total);
          setItems((prev) => (offset === 0 ? page.items : [...prev, ...page.items]));
        })
        .catch((err) => toast(errorText(err), "err"))
        .finally(() => my === token.current && setLoading(false));
    },
    [query, toast],
  );

  useEffect(() => {
    load(0);
    listRef.current?.scrollTo({ top: 0 });
  }, [load, reloadKey]);

  useEffect(() => {
    api.assetFacets(mediaType).then(setFacets).catch(() => setFacets(null));
  }, [mediaType, reloadKey]);

  // dừng nhạc khi rời trang
  useEffect(() => () => audioRef.current?.pause(), []);

  const focus = items.find((i) => i.id === focusId) ?? (outside && outside.id === focusId ? outside : null);

  const play = useCallback(
    async (item: AssetItem, at?: number) => {
      const a = audioRef.current;
      if (!a || item.media_type !== "audio" || !item.available) return;
      if (playing === item.id && at === undefined) {
        if (a.paused) a.play().catch(() => undefined);
        else a.pause();
        return;
      }
      let src: string | null = null;
      if (AUDIO_NATIVE.has(item.ext)) src = convertFileSrc(pathOf(item));
      else {
        const info = await api.assetPreview(item.id).catch(() => null);
        if (info?.playable && info.file) src = convertFileSrc(info.file);
      }
      if (!src) {
        toast(`Không phát trực tiếp được định dạng .${item.ext}`, "err");
        return;
      }
      if (playing !== item.id) {
        a.src = src;
        setPlaying(item.id);
        setProgress(0);
      }
      if (at !== undefined) {
        const seek = () => {
          if (a.duration) a.currentTime = at * a.duration;
        };
        if (a.readyState >= 1) seek();
        else a.addEventListener("loadedmetadata", seek, { once: true });
      }
      a.play().catch(() => undefined);
    },
    [playing, pathOf, toast],
  );

  const select = useCallback(
    (item: AssetItem, e?: { ctrlKey?: boolean; metaKey?: boolean; shiftKey?: boolean }) => {
      if (e?.shiftKey && anchor !== null) {
        const a = items.findIndex((i) => i.id === anchor);
        const b = items.findIndex((i) => i.id === item.id);
        const [lo, hi] = a < b ? [a, b] : [b, a];
        setSelected(new Set(items.slice(lo, hi + 1).map((i) => i.id)));
      } else if (e?.ctrlKey || e?.metaKey) {
        setSelected((s) => {
          const n = new Set(s);
          if (n.has(item.id)) n.delete(item.id);
          else n.add(item.id);
          return n;
        });
        setAnchor(item.id);
      } else {
        setSelected(new Set([item.id]));
        setAnchor(item.id);
        if (autoPlay && item.media_type === "audio" && playing !== item.id) play(item);
      }
      setFocusId(item.id);
    },
    [anchor, items, autoPlay, play, playing],
  );

  const drag = useCallback(
    async (item: AssetItem) => {
      const ids = selected.has(item.id) && selected.size > 1 ? [...selected] : [item.id];
      try {
        const paths = await api.useAssets(ids);
        if (paths.length === 0) return toast("File hiện không truy cập được", "err");
        const thumb = item.media_type !== "audio" ? thumbCache.get(item.id) : null;
        if (!dragIcon.current) dragIcon.current = await api.dragIcon();
        // mode "copy": app đích chỉ sao chép / tham chiếu — không bao giờ di chuyển file gốc
        await startDrag({ item: paths, icon: thumb || dragIcon.current, mode: "copy" });
      } catch (err) {
        toast(errorText(err), "err");
      }
    },
    [selected, toast],
  );

  const onKey = (e: React.KeyboardEvent) => {
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    const idx = items.findIndex((i) => i.id === focusId);
    const cols = layout === "grid" && listRef.current ? Math.max(1, Math.floor(listRef.current.clientWidth / 190)) : 1;
    let next = -1;
    if (e.key === "ArrowDown") next = Math.min(items.length - 1, idx + cols);
    else if (e.key === "ArrowUp") next = Math.max(0, idx - cols);
    else if (e.key === "ArrowRight" && layout === "grid") next = Math.min(items.length - 1, idx + 1);
    else if (e.key === "ArrowLeft" && layout === "grid") next = Math.max(0, idx - 1);
    else if (e.key === " " && focus) {
      e.preventDefault();
      play(focus);
      return;
    } else return;
    e.preventDefault();
    if (next >= 0 && items[next]) {
      select(items[next], e);
      document.getElementById(`mb-${items[next].id}`)?.scrollIntoView({ block: "nearest" });
    }
  };

  const refreshItem = useCallback((id: number, patch: Partial<AssetItem>) => {
    setItems((list) => list.map((i) => (i.id === id ? { ...i, ...patch } : i)));
    setOutside((o) => (o && o.id === id ? { ...o, ...patch } : o));
  }, []);

  const pick = useCallback(
    (item: AssetItem) => {
      if (!items.some((i) => i.id === item.id)) setOutside(item);
      setSelected(new Set([item.id]));
      setAnchor(item.id);
      setFocusId(item.id);
      if (autoPlay && item.media_type === "audio") play(item);
    },
    [items, autoPlay, play],
  );

  const tagCategory = async () => {
    if (!aiCat) return;
    if (!confirm(`Gắn tag “${aiCat}” cho toàn bộ ${formatNumber(total)} file đang lọc?`)) return;
    try {
      const n = await api.tagAssetsByQuery({ ...query, offset: 0 }, aiCat);
      toast(`Đã gắn tag “${aiCat}” cho ${formatNumber(n)} file`);
      onChanged();
      api.assetFacets(mediaType).then(setFacets).catch(() => undefined);
    } catch (err) {
      toast(errorText(err), "err");
    }
  };

  const toggleFav = async (ids: number[], on: boolean) => {
    try {
      await api.setAssetFavorite(ids, on);
      setItems((list) => list.map((i) => (ids.includes(i.id) ? { ...i, favorite: on } : i)));
      onChanged();
    } catch (err) {
      toast(errorText(err), "err");
    }
  };

  const title = MEDIA_VIEW_LABEL[view];
  const hasFilter = !!(debounced || ext || tagId || aiCat || libraryId || resource || collection);
  const aiIndexing = aiStatus && aiStatus.indexed < aiStatus.total;

  return (
    <div className="mb">
      <header className="toolbar">
        <div className="toolbar-title">
          <h1>{title}</h1>
          {collection && (
            <span className="chip coll-chip mb-res-chip" title="Chỉ hiện file trong collection này">
              <Icon name="layers" size={11} /> {collection.name}
              <button className="chip-x" onClick={onClearCollection} aria-label="Bỏ lọc collection">
                <Icon name="x" size={10} />
              </button>
            </span>
          )}
          {resource && (
            <span className="chip k-type mb-res-chip" title="Chỉ hiện file trong resource này">
              <Icon name="layers" size={11} /> {resource.name}
              <button className="chip-x" onClick={onClearResource} aria-label="Bỏ lọc resource">
                <Icon name="x" size={10} />
              </button>
            </span>
          )}
          <span className="muted small">
            {formatNumber(total)} file{pendingMeta > 0 ? ` · đang đọc metadata nền (${formatNumber(pendingMeta)})` : ""}
            {aiIndexing ? ` · AI đang học ${formatNumber(aiStatus.indexed)}/${formatNumber(aiStatus.total)}` : ""}
          </span>
        </div>
        <div className="search">
          <Icon name="search" size={15} />
          <input
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={aiReady ? "Tìm theo ý nghĩa… (vd: tiếng súng, vỗ tay, chuyển cảnh nhanh, hạt bụi phim)" : "Tìm tên file, thư mục, tag, resource… (vd: whoosh fast)"}
            spellCheck={false}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setText("");
                (e.target as HTMLInputElement).blur();
              }
            }}
          />
          {aiReady && (
            <span className="ai-chip" title="Tìm theo ý nghĩa (AI offline) kết hợp khớp từ khóa">
              <Icon name="sparkles" size={11} /> AI
            </span>
          )}
          {text && (
            <button className="icon-btn" onClick={() => setText("")} aria-label="Xóa tìm kiếm">
              <Icon name="x" size={13} />
            </button>
          )}
        </div>
        <div className="toolbar-actions">
          <select value={ext} onChange={(e) => setExt(e.target.value)} aria-label="Định dạng">
            <option value="">Mọi định dạng</option>
            {facets?.exts.map((f) => (
              <option key={f.key} value={f.key}>
                .{f.key} ({formatNumber(f.count)})
              </option>
            ))}
          </select>
          {facets && facets.ai.length > 0 && (
            <select value={aiCat} onChange={(e) => setAiCat(e.target.value)} aria-label="Danh mục AI" title="Danh mục do AI tự nhận diện từ tên file và thư mục">
              <option value="">✦ Danh mục AI</option>
              {facets.ai.map((f) => (
                <option key={f.key} value={f.key}>
                  {f.key} ({formatNumber(f.count)})
                </option>
              ))}
            </select>
          )}
          {aiCat && total > 0 && (
            <button className="btn" onClick={tagCategory} title="Biến danh mục AI thành tag thật (dùng chung với plugin DaVinci)">
              <Icon name="tag" size={14} /> Gắn tag “{aiCat}”
            </button>
          )}
          {facets && facets.tags.length > 0 && (
            <select value={tagId ?? ""} onChange={(e) => setTagId(e.target.value ? Number(e.target.value) : null)} aria-label="Tag">
              <option value="">Mọi tag</option>
              {facets.tags.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.key} ({f.count})
                </option>
              ))}
            </select>
          )}
          {libraries.length > 1 && (
            <select value={libraryId ?? ""} onChange={(e) => setLibraryId(e.target.value ? Number(e.target.value) : null)} aria-label="Library">
              <option value="">Mọi library</option>
              {libraries.map((l) => (
                <option key={l.id} value={l.id}>
                  {l.name}
                </option>
              ))}
            </select>
          )}
          <select value={sort} onChange={(e) => setSort(e.target.value as AssetSort)} aria-label="Sắp xếp">
            {debounced && <option value="relevance">Liên quan</option>}
            <option value="name">Tên</option>
            <option value="added">Ngày thêm</option>
            <option value="size">Dung lượng</option>
            <option value="duration">Thời lượng</option>
            <option value="recent">Dùng gần đây</option>
          </select>
          <button className="icon-btn" onClick={() => setDesc((d) => !d)} title={desc ? "Giảm dần" : "Tăng dần"}>
            <Icon name={desc ? "sortDesc" : "sortAsc"} size={16} />
          </button>
          <div className="segmented">
            <button className={layout === "list" ? "on" : ""} onClick={() => setLayout("list")} aria-label="Danh sách">
              <Icon name="list" size={15} />
            </button>
            <button className={layout === "grid" ? "on" : ""} onClick={() => setLayout("grid")} aria-label="Lưới">
              <Icon name="grid" size={15} />
            </button>
          </div>
          {(mediaType === "audio" || mediaType === null) && (
            <label className="mb-toggle" title="Tự phát âm thanh khi chọn">
              <input type="checkbox" checked={autoPlay} onChange={(e) => setAutoPlay(e.target.checked)} /> Tự nghe
            </label>
          )}
        </div>
      </header>

      {visionHint && (mediaType === "image" || mediaType === "video") && !(vision?.enabled) && (
        <div className="mb-hint-bar">
          <Icon name="sparkles" size={14} />
          <span>
            Muốn tìm theo <b>nội dung hình</b> (vd: <i>cô gái tóc dài</i>, <i>nền trời xanh</i>)? Bật <b>AI nhìn ảnh/video</b> trong Libraries & Settings → AI
            {aiReady ? (vision?.installed ? "." : " (tải thêm model ~3.3 GB).") : " — cần bật AI offline trước."}
          </span>
          <button
            className="icon-btn"
            onClick={() => {
              setVisionHint(false);
              savePref("mb-vision-hint-hidden", true);
            }}
            aria-label="Ẩn nhắc nhở"
            title="Không nhắc lại"
          >
            <Icon name="x" size={13} />
          </button>
        </div>
      )}
      {vision?.enabled && vision.done < vision.total && (mediaType === "image" || mediaType === "video") && (
        <div className="mb-hint-bar subtle">
          <span className="spinner" />
          <span>
            AI đang xem ảnh/video: {formatNumber(vision.done)}/{formatNumber(vision.total)}
            {vision.paused ? " · đang tạm dừng" : ""}
          </span>
        </div>
      )}

      <div className="mb-body">
        <div
          className={`mb-results mb-${layout}`}
          ref={listRef}
          tabIndex={0}
          onKeyDown={onKey}
          onScroll={(e) => {
            const el = e.currentTarget;
            if (!loading && items.length < total && el.scrollTop + el.clientHeight > el.scrollHeight - 600) load(items.length);
          }}
        >
          {items.length === 0 && !loading && (
            <div className="empty-state">
              <Icon name={hasFilter ? "search" : mediaType === "audio" ? "music" : mediaType === "video" ? "film" : mediaType === "image" ? "image" : "star"} size={28} />
              <p>
                {hasFilter
                  ? "Không có file nào khớp bộ lọc."
                  : view === "favorites"
                    ? "Chưa có media yêu thích — bấm ★ trên file để thêm."
                    : view === "recent"
                      ? "Chưa dùng file nào — kéo file sang app dựng hoặc sao chép đường dẫn sẽ được ghi lại ở đây."
                      : "Chưa có file nào. Thêm library và quét trong Libraries & Settings."}
              </p>
            </div>
          )}
          {layout === "list"
            ? items.map((it) => (
                <div
                  key={it.id}
                  id={`mb-${it.id}`}
                  className={`mb-row ${selected.has(it.id) ? "selected" : ""} ${it.available ? "" : "missing"}`}
                  onClick={(e) => select(it, e)}
                  onDoubleClick={() => it.media_type === "audio" && play(it)}
                  draggable={it.available}
                  onDragStart={(e) => {
                    e.preventDefault();
                    drag(it);
                  }}
                >
                  {it.media_type === "audio" ? (
                    <button
                      className={`mb-play ${playing === it.id && !paused ? "on" : ""}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        setFocusId(it.id);
                        play(it);
                      }}
                      aria-label="Nghe thử"
                    >
                      <Icon name={playing === it.id && !paused ? "pause" : "play"} size={12} />
                    </button>
                  ) : (
                    <div className="mb-row-thumb">
                      <Thumb item={it} />
                    </div>
                  )}
                  <div className="mb-row-main">
                    <div className="truncate mb-name">{it.filename}</div>
                    <div className="truncate muted small" title={it.rel_path}>
                      {contextOf(it)}
                    </div>
                  </div>
                  <span className="mb-col muted small">{formatDuration(it.duration)}</span>
                  <span className="mb-col ext">{it.ext}</span>
                  <span className="mb-col muted small">{formatSize(it.size)}</span>
                  <button
                    className={`fav-btn ${it.favorite ? "on" : ""}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleFav([it.id], !it.favorite);
                    }}
                    aria-label="Yêu thích"
                  >
                    <Icon name="star" size={14} />
                  </button>
                </div>
              ))
            : items.map((it) => (
                <div
                  key={it.id}
                  id={`mb-${it.id}`}
                  className={`mb-card ${selected.has(it.id) ? "selected" : ""} ${it.available ? "" : "missing"}`}
                  onClick={(e) => select(it, e)}
                  onDoubleClick={() => it.media_type === "audio" && play(it)}
                  draggable={it.available}
                  onDragStart={(e) => {
                    e.preventDefault();
                    drag(it);
                  }}
                  title={it.rel_path}
                >
                  {it.media_type === "audio" ? (
                    <div className="mb-thumb audio" onClick={(e) => (e.stopPropagation(), setFocusId(it.id), play(it))}>
                      <Icon name={playing === it.id && !paused ? "pause" : "music"} size={26} />
                      {it.duration ? <span className="mb-badge right">{formatDuration(it.duration)}</span> : null}
                    </div>
                  ) : (
                    <Thumb item={it} />
                  )}
                  <div className="mb-card-foot">
                    <span className="truncate">{it.filename}</span>
                    <button
                      className={`fav-btn ${it.favorite ? "on" : ""}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        toggleFav([it.id], !it.favorite);
                      }}
                      aria-label="Yêu thích"
                    >
                      <Icon name="star" size={13} />
                    </button>
                  </div>
                </div>
              ))}
          {loading && (
            <div className="mb-loading muted small">
              <span className="spinner" /> Đang tải…
            </div>
          )}
        </div>

        <aside className="mb-inspector">
          {selected.size > 1 ? (
            <BulkPanel
              ids={[...selected]}
              items={items.filter((i) => selected.has(i.id))}
              onFav={toggleFav}
              onDrag={() => {
                const first = items.find((i) => selected.has(i.id));
                if (first) drag(first);
              }}
              onChanged={() => {
                onChanged();
                load(0);
              }}
              toast={toast}
            />
          ) : focus ? (
            <AssetInspector
              key={focus.id}
              item={focus}
              reloadKey={reloadKey}
              playing={playing === focus.id}
              paused={paused}
              progress={playing === focus.id ? progress : 0}
              onPlay={(at) => play(focus, at)}
              onDrag={() => drag(focus)}
              onFav={(on) => toggleFav([focus.id], on)}
              onPatch={(p) => refreshItem(focus.id, p)}
              onTagsChanged={() => {
                onChanged();
                api.assetFacets(mediaType).then(setFacets).catch(() => undefined);
              }}
              onOpenResource={onOpenResource}
              onPick={pick}
              aiReady={aiReady}
              toast={toast}
            />
          ) : (
            <div className="empty-state">
              <Icon name="eye" size={24} />
              <p className="muted small">
                Chọn một file để xem trước.
                <br />
                Kéo file (hoặc nhiều file) thả thẳng vào Premiere, CapCut, After Effects…
                <br />
                Phím ↑ ↓ để duyệt, Space để nghe.
              </p>
            </div>
          )}
        </aside>
      </div>

      <audio
        ref={audioRef}
        onPlay={() => setPaused(false)}
        onPause={() => setPaused(true)}
        onEnded={() => setPaused(true)}
        onTimeUpdate={(e) => {
          const a = e.currentTarget;
          setProgress(a.duration ? a.currentTime / a.duration : 0);
        }}
      />
    </div>
  );
}

// ---------------------------------------------------------------- inspector

interface InspectorProps {
  item: AssetItem;
  reloadKey: number;
  playing: boolean;
  paused: boolean;
  progress: number;
  onPlay: (at?: number) => void;
  onDrag: () => void;
  onFav: (on: boolean) => void;
  onPatch: (p: Partial<AssetItem>) => void;
  onTagsChanged: () => void;
  onOpenResource: (id: number) => void;
  onPick: (item: AssetItem) => void;
  aiReady: boolean;
  toast: (msg: string, kind?: "ok" | "err") => void;
}

function AssetInspector({
  item,
  reloadKey,
  playing,
  paused,
  progress,
  onPlay,
  onDrag,
  onFav,
  onPatch,
  onTagsChanged,
  onOpenResource,
  onPick,
  aiReady,
  toast,
}: InspectorProps) {
  const [similar, setSimilar] = useState<AssetItem[] | null>(null);
  useEffect(() => {
    let alive = true;
    setSimilar(null);
    if (aiReady) api.similarAssets(item.id).then((s) => alive && setSimilar(s)).catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [item.id, aiReady]);
  const [d, setD] = useState<AssetDetail | null>(null);
  const [peaks, setPeaks] = useState<number[] | null>(null);
  const [video, setVideo] = useState<{ src: string | null; state: "loading" | "ready" | "proxy" | "failed" }>({ src: null, state: "loading" });
  const [big, setBig] = useState<string | null>(null);

  const reload = useCallback(() => {
    api
      .getAsset(item.id)
      .then((x) => {
        setD(x);
        onPatch({ duration: x.duration, width: x.width, height: x.height, favorite: x.favorite });
      })
      .catch((err) => toast(errorText(err), "err"));
  }, [item.id]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(reload, [reload, reloadKey]);

  useEffect(() => {
    let alive = true;
    if (!item.available) return;
    if (item.media_type === "audio") api.assetWaveform(item.id).then((p) => alive && setPeaks(p)).catch(() => alive && setPeaks([]));
    if (item.media_type === "image" && !IMAGE_NATIVE.has(item.ext)) api.assetThumbnail(item.id, 1600).then((t) => alive && setBig(t)).catch(() => undefined);
    if (item.media_type === "video") {
      api.assetThumbnail(item.id, 960).then((t) => alive && setBig(t)).catch(() => undefined);
      api
        .assetPreview(item.id)
        .then(async (info) => {
          if (!alive) return;
          if (info.playable && info.file) return setVideo({ src: info.file, state: "ready" });
          setVideo({ src: null, state: "proxy" });
          const p = await api.assetProxy(item.id);
          if (alive) setVideo({ src: p, state: "ready" });
        })
        .catch(() => alive && setVideo({ src: null, state: "failed" }));
    }
    return () => {
      alive = false;
    };
  }, [item.id, item.available, item.media_type, item.ext]);

  const copyPath = async () => {
    if (!d) return;
    const paths = await api.useAssets([d.id]);
    await navigator.clipboard.writeText(paths.join("\r\n") || d.path);
    toast("Đã sao chép đường dẫn");
  };

  const meta: [string, string][] = d
    ? ([
        ["Định dạng", `.${d.ext}${d.codec ? ` · ${d.codec}` : ""}`],
        ["Thời lượng", formatDuration(d.duration)],
        ["Độ phân giải", d.width ? `${d.width}×${d.height}` : ""],
        ["FPS", d.fps ? `${Math.round(d.fps * 100) / 100}` : ""],
        ["Âm thanh", d.sample_rate ? `${(d.sample_rate / 1000).toFixed(1)} kHz${d.channels ? ` · ${d.channels === 1 ? "mono" : d.channels === 2 ? "stereo" : `${d.channels} kênh`}` : ""}` : ""],
        ["Chuỗi ảnh", d.seq_count ? `${d.seq_count} khung (${d.seq_start}–${d.seq_end}) · ${d.seq_pattern}` : ""],
        ["Dung lượng", formatSize(d.size)],
        ["Library", d.library_name],
        ["Ngày thêm", formatDateTime(d.added_at)],
        ["Đã dùng", d.use_count ? `${d.use_count} lần · ${formatDateTime(d.last_used_at)}` : ""],
      ].filter(([, v]) => v) as [string, string][])
    : [];

  return (
    <div className="mb-insp">
      <div className="mb-preview">
        {!item.available ? (
          <div className="preview-msg">
            <Icon name="alert" size={18} /> File hiện không truy cập được (library offline hoặc đã bị xóa/di chuyển).
          </div>
        ) : item.media_type === "image" ? (
          IMAGE_NATIVE.has(item.ext) && d ? (
            <img src={convertFileSrc(d.path)} alt={item.filename} />
          ) : big ? (
            <img src={convertFileSrc(big)} alt={item.filename} />
          ) : (
            <span className="spinner" />
          )
        ) : item.media_type === "video" ? (
          video.state === "ready" && video.src ? (
            <video key={video.src} src={convertFileSrc(video.src)} poster={big ? convertFileSrc(big) : undefined} controls />
          ) : (
            <div className="mb-video-wait">
              {big && <img src={convertFileSrc(big)} alt="" className="dim" />}
              <span className="preview-msg overlay">
                {video.state === "failed" ? (
                  "Không tạo được bản xem thử"
                ) : video.state === "proxy" ? (
                  <>
                    <span className="spinner" /> Đang tạo bản xem thử (GPU)…
                  </>
                ) : (
                  <span className="spinner" />
                )}
              </span>
            </div>
          )
        ) : (
          <div className="mb-audio">
            <button className={`mb-play big ${playing && !paused ? "on" : ""}`} onClick={() => onPlay()} aria-label="Nghe thử">
              <Icon name={playing && !paused ? "pause" : "play"} size={18} />
            </button>
            <Waveform peaks={peaks} progress={progress} onSeek={(r) => onPlay(r)} />
          </div>
        )}
      </div>

      <div className="insp-block">
        <div className="mb-insp-title">
          <h2 className="truncate" title={item.filename}>
            {item.filename}
          </h2>
          <button className={`fav-btn ${item.favorite ? "on" : ""}`} onClick={() => onFav(!item.favorite)} aria-label="Yêu thích">
            <Icon name="star" size={16} />
          </button>
        </div>
        <div className="muted small mb-path" title={d?.path}>
          {folderOf(item.rel_path) || "(thư mục gốc library)"}
        </div>
        <div className="mb-actions">
          <button
            className="btn primary mb-drag"
            draggable={item.available}
            onDragStart={(e) => {
              e.preventDefault();
              onDrag();
            }}
            title="Kéo nút này thả vào timeline / bin của app dựng (chỉ sao chép, file gốc giữ nguyên)"
          >
            <Icon name="drag" size={14} /> Kéo vào app dựng
          </button>
          <button className="btn" onClick={() => api.revealAsset(item.id).catch((err) => toast(errorText(err), "err"))} disabled={!item.available}>
            <Icon name="folder" size={14} /> Mở vị trí
          </button>
          <button className="btn" onClick={() => copyPath().catch((err) => toast(errorText(err), "err"))}>
            <Icon name="copy" size={14} /> Sao chép đường dẫn
          </button>
          {item.media_type !== "audio" && item.resource_id != null && item.available && (
            <button
              className="btn"
              title="Dùng ảnh/khung hình này làm ảnh bìa cho resource chứa nó"
              onClick={() =>
                api
                  .setCoverFromAsset(item.id)
                  .then(() => {
                    invalidateCovers([item.resource_id!]);
                    toast(`Đã đặt làm ảnh bìa của “${item.resource_name ?? "resource"}”`);
                  })
                  .catch((err) => toast(errorText(err), "err"))
              }
            >
              <Icon name="image" size={14} /> Đặt làm ảnh bìa
            </button>
          )}
        </div>
      </div>

      {d?.resource_id && (
        <div className="insp-block">
          <div className="insp-label">Thuộc resource</div>
          <button className="mb-resource" onClick={() => onOpenResource(d.resource_id!)}>
            <Icon name="layers" size={14} />
            <span className="truncate">{d.resource_name}</span>
            <Icon name="link" size={12} />
          </button>
        </div>
      )}

      {d?.ai_caption && (
        <div className="insp-block">
          <div className="insp-label">
            <span>
              <Icon name="eye" size={11} /> AI mô tả
            </span>
          </div>
          <div className="mb-caption">{d.ai_caption}</div>
        </div>
      )}

      {d?.ai_category && (
        <div className="insp-block">
          <div className="insp-label">Danh mục AI</div>
          <div className="mb-ai-cat">
            <span className="ai-chip">
              <Icon name="sparkles" size={11} /> {d.ai_category}
            </span>
            <span className="muted small">độ tin cậy {Math.round((d.ai_score ?? 0) * 100)}%</span>
            {!d.tags.some((t) => t.name.toLowerCase() === d.ai_category!.toLowerCase()) && (
              <button
                className="btn tiny"
                onClick={() =>
                  api
                    .setAssetTag([d.id], d.ai_category!, true)
                    .then(() => (reload(), onTagsChanged()))
                    .catch((err) => toast(errorText(err), "err"))
                }
              >
                <Icon name="plus" size={11} /> Gắn thành tag
              </button>
            )}
          </div>
        </div>
      )}

      {d && <TagEditor ids={[d.id]} tags={d.tags.map((t) => t.name)} onChanged={() => (reload(), onTagsChanged())} toast={toast} />}

      {d && <CollectionPicker kind="asset" ids={[d.id]} onChanged={onTagsChanged} toast={toast} />}

      {similar && similar.length > 0 && (
        <div className="insp-block">
          <div className="insp-label">
            <span>
              <Icon name="sparkles" size={11} /> Tương tự
            </span>
          </div>
          <div className="mb-similar">
            {similar.map((s) => (
              <button key={s.id} className="mb-similar-row" onClick={() => onPick(s)} title={s.rel_path}>
                <Icon name={s.media_type === "audio" ? "music" : s.media_type === "video" ? "film" : "image"} size={12} />
                <span className="truncate">{s.filename}</span>
                <span className="muted small">{formatDuration(s.duration)}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {d && (
        <div className="insp-block">
          <div className="insp-label">Thông tin</div>
          <dl className="mb-meta">
            {meta.map(([k, v]) => (
              <div key={k}>
                <dt>{k}</dt>
                <dd>{v}</dd>
              </div>
            ))}
          </dl>
          {d.meta_state === 2 && <div className="muted small">Không đọc được metadata của file này.</div>}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- tags

let tagNamesCache: string[] | null = null;

function TagEditor({ ids, tags, onChanged, toast }: { ids: number[]; tags: string[]; onChanged: () => void; toast: (m: string, k?: "ok" | "err") => void }) {
  const [input, setInput] = useState("");
  const [names, setNames] = useState<string[]>(tagNamesCache ?? []);
  useEffect(() => {
    api
      .listAssetTagNames()
      .then((n) => {
        tagNamesCache = n;
        setNames(n);
      })
      .catch(() => undefined);
  }, []);
  const change = async (name: string, on: boolean) => {
    try {
      await api.setAssetTag(ids, name, on);
      tagNamesCache = null;
      onChanged();
    } catch (err) {
      toast(errorText(err), "err");
    }
  };
  return (
    <div className="insp-block">
      <div className="insp-label">Tag{ids.length > 1 ? ` (áp dụng cho ${ids.length} file)` : ""}</div>
      <div className="chips">
        {tags.map((t) => (
          <span key={t} className="chip k-function">
            {t}
            <button className="chip-x" onClick={() => change(t, false)} aria-label={`Gỡ tag ${t}`}>
              <Icon name="x" size={10} />
            </button>
          </span>
        ))}
        <input
          className="mb-tag-input"
          list="mb-tag-names"
          value={input}
          placeholder="+ thêm tag"
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && input.trim()) {
              change(input.trim(), true);
              setInput("");
            }
          }}
        />
        <datalist id="mb-tag-names">
          {names.filter((n) => !tags.includes(n)).map((n) => (
            <option key={n} value={n} />
          ))}
        </datalist>
      </div>
      <div className="muted small">Tag dùng chung với plugin HNH SFX Finder trong DaVinci.</div>
    </div>
  );
}

function BulkPanel({
  ids,
  items,
  onFav,
  onDrag,
  onChanged,
  toast,
}: {
  ids: number[];
  items: AssetItem[];
  onFav: (ids: number[], on: boolean) => void;
  onDrag: () => void;
  onChanged: () => void;
  toast: (m: string, k?: "ok" | "err") => void;
}) {
  const size = items.reduce((s, i) => s + i.size, 0);
  const allFav = items.length > 0 && items.every((i) => i.favorite);
  return (
    <div className="mb-insp">
      <div className="insp-block">
        <h2>{ids.length} file đang chọn</h2>
        <div className="muted small">{formatSize(size)}</div>
        <div className="mb-actions">
          <button
            className="btn primary mb-drag"
            draggable
            onDragStart={(e) => {
              e.preventDefault();
              onDrag();
            }}
            title="Kéo nút này thả vào app dựng (chỉ sao chép, file gốc giữ nguyên)"
          >
            <Icon name="drag" size={14} /> Kéo {ids.length} file vào app dựng
          </button>
          <button className="btn" onClick={() => onFav(ids, !allFav)}>
            <Icon name="star" size={14} /> {allFav ? "Bỏ yêu thích" : "Yêu thích tất cả"}
          </button>
          <button
            className="btn"
            onClick={async () => {
              const paths = await api.useAssets(ids);
              await navigator.clipboard.writeText(paths.join("\r\n"));
              toast(`Đã sao chép ${paths.length} đường dẫn`);
            }}
          >
            <Icon name="copy" size={14} /> Sao chép đường dẫn
          </button>
        </div>
      </div>
      <TagEditor ids={ids} tags={[]} onChanged={onChanged} toast={toast} />
      <CollectionPicker kind="asset" ids={ids} onChanged={onChanged} toast={toast} />
      <div className="muted small mb-hint">Ctrl + click để chọn thêm / bỏ, Shift + click để chọn một dải.</div>
    </div>
  );
}
