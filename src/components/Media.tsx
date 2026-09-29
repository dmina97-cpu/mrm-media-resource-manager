import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { api, CoverInfo, FileEntry, PreviewInfo, SourceInfo } from "../api";
import { errorText, formatSize } from "../format";
import { Icon } from "./Icon";

// ---------------------------------------------------------------- cover cache

const coverCache = new Map<number, CoverInfo | null>();
const coverPending = new Map<number, Promise<CoverInfo | null>>();
let coverVersion = 0;
const coverListeners = new Set<() => void>();

/** Xóa cache ảnh bìa (sau khi quét lại / đổi ảnh bìa). */
export function invalidateCovers(ids?: number[]) {
  if (ids) ids.forEach((id) => coverCache.delete(id));
  else coverCache.clear();
  coverVersion++;
  coverListeners.forEach((f) => f());
}

function loadCover(id: number): Promise<CoverInfo | null> {
  if (coverCache.has(id)) return Promise.resolve(coverCache.get(id) ?? null);
  let p = coverPending.get(id);
  if (!p) {
    p = api
      .getCover(id)
      .catch(() => null)
      .then((c) => {
        coverCache.set(id, c);
        coverPending.delete(id);
        return c;
      });
    coverPending.set(id, p);
  }
  return p;
}

export function useCover(id: number | null, delay = 120) {
  const [cover, setCover] = useState<CoverInfo | null | undefined>(() => (id !== null && coverCache.has(id) ? coverCache.get(id) : undefined));
  const [version, setVersion] = useState(coverVersion);
  useEffect(() => {
    const f = () => setVersion(coverVersion);
    coverListeners.add(f);
    return () => {
      coverListeners.delete(f);
    };
  }, []);
  useEffect(() => {
    if (id === null) return;
    if (coverCache.has(id)) {
      setCover(coverCache.get(id));
      return;
    }
    setCover(undefined);
    let alive = true;
    // chờ một chút để khi cuộn nhanh không gọi hàng loạt
    const t = setTimeout(() => loadCover(id).then((c) => alive && setCover(c)), delay);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [id, version, delay]);
  return cover;
}

// ---------------------------------------------------------------- cover view (card)

export function CoverThumb({ resourceId, fallback }: { resourceId: number; fallback: React.ReactNode }) {
  const cover = useCover(resourceId);
  const [sprite, setSprite] = useState<string | null>(null);
  const [frame, setFrame] = useState<number | null>(null);
  const hoverTimer = useRef<number | undefined>(undefined);

  if (!cover) return <div className="cover empty">{cover === undefined ? <span className="cover-loading" /> : fallback}</div>;

  const isVideo = cover.kind === "video";
  const onEnter = () => {
    if (!isVideo || sprite) return;
    hoverTimer.current = window.setTimeout(() => {
      api.getVideoSprite(cover.source_id, cover.path).then(setSprite).catch(() => undefined);
    }, 250);
  };
  const onMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!sprite) return;
    const rect = e.currentTarget.getBoundingClientRect();
    setFrame(Math.min(9, Math.max(0, Math.floor(((e.clientX - rect.left) / rect.width) * 10))));
  };
  const onLeave = () => {
    window.clearTimeout(hoverTimer.current);
    setFrame(null);
  };

  return (
    <div className="cover" onMouseEnter={onEnter} onMouseMove={onMove} onMouseLeave={onLeave}>
      {sprite && frame !== null ? (
        <div
          className="cover-sprite"
          style={{ backgroundImage: `url("${convertFileSrc(sprite)}")`, backgroundPosition: `${(frame / 9) * 100}% 0` }}
        />
      ) : (
        <img src={convertFileSrc(cover.thumb)} alt="" loading="lazy" draggable={false} />
      )}
      {isVideo && (
        <span className="cover-badge">
          <Icon name="play" size={10} filled />
        </span>
      )}
      {sprite && frame !== null && (
        <span className="cover-scrub">
          <span style={{ width: `${((frame + 1) / 10) * 100}%` }} />
        </span>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- file browser

const KIND_ICON: Record<string, string> = {
  dir: "folder",
  image: "image",
  video: "film",
  audio: "music",
  text: "file",
  pdf: "file",
  archive: "archive",
  other: "file",
};

export function FileBrowser({
  sources,
  onOpen,
}: {
  sources: SourceInfo[];
  onOpen: (sourceId: number, entry: FileEntry, siblings: FileEntry[]) => void;
}) {
  const browsable = sources.filter((s) => s.available && s.kind !== "file");
  const [sourceId, setSourceId] = useState<number | null>(browsable[0]?.id ?? null);
  const [dir, setDir] = useState("");
  const [entries, setEntries] = useState<FileEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setSourceId(browsable[0]?.id ?? null);
    setDir("");
  }, [sources.map((s) => s.id).join(",")]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (sourceId === null) return;
    setEntries(null);
    setError(null);
    api
      .listSourceFiles(sourceId, dir)
      .then(setEntries)
      .catch((err) => setError(errorText(err)));
  }, [sourceId, dir]);

  if (browsable.length === 0) return null;
  const crumbs = dir ? dir.split("/") : [];

  return (
    <div className="files">
      {browsable.length > 1 && (
        <div className="segmented small">
          {browsable.map((s) => (
            <button
              key={s.id}
              className={s.id === sourceId ? "on" : ""}
              onClick={() => {
                setSourceId(s.id);
                setDir("");
              }}
            >
              <Icon name={s.kind === "folder" ? "folder" : "archive"} size={12} /> {s.kind === "folder" ? "Folder" : "ZIP"}
            </button>
          ))}
        </div>
      )}
      <div className="crumbs">
        <button onClick={() => setDir("")}>root</button>
        {crumbs.map((c, i) => (
          <span key={i}>
            <span className="muted">/</span>
            <button onClick={() => setDir(crumbs.slice(0, i + 1).join("/"))}>{c}</button>
          </span>
        ))}
      </div>
      <div className="file-list">
        {error && <div className="muted small">{error}</div>}
        {entries === null && !error && <div className="muted small">Đang đọc…</div>}
        {entries?.length === 0 && <div className="muted small">Thư mục trống</div>}
        {entries?.slice(0, 500).map((en) => (
          <button
            key={en.path}
            className="file-row"
            onClick={() => (en.is_dir ? setDir(en.path) : sourceId !== null && onOpen(sourceId, en, entries.filter((x) => !x.is_dir)))}
            title={en.path}
          >
            <Icon name={KIND_ICON[en.kind] ?? "file"} size={13} className={`fk-${en.kind}`} />
            <span className="truncate">{en.name}</span>
            {!en.is_dir && <span className="muted small">{formatSize(en.size)}</span>}
          </button>
        ))}
        {entries && entries.length > 500 && <div className="muted small">… và {entries.length - 500} mục khác</div>}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- waveform

function Waveform({ peaks, progress, onSeek }: { peaks: number[]; progress: number; onSeek: (ratio: number) => void }) {
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
    const styles = getComputedStyle(c);
    const played = styles.getPropertyValue("--accent").trim() || "#6d8cff";
    const rest = styles.getPropertyValue("--line-2").trim() || "#444";
    const bars = Math.min(peaks.length, Math.floor(w / 3));
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
      className="waveform"
      onClick={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        onSeek((e.clientX - r.left) / r.width);
      }}
    />
  );
}

function formatDuration(s: number) {
  if (!s) return "";
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return `${m}:${sec.toString().padStart(2, "0")}`;
}

// ---------------------------------------------------------------- preview modal

export interface PreviewTarget {
  sourceId: number;
  entry: FileEntry;
  siblings: FileEntry[];
}

const PREVIEWABLE = new Set(["image", "video", "audio", "text", "pdf"]);

export function PreviewModal({
  target,
  onClose,
  onSetCover,
  onAddToNotes,
}: {
  target: PreviewTarget;
  onClose: () => void;
  onSetCover?: (sourceId: number, path: string) => void;
  onAddToNotes?: (sourceId: number, path: string) => void;
}) {
  const list = useMemo(() => target.siblings.filter((f) => PREVIEWABLE.has(f.kind)), [target]);
  const [index, setIndex] = useState(() => Math.max(0, list.findIndex((f) => f.path === target.entry.path)));
  const entry = list[index] ?? target.entry;
  const [info, setInfo] = useState<PreviewInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [proxy, setProxy] = useState<string | null>(null);
  const [proxyState, setProxyState] = useState<"none" | "working" | "failed">("none");
  const [poster, setPoster] = useState<string | null>(null);
  const [peaks, setPeaks] = useState<number[] | null>(null);
  const [progress, setProgress] = useState(0);
  const audioRef = useRef<HTMLAudioElement>(null);

  useEffect(() => {
    let alive = true;
    setInfo(null);
    setError(null);
    setProxy(null);
    setProxyState("none");
    setPoster(null);
    setPeaks(null);
    setProgress(0);
    api
      .previewFile(target.sourceId, entry.path)
      .then((i) => {
        if (!alive) return;
        setInfo(i);
        if ((i.kind === "video" && !i.playable) || (i.kind === "image" && !i.playable)) {
          api.getThumbnail(target.sourceId, entry.path, 1600).then((t) => alive && setPoster(t)).catch(() => undefined);
        }
        if (i.kind === "video" && !i.playable) {
          setProxyState("working");
          api
            .getVideoProxy(target.sourceId, entry.path)
            .then((p) => alive && (setProxy(p), setProxyState("none")))
            .catch(() => alive && setProxyState("failed"));
        }
        if (i.kind === "audio") api.getWaveform(target.sourceId, entry.path).then((p) => alive && setPeaks(p)).catch(() => undefined);
      })
      .catch((err) => alive && setError(errorText(err)));
    return () => {
      alive = false;
    };
  }, [target.sourceId, entry.path]);

  const go = useCallback((d: number) => setIndex((i) => (list.length ? (i + d + list.length) % list.length : i)), [list.length]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowRight") go(1);
      else if (e.key === "ArrowLeft") go(-1);
      else return;
      e.preventDefault();
      e.stopPropagation();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [go, onClose]);

  const m = info?.media;
  const meta = [
    m?.width ? `${m.width}×${m.height}` : null,
    m?.fps ? `${m.fps} fps` : null,
    m?.video_codec,
    m?.audio_codec,
    m?.duration ? formatDuration(m.duration) : null,
    info ? formatSize(info.size) : null,
  ].filter(Boolean);

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="preview-modal">
        <header className="preview-head">
          <Icon name={KIND_ICON[entry.kind] ?? "file"} size={16} />
          <div className="preview-title">
            <div className="truncate">{entry.name}</div>
            <div className="muted small">{meta.join(" · ")}</div>
          </div>
          {list.length > 1 && (
            <span className="muted small">
              {index + 1}/{list.length}
            </span>
          )}
          {onAddToNotes && (entry.kind === "image" || entry.kind === "video") && (
            <button className="btn tiny" onClick={() => onAddToNotes(target.sourceId, entry.path)} title="Lưu ảnh này (hoặc 1 khung hình video) vào ảnh minh họa của Notes">
              <Icon name="plus" size={12} /> Thêm vào Notes
            </button>
          )}
          {onSetCover && (entry.kind === "image" || entry.kind === "video") && (
            <button className="btn tiny" onClick={() => onSetCover(target.sourceId, entry.path)}>
              <Icon name="image" size={12} /> Đặt làm ảnh bìa
            </button>
          )}
          <button className="icon-btn" onClick={onClose} aria-label="Đóng">
            <Icon name="x" size={16} />
          </button>
        </header>
        <div className="preview-body">
          {list.length > 1 && (
            <button className="nav-arrow left" onClick={() => go(-1)} aria-label="Trước">
              ‹
            </button>
          )}
          {error && <div className="preview-msg">{error}</div>}
          {!info && !error && <div className="preview-msg"><span className="spinner" /> Đang tải…</div>}
          {info?.kind === "image" &&
            (info.playable && info.file ? (
              <img className="preview-img" src={convertFileSrc(info.file)} alt={info.name} />
            ) : poster ? (
              <img className="preview-img" src={convertFileSrc(poster)} alt={info.name} />
            ) : (
              <div className="preview-msg"><span className="spinner" /> Đang chuyển đổi ảnh…</div>
            ))}
          {info?.kind === "video" &&
            (info.playable && info.file ? (
              <video key={info.file} className="preview-video" src={convertFileSrc(info.file)} controls autoPlay />
            ) : proxy ? (
              <div className="preview-stack">
                <video key={proxy} className="preview-video" src={convertFileSrc(proxy)} controls autoPlay />
                <div className="muted small">Bản xem thử 720p · 2 phút đầu (codec gốc {m?.video_codec} không phát trực tiếp được)</div>
              </div>
            ) : (
              <div className="preview-stack">
                {poster && <img className="preview-img dim" src={convertFileSrc(poster)} alt="" />}
                <div className="preview-msg overlay">
                  {proxyState === "failed" ? (
                    "Không tạo được bản xem thử cho video này"
                  ) : (
                    <>
                      <span className="spinner" /> Đang tạo bản xem thử (GPU)…
                    </>
                  )}
                </div>
              </div>
            ))}
          {info?.kind === "audio" && info.file && (
            <div className="preview-audio">
              <Icon name="music" size={40} />
              {peaks ? (
                <Waveform
                  peaks={peaks}
                  progress={progress}
                  onSeek={(r) => {
                    const a = audioRef.current;
                    if (a && a.duration) {
                      a.currentTime = r * a.duration;
                      a.play();
                    }
                  }}
                />
              ) : (
                <div className="waveform placeholder" />
              )}
              {info.playable ? (
                <audio
                  ref={audioRef}
                  src={convertFileSrc(info.file)}
                  controls
                  autoPlay
                  onTimeUpdate={(e) => {
                    const a = e.currentTarget;
                    setProgress(a.duration ? a.currentTime / a.duration : 0);
                  }}
                />
              ) : (
                <div className="muted small">Định dạng này không phát trực tiếp được — chỉ xem waveform.</div>
              )}
            </div>
          )}
          {info?.kind === "text" && (
            <pre className="preview-text">
              {info.text}
              {info.truncated && "\n\n… (đã cắt, file quá dài)"}
            </pre>
          )}
          {info?.kind === "pdf" && info.file && <iframe className="preview-pdf" src={convertFileSrc(info.file)} title={info.name} />}
          {info && !PREVIEWABLE.has(info.kind) && <div className="preview-msg">Không có bản xem trước cho loại file này.</div>}
          {list.length > 1 && (
            <button className="nav-arrow right" onClick={() => go(1)} aria-label="Sau">
              ›
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
