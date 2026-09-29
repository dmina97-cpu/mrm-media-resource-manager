import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { api, TAG_SOURCE_LABEL, KIND_LABEL, RELATION_LABEL, RelationKind, ResourceDetail, ResourceSummary, SuggestedTag, Tag, TagKind } from "../api";
import { errorText, formatDateTime, formatNumber, formatSize } from "../format";
import { Icon } from "./Icon";
import { TagPicker } from "./TagPicker";
import { AppLogo } from "./AppLogo";
import { FileBrowser, invalidateCovers, PreviewModal, PreviewTarget, useCover } from "./Media";
import { ImageViewer, imagesFromClipboard, MAX_NOTE_IMAGES, NoteImages } from "./NoteImages";
import { CollectionPicker } from "./Collections";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

interface Props {
  focusId: number | null;
  selection: ResourceSummary[];
  tags: Tag[];
  reloadKey: number;
  onChanged: () => void;
  onFocus: (id: number) => void;
  onNext: (() => void) | null;
  onOpenMedia?: (view: "audio" | "image" | "video", resourceId: number, resourceName: string) => void;
  toast: (msg: string, kind?: "ok" | "err") => void;
  aiReady: boolean;
}

const SOURCE_LABEL: Record<SuggestedTag["source"], string> = {
  content: "Nội dung",
  path: "Thư mục",
  rule: "Rule",
  learned: "Học từ bạn",
  ai: "AI",
};

const EDIT_KINDS: TagKind[] = ["app", "type", "function", "personal", "status"];

export function Inspector(props: Props) {
  const { selection } = props;
  if (selection.length > 1) return <BulkInspector {...props} />;
  if (props.focusId === null) {
    return (
      <aside className="inspector empty">
        <Icon name="eye" size={28} />
        <p>Chọn một resource để xem chi tiết</p>
        <p className="muted small">Ctrl/Shift + click để chọn nhiều</p>
      </aside>
    );
  }
  return <SingleInspector {...props} focusId={props.focusId} />;
}

/** Số file âm thanh / ảnh / video trong resource -> mở thẳng Media Browser lọc theo resource. */
function ResourceMedia({
  resourceId,
  name,
  reloadKey,
  onOpen,
}: {
  resourceId: number;
  name: string;
  reloadKey: number;
  onOpen: NonNullable<Props["onOpenMedia"]>;
}) {
  const [c, setC] = useState<{ audio: number; image: number; video: number } | null>(null);
  useEffect(() => {
    let alive = true;
    api.resourceAssetCounts(resourceId).then((x) => alive && setC(x)).catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [resourceId, reloadKey]);
  if (!c || c.audio + c.image + c.video === 0) return null;
  const items: ["audio" | "image" | "video", string, string, number][] = [
    ["audio", "music", "âm thanh", c.audio],
    ["image", "image", "ảnh", c.image],
    ["video", "film", "video", c.video],
  ];
  return (
    <div className="insp-block">
      <div className="insp-label">File media</div>
      <div className="chips">
        {items
          .filter(([, , , n]) => n > 0)
          .map(([view, icon, label, n]) => (
            <button key={view} className="btn tiny" onClick={() => onOpen(view, resourceId, name)} title="Mở trong Media Browser">
              <Icon name={icon} size={12} /> {n.toLocaleString("en-US")} {label}
            </button>
          ))}
      </div>
    </div>
  );
}

function useCreateAndApply(ids: number[], props: Props) {
  return useCallback(
    async (kind: TagKind, name: string) => {
      try {
        const id = await api.createTag(kind, name);
        await api.setTag(ids, id, true);
        props.onChanged();
      } catch (err) {
        props.toast(errorText(err), "err");
      }
    },
    [ids.join(","), props.onChanged], // eslint-disable-line react-hooks/exhaustive-deps
  );
}

function SingleInspector(props: Props & { focusId: number }) {
  const { focusId, tags, onChanged, toast } = props;
  const [detail, setDetail] = useState<ResourceDetail | null>(null);
  const [name, setName] = useState("");
  const [notes, setNotes] = useState("");
  const [saving, setSaving] = useState<"idle" | "pending" | "saved">("idle");
  const notesRef = useRef({ id: focusId, value: "", dirty: false });
  const timer = useRef<number | undefined>(undefined);
  const [preview, setPreview] = useState<PreviewTarget | null>(null);
  const [showFiles, setShowFiles] = useState(false);
  const [similar, setSimilar] = useState<{ id: number; name: string; score: number }[]>([]);
  const [relKind, setRelKind] = useState<RelationKind>("related");
  const [relQuery, setRelQuery] = useState("");
  const [relHits, setRelHits] = useState<ResourceSummary[]>([]);
  const [analyzing, setAnalyzing] = useState(false);
  const [noteViewer, setNoteViewer] = useState<number | null>(null);
  const cover = useCover(focusId, 0);

  const flushNotes = useCallback(async () => {
    window.clearTimeout(timer.current);
    const n = notesRef.current;
    if (!n.dirty) return;
    n.dirty = false;
    try {
      await api.updateResource(n.id, { notes: n.value });
      setSaving("saved");
      onChanged();
    } catch (err) {
      toast(errorText(err), "err");
    }
  }, [onChanged, toast]);

  const load = useCallback(async () => {
    const d = await api.getResource(focusId);
    setDetail(d);
    if (d) {
      setName(d.name);
      if (!(notesRef.current.id === d.id && notesRef.current.dirty)) {
        setNotes(d.notes);
        notesRef.current = { id: d.id, value: d.notes, dirty: false };
      }
    }
  }, [focusId]);

  useEffect(() => {
    setSaving("idle");
    load();
    return () => {
      flushNotes();
    };
  }, [focusId]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    load();
  }, [props.reloadKey]); // eslint-disable-line react-hooks/exhaustive-deps

  // Resource tương tự (AI embedding)
  useEffect(() => {
    setSimilar([]);
    if (!props.aiReady) return;
    let alive = true;
    api
      .aiSimilar(focusId)
      .then(async (pairs) => {
        const out = [];
        for (const [id, score] of pairs.slice(0, 6)) {
          const d = await api.getResource(id);
          if (d) out.push({ id, name: d.name, score });
        }
        if (alive) setSimilar(out);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [focusId, props.aiReady]);

  // Tìm resource để tạo quan hệ
  useEffect(() => {
    if (!relQuery.trim()) {
      setRelHits([]);
      return;
    }
    const t = setTimeout(
      () =>
        api
          .queryResources({ view: "all", search: relQuery, sort: "relevance", desc: false, semantic: false })
          .then((r) => setRelHits(r.filter((x) => x.id !== focusId).slice(0, 6))),
      150,
    );
    return () => clearTimeout(t);
  }, [relQuery, focusId]);

  const createAndApply = useCreateAndApply([focusId], props);

  if (!detail) return <aside className="inspector" />;

  const tagById = new Map(tags.map((t) => [t.id, t]));
  const mine = detail.tag_ids.map((id) => tagById.get(id)).filter((t): t is Tag => !!t);
  const toggle = async (tag: Tag, on: boolean) => {
    try {
      await api.setTag([detail.id], tag.id, on);
      onChanged();
    } catch (err) {
      toast(errorText(err), "err");
    }
  };

  const applySuggestion = async (sug: { kind: TagKind; name: string; tag_id?: number | null }) => {
    if (sug.tag_id) await toggle({ id: sug.tag_id } as Tag, true);
    else await createAndApply(sug.kind, sug.name);
  };
  const applyAll = async (list: { kind: TagKind; name: string; tag_id?: number | null }[]) => {
    for (const s of list) {
      if (s.tag_id) await api.setTag([detail.id], s.tag_id, true);
      else await api.setTag([detail.id], await api.createTag(s.kind, s.name), true);
    }
    onChanged();
  };
  const suggested = detail.suggested;
  const totalSize = detail.sources.reduce((a, s) => a + s.size, 0);
  const has = (kind: TagKind, n: string) => mine.some((t) => t.kind === kind && t.name.toLowerCase() === n.toLowerCase());
  const ai = detail.ai;
  const aiTags: { kind: TagKind; name: string }[] = ai
    ? [
        ...ai.apps.map((n) => ({ kind: "app" as TagKind, name: n })),
        ...ai.types.map((n) => ({ kind: "type" as TagKind, name: n })),
        ...ai.function_tags.map((n) => ({ kind: "function" as TagKind, name: n })),
      ].filter((x) => !has(x.kind, x.name))
    : [];
  const runAi = async () => {
    setAnalyzing(true);
    try {
      await api.aiAnalyze([detail.id]);
      await load();
    } catch (err) {
      toast(errorText(err), "err");
    } finally {
      setAnalyzing(false);
    }
  };
  const parentRel = (rel: string) => rel.replace(/[\\/][^\\/]+$/, "");
  const parentName = (rel: string) => parentRel(rel).split(/[\\/]/).pop() ?? "";
  const regroup = async (libraryId: number, rel: string, mode: "pack" | "container" | null, ok: string) => {
    try {
      await api.setScanOverride(libraryId, rel, mode);
      const r = await api.scanLibrary(libraryId, false);
      toast(`${ok} · ${r.found} mục trong library`);
      onChanged();
    } catch (err) {
      toast(errorText(err), "err");
    }
  };
  const refreshAll = async () => {
    invalidateCovers([detail.id]);
    await load();
    onChanged();
  };
  const setCover = async (sourceId: number | null, path: string | null) => {
    await api.setCover(detail.id, sourceId, path);
    invalidateCovers([detail.id]);
    toast(sourceId ? "Đã đặt ảnh bìa" : "Ảnh bìa sẽ được chọn tự động");
    onChanged();
  };

  // Ảnh bìa: dán ảnh (Ctrl+V) hoặc chọn file -> thành ảnh Notes đầu tiên (= ảnh bìa)
  const coverFrom = async (input: Parameters<typeof api.setCoverImage>[1]) => {
    try {
      await api.setCoverImage(detail.id, input);
      toast("Đã đặt ảnh bìa");
      refreshAll();
    } catch (err) {
      const msg = errorText(err);
      toast(msg.includes("Tối đa") ? `Đã đủ ${MAX_NOTE_IMAGES} ảnh Notes — xóa bớt 1 ảnh để đặt ảnh bìa mới` : msg, "err");
    }
  };
  const pickCover = async () => {
    const sel = await openDialog({
      multiple: false,
      title: "Chọn ảnh bìa",
      filters: [{ name: "Ảnh / video", extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "psd", "mp4", "mov", "mkv", "webm"] }],
    });
    if (typeof sel === "string") coverFrom({ path: sel });
  };

  const saveName = async () => {
    if (name.trim() && name.trim() !== detail.name) {
      await api.updateResource(detail.id, { name: name.trim() });
      onChanged();
    } else setName(detail.name);
  };

  return (
    <aside className="inspector">
      <div className="insp-head">
        <input
          className="insp-title"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={saveName}
          onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
          spellCheck={false}
        />
        <button
          className={`fav-btn big ${detail.favorite ? "on" : ""}`}
          onClick={async () => {
            await api.updateResource(detail.id, { favorite: !detail.favorite });
            onChanged();
          }}
          title="Favorite"
        >
          <Icon name="star" size={18} filled={detail.favorite} />
        </button>
      </div>
      <div className="insp-sub">
        {detail.version && <span>v{detail.version}</span>}
        <span>{formatSize(totalSize)}</span>
        <span>{detail.sources.length} nguồn</span>
        {props.onNext && (
          <button className="btn tiny" onClick={props.onNext} title="Chuyển sang mục chưa phân loại tiếp theo (phím N)">
            Next unclassified <Icon name="arrowRight" size={12} />
          </button>
        )}
      </div>

      <div
        className="insp-cover-wrap"
        tabIndex={0}
        title="Dán ảnh (Ctrl+V) để đặt làm ảnh bìa"
        onPaste={async (e) => {
          const imgs = await imagesFromClipboard(e);
          if (imgs.length === 0) return;
          e.preventDefault();
          coverFrom(imgs[0]);
        }}
      >
      {!cover && (
        <button className="insp-cover empty" onClick={pickCover}>
          <Icon name="image" size={22} />
          <span>Chưa có ảnh bìa</span>
          <small>Bấm để chọn ảnh · hoặc bấm vào đây rồi dán (Ctrl+V)</small>
        </button>
      )}
      {cover && (
        <button className="cover-change" onClick={pickCover} title="Chọn ảnh khác làm ảnh bìa (hoặc bấm vào ảnh rồi dán Ctrl+V)">
          <Icon name="image" size={12} /> Đổi ảnh bìa
        </button>
      )}
      {cover && (
        <button
          className="insp-cover"
          onClick={() =>
            cover.kind === "note"
              ? setNoteViewer(0)
              : setPreview({
                  sourceId: cover.source_id,
                  entry: { name: cover.path.split("/").pop() || cover.path, path: cover.path, is_dir: false, size: 0, kind: cover.kind as "image" },
                  siblings: [],
                })
          }
          title="Xem"
        >
          <img src={convertFileSrc(cover.thumb)} alt="" />
          {cover.kind === "video" && (
            <span className="cover-badge big">
              <Icon name="play" size={14} filled />
            </span>
          )}
          {detail.cover_user && (
            <span
              className="cover-reset"
              onClick={(e) => {
                e.stopPropagation();
                setCover(null, null);
              }}
              title="Dùng ảnh bìa tự động"
            >
              <Icon name="refresh" size={12} />
            </span>
          )}
        </button>
      )}
      </div>

      {detail.auto_tags.length > 0 && (
        <div className="auto-notice">
          <Icon name="sparkles" size={13} />
          <span>
            {detail.auto_tags.length} tag được gắn tự động (viền đứt) — rê chuột để xem lý do.
          </span>
          <button
            className="link-btn"
            onClick={async () => {
              await api.confirmAutoTags([detail.id]);
              await load();
              onChanged();
            }}
          >
            Xác nhận hết
          </button>
        </div>
      )}

      {suggested.length > 0 && (
        <div className="insp-block suggest">
          <div className="insp-label">
            Gợi ý
            <button className="link-btn" onClick={() => applyAll(suggested)}>
              Áp dụng tất cả
            </button>
          </div>
          <div className="chips">
            {suggested.map((sg) => (
              <button
                key={sg.kind + sg.name}
                className={`chip k-${sg.kind} dashed`}
                onClick={() => applySuggestion(sg)}
                title={`${KIND_LABEL[sg.kind]} · ${sg.reason}`}
              >
                <Icon name="plus" size={11} /> {sg.name}
                <span className={`sg-src s-${sg.source}`}>{SOURCE_LABEL[sg.source]}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {(ai || props.aiReady) && (
        <div className="insp-block ai-block">
          <div className="insp-label">
            <span>
              <Icon name="sparkles" size={12} /> Phân tích AI
            </span>
            {props.aiReady && (
              <button className="link-btn" onClick={runAi} disabled={analyzing}>
                {analyzing ? "Đang phân tích…" : ai ? "Phân tích lại" : "Phân tích"}
              </button>
            )}
          </div>
          {analyzing && !ai && <div className="muted small"><span className="spinner" /> AI đang đọc cấu trúc file và README…</div>}
          {ai && (
            <>
              <p className="ai-desc">{ai.description}</p>
              {ai.install_notes && (
                <p className="ai-install">
                  <b>Cài đặt:</b> {ai.install_notes}
                </p>
              )}
              {aiTags.length > 0 && (
                <div className="chips">
                  {aiTags.map((t) => (
                    <button key={t.kind + t.name} className={`chip k-${t.kind} dashed`} onClick={() => applySuggestion(t)} title={KIND_LABEL[t.kind]}>
                      <Icon name="plus" size={11} /> {t.name}
                    </button>
                  ))}
                </div>
              )}
              <div className="row-actions">
                <button
                  className="btn tiny primary"
                  onClick={async () => {
                    await flushNotes();
                    await api.aiApply([detail.id]);
                    notesRef.current = { id: detail.id, value: "", dirty: false };
                    await load();
                    onChanged();
                  }}
                >
                  <Icon name="check" size={12} /> Áp dụng tất cả
                </button>
                <button
                  className="btn tiny"
                  onClick={async () => {
                    const next = notes.trim() ? `${notes.trim()}\n\n${ai.description}` : ai.description;
                    setNotes(next);
                    await api.updateResource(detail.id, { notes: next });
                    notesRef.current = { id: detail.id, value: next, dirty: false };
                    onChanged();
                  }}
                >
                  Thêm mô tả vào Notes
                </button>
                <button
                  className="btn tiny"
                  onClick={async () => {
                    await api.aiDismiss(detail.id);
                    load();
                  }}
                >
                  Bỏ qua
                </button>
              </div>
              <div className="muted tiny-text">Model: {ai.model} · AI chỉ đề xuất, không tự sửa dữ liệu</div>
            </>
          )}
        </div>
      )}

      {EDIT_KINDS.map((kind) => {
        const list = mine.filter((t) => t.kind === kind);
        return (
          <div className="insp-block" key={kind}>
            <div className="insp-label">{KIND_LABEL[kind]}</div>
            <div className="chips">
              {list.map((t) => {
                const auto = detail.auto_tags.find((a) => a.tag_id === t.id);
                return (
                  <span
                    key={t.id}
                    className={`chip k-${kind} ${auto ? "auto" : ""}`}
                    title={auto ? `Tự động · ${TAG_SOURCE_LABEL[auto.source] ?? auto.source}${auto.reason ? ` — ${auto.reason}` : ""}\nBấm ✓ để xác nhận, ✕ để gỡ (sẽ không tự gắn lại)` : undefined}
                  >
                    {kind === "app" && <AppLogo name={t.name} size={13} />}
                    {t.name}
                    {auto && (
                      <button
                        className="chip-x ok"
                        onClick={async () => {
                          await api.confirmAutoTags([detail.id], t.id);
                          await load();
                          onChanged();
                        }}
                        aria-label={`Xác nhận ${t.name}`}
                      >
                        <Icon name="check" size={10} />
                      </button>
                    )}
                    <button className="chip-x" onClick={() => toggle(t, false)} aria-label={`Bỏ ${t.name}`}>
                      <Icon name="x" size={10} />
                    </button>
                  </span>
                );
              })}
              <TagPicker
                kind={kind}
                tags={tags}
                selected={detail.tag_ids}
                onToggle={toggle}
                onCreate={createAndApply}
                allowCreate={kind !== "status"}
              />
            </div>
          </div>
        );
      })}

      <div className="insp-block">
        <div className="insp-label">
          Notes
          <span className="muted small">{saving === "pending" ? "đang lưu…" : saving === "saved" ? "đã lưu" : ""}</span>
        </div>
        <textarea
          className="notes"
          placeholder="Cái này là gì, dùng làm gì, cài thế nào, bên trong có gì…"
          value={notes}
          onChange={(e) => {
            setNotes(e.target.value);
            notesRef.current = { id: detail.id, value: e.target.value, dirty: true };
            setSaving("pending");
            window.clearTimeout(timer.current);
            timer.current = window.setTimeout(flushNotes, 700);
          }}
          onBlur={flushNotes}
          onPaste={async (e) => {
            const imgs = await imagesFromClipboard(e);
            if (imgs.length === 0) return; // dán chữ bình thường
            e.preventDefault();
            const room = MAX_NOTE_IMAGES - detail.note_images.length;
            if (room <= 0) {
              toast(`Đã đủ ${MAX_NOTE_IMAGES} ảnh`, "err");
              return;
            }
            try {
              for (const img of imgs.slice(0, room)) await api.addNoteImage(detail.id, img);
              toast("Đã thêm ảnh vào Notes");
            } catch (err) {
              toast(errorText(err), "err");
            }
            refreshAll();
          }}
        />
        <NoteImages
          resourceId={detail.id}
          images={detail.note_images}
          coverIsNote={!detail.cover_user}
          onChanged={refreshAll}
          toast={toast}
        />
      </div>

      {detail.matches.length > 0 && (
        <div className="insp-block">
          <div className="insp-label">Có thể là cùng tài nguyên</div>
          {detail.matches.map((m) => {
            const other = m.a.resource_id === detail.id ? m.b : m.a;
            return (
              <div key={m.id} className="match-mini">
                <div className="truncate" title={other.rel_path}>
                  <Icon name={other.kind === "folder" ? "folder" : "archive"} size={13} /> {other.name}
                </div>
                <span className="score">{m.score.toFixed(0)}%</span>
                <button
                  className="btn tiny primary"
                  onClick={async () => {
                    const keep = await api.resolveMatch(m.id, true);
                    onChanged();
                    if (keep) props.onFocus(keep);
                  }}
                >
                  Ghép
                </button>
                <button
                  className="btn tiny"
                  onClick={async () => {
                    await api.resolveMatch(m.id, false);
                    onChanged();
                  }}
                >
                  Bỏ qua
                </button>
              </div>
            );
          })}
        </div>
      )}

      {detail.versions.length > 1 && (
        <div className="insp-block">
          <div className="insp-label">Các phiên bản</div>
          {detail.versions.map((v) => (
            <button key={v.id} className={`rel-row ${v.id === detail.id ? "current" : ""}`} onClick={() => v.id !== detail.id && props.onFocus(v.id)}>
              <Icon name="git" size={13} />
              <span className="truncate">{v.name}</span>
              <span className={`badge ${v.latest ? "ok" : ""}`}>{v.version ? `v${v.version}` : "?"}{v.latest ? " · mới nhất" : ""}</span>
            </button>
          ))}
        </div>
      )}

      <div className="insp-block">
        <div className="insp-label">Quan hệ</div>
        {detail.relations.map((r) => (
          <div key={r.kind + r.other_id} className="rel-row">
            <span className="rel-kind">{RELATION_LABEL[r.kind][r.outgoing ? 0 : 1]}</span>
            <button className="truncate link-btn" onClick={() => props.onFocus(r.other_id)}>
              {r.other_name}
            </button>
            <button
              className="chip-x"
              onClick={async () => {
                await api.removeRelation(r.outgoing ? detail.id : r.other_id, r.outgoing ? r.other_id : detail.id, r.kind);
                load();
              }}
              aria-label="Bỏ quan hệ"
            >
              <Icon name="x" size={10} />
            </button>
          </div>
        ))}
        <div className="rel-add">
          <select value={relKind} onChange={(e) => setRelKind(e.target.value as RelationKind)} aria-label="Loại quan hệ">
            {(Object.keys(RELATION_LABEL) as RelationKind[]).map((k) => (
              <option key={k} value={k}>
                {RELATION_LABEL[k][0]}
              </option>
            ))}
          </select>
          <input placeholder="Tìm resource…" value={relQuery} onChange={(e) => setRelQuery(e.target.value)} />
        </div>
        {relHits.length > 0 && (
          <div className="rel-hits">
            {relHits.map((h) => (
              <button
                key={h.id}
                className="rel-row"
                onClick={async () => {
                  await api.addRelation(detail.id, h.id, relKind);
                  setRelQuery("");
                  load();
                }}
              >
                <Icon name="plus" size={12} />
                <span className="truncate">{h.name}</span>
              </button>
            ))}
          </div>
        )}
      </div>

      {similar.length > 0 && (
        <div className="insp-block">
          <div className="insp-label">
            <span>
              <Icon name="sparkles" size={12} /> Tương tự
            </span>
          </div>
          {similar.map((x) => (
            <button key={x.id} className="rel-row" onClick={() => props.onFocus(x.id)}>
              <span className="truncate">{x.name}</span>
              <span className="muted small">{Math.round(x.score * 100)}%</span>
            </button>
          ))}
        </div>
      )}

      <CollectionPicker kind="resource" ids={[detail.id]} onChanged={onChanged} toast={toast} />

      {props.onOpenMedia && <ResourceMedia resourceId={detail.id} name={detail.name} reloadKey={props.reloadKey} onOpen={props.onOpenMedia} />}

      {detail.sources.some((s) => s.available && s.kind !== "file") && (
        <div className="insp-block">
          <button className="insp-label clickable" onClick={() => setShowFiles((v) => !v)}>
            <span>
              <span className={`caret ${showFiles ? "open" : ""}`}>›</span> Nội dung bên trong
            </span>
          </button>
          {showFiles && (
            <FileBrowser sources={detail.sources} onOpen={(sourceId, entry, siblings) => setPreview({ sourceId, entry, siblings })} />
          )}
        </div>
      )}

      <div className="insp-block">
        <div className="insp-label">Sources</div>
        {detail.sources.map((s) => (
          <div key={s.id} className={`source ${s.available ? "" : "unavailable"}`}>
            <div className="source-head">
              <Icon name={s.kind === "folder" ? "folder" : s.kind === "archive" ? "archive" : "file"} size={15} />
              <span className="truncate source-name" title={s.name}>{s.name}</span>
              {!s.available && <span className="badge warn">unavailable</span>}
              {(s.match_state === "auto" || s.match_state === "confirmed") && s.match_score !== null && (
                <span className="badge" title={s.match_state === "auto" ? "Tự động ghép" : "Đã xác nhận"}>
                  {s.match_state === "auto" ? "auto" : "✓"} {s.match_score.toFixed(0)}%
                </span>
              )}
            </div>
            <div className="source-path" title={s.abs_path}>{s.abs_path}</div>
            <div className="source-meta">
              <span>{formatSize(s.size)}</span>
              {s.file_count > 0 && <span>{formatNumber(s.file_count)} file</span>}
              <span>{s.library_name}</span>
            </div>
            {s.extensions.length > 0 && (
              <div className="ext-list">
                {s.extensions.slice(0, 8).map(([ext, n]) => (
                  <span key={ext} className="ext">
                    .{ext} <b>{n}</b>
                  </span>
                ))}
              </div>
            )}
            <div className="source-actions">
              <button
                className="btn tiny"
                disabled={!s.available}
                onClick={() => api.revealSource(s.id).catch((err) => toast(errorText(err), "err"))}
              >
                <Icon name="folder" size={12} /> Mở vị trí
              </button>
              <button
                className="btn tiny"
                onClick={() => {
                  navigator.clipboard.writeText(s.abs_path);
                  toast("Đã copy đường dẫn");
                }}
              >
                <Icon name="copy" size={12} /> Copy path
              </button>
              {s.library_auto && s.available && s.kind === "folder" && s.override_mode !== "container" && (
                <button
                  className="btn tiny"
                  title="Thư mục này chứa nhiều gói khác nhau — tách mỗi thư mục/file con thành resource riêng (tag & notes được giữ lại cho các phần con)"
                  onClick={() => regroup(s.library_id, s.rel_path, "container", "Đã tách thành các resource con")}
                >
                  <Icon name="layers" size={12} /> Tách nhỏ
                </button>
              )}
              {s.library_auto && s.available && /[\\/]/.test(s.rel_path) && (
                <button
                  className="btn tiny"
                  title={`Gộp cả thư mục cha "${parentName(s.rel_path)}" thành một resource`}
                  onClick={() => regroup(s.library_id, parentRel(s.rel_path), "pack", `Đã gộp vào "${parentName(s.rel_path)}"`)}
                >
                  <Icon name="merge" size={12} /> Gộp thư mục cha
                </button>
              )}
              {s.override_mode && (
                <button
                  className="btn tiny"
                  title="Bỏ lựa chọn tách/gộp thủ công, để MRM tự nhận diện lại"
                  onClick={() => regroup(s.library_id, s.rel_path, null, "Đã trả về chế độ tự nhận diện")}
                >
                  Bỏ ghi đè
                </button>
              )}
              {detail.sources.length > 1 && (
                <button
                  className="btn tiny"
                  title="Tách nguồn này thành resource riêng"
                  onClick={async () => {
                    try {
                      await api.splitSource(s.id);
                      onChanged();
                      toast("Đã tách thành resource riêng");
                    } catch (err) {
                      toast(errorText(err), "err");
                    }
                  }}
                >
                  <Icon name="split" size={12} /> Tách
                </button>
              )}
            </div>
          </div>
        ))}
      </div>

      <div className="insp-block meta">
        {detail.ai_applied_at && (
          <div title="Tag và mô tả do AI điền — kiểm tra lại nếu chưa chuẩn. Tìm tất cả bằng is:ai">
            <span>AI đã điền</span>
            {formatDateTime(detail.ai_applied_at)}
          </div>
        )}
        <div><span>Thêm vào</span>{formatDateTime(detail.created_at)}</div>
        <div><span>Cập nhật</span>{formatDateTime(detail.updated_at)}</div>
      </div>
      {preview && (
        <PreviewModal
          target={preview}
          onClose={() => setPreview(null)}
          onSetCover={(sid, path) => {
            setCover(sid, path);
          }}
          onAddToNotes={
            detail.note_images.length < MAX_NOTE_IMAGES
              ? async (sid, path) => {
                  try {
                    await api.addNoteImage(detail.id, { source_id: sid, inner: path });
                    toast("Đã thêm vào ảnh Notes");
                    refreshAll();
                  } catch (err) {
                    toast(errorText(err), "err");
                  }
                }
              : undefined
          }
        />
      )}
      {noteViewer !== null && detail.note_images.length > 0 && (
        <ImageViewer
          files={detail.note_images.map((x) => x.file)}
          index={Math.min(noteViewer, detail.note_images.length - 1)}
          onIndex={setNoteViewer}
          onClose={() => setNoteViewer(null)}
        />
      )}
    </aside>
  );
}

function BulkInspector(props: Props) {
  const { selection, tags, onChanged, toast } = props;
  const ids = selection.map((r) => r.id);
  const createAndApply = useCreateAndApply(ids, props);
  const total = selection.reduce((a, r) => a + r.size, 0);
  const tagCount = new Map<number, number>();
  selection.forEach((r) => r.tag_ids.forEach((t) => tagCount.set(t, (tagCount.get(t) ?? 0) + 1)));
  const allFav = selection.every((r) => r.favorite);

  const toggle = async (tag: Tag) => {
    // Nếu tất cả đã có tag -> bỏ; ngược lại -> gắn cho tất cả
    const on = (tagCount.get(tag.id) ?? 0) < ids.length;
    try {
      await api.setTag(ids, tag.id, on);
      onChanged();
    } catch (err) {
      toast(errorText(err), "err");
    }
  };

  return (
    <aside className="inspector">
      <div className="insp-head">
        <div className="insp-title static">{selection.length} resource đã chọn</div>
      </div>
      <div className="insp-sub">
        <span>{formatSize(total)}</span>
      </div>
      <CollectionPicker kind="resource" ids={ids} onChanged={onChanged} toast={toast} />
      <div className="insp-block">
        <div className="bulk-actions">
          <button
            className="btn"
            onClick={async () => {
              await api.setFavorite(ids, !allFav);
              onChanged();
            }}
          >
            <Icon name="star" size={14} filled={!allFav} /> {allFav ? "Bỏ Favorite" : "Favorite"}
          </button>
          <button
            className="btn"
            title="Gộp các resource đã chọn thành một (giữ nguyên file thật)"
            onClick={async () => {
              if (!confirm(`Gộp ${ids.length} resource thành một? Tag và notes sẽ được hợp nhất. File thật không bị thay đổi.`)) return;
              try {
                const keep = await api.mergeResources(ids);
                onChanged();
                props.onFocus(keep);
                toast("Đã gộp resource");
              } catch (err) {
                toast(errorText(err), "err");
              }
            }}
          >
            <Icon name="merge" size={14} /> Gộp
          </button>
          {props.aiReady && (
            <button
              className="btn"
              title="AI đọc cấu trúc file/README và đề xuất mô tả + tag cho từng resource"
              onClick={async () => {
                toast(`AI bắt đầu phân tích ${ids.length} resource — kết quả hiện trong Inspector từng mục`);
                api.aiAnalyze(ids).catch((err) => toast(errorText(err), "err"));
              }}
            >
              <Icon name="sparkles" size={14} /> Phân tích AI
            </button>
          )}
          <button
            className="btn"
            title="Áp dụng kết quả AI đang chờ (tag + mô tả) cho các mục đã chọn"
            onClick={async () => {
              const n = await api.aiApply(ids);
              toast(n ? `Đã áp dụng kết quả AI cho ${n} resource` : "Các mục đã chọn không có kết quả AI đang chờ");
              onChanged();
            }}
          >
            <Icon name="check" size={14} /> Áp dụng kết quả AI
          </button>
          <button
            className="btn"
            title="Chấp nhận các tag gắn tự động của những mục đã chọn"
            onClick={async () => {
              const n = await api.confirmAutoTags(ids);
              toast(n ? `Đã xác nhận ${n} tag tự động` : "Không có tag tự động nào cần xác nhận");
              onChanged();
            }}
          >
            <Icon name="check" size={14} /> Xác nhận tag tự động
          </button>
        </div>
      </div>
      {EDIT_KINDS.map((kind) => {
        const present = tags.filter((t) => t.kind === kind && tagCount.has(t.id));
        return (
          <div className="insp-block" key={kind}>
            <div className="insp-label">{KIND_LABEL[kind]}</div>
            <div className="chips">
              {present.map((t) => {
                const n = tagCount.get(t.id) ?? 0;
                return (
                  <span key={t.id} className={`chip k-${kind} ${n < ids.length ? "partial" : ""}`} title={`${n}/${ids.length}`}>
                    {t.name}
                    {n < ids.length && <small>{n}/{ids.length}</small>}
                    <button
                      className="chip-x"
                      onClick={async () => {
                        await api.setTag(ids, t.id, false);
                        onChanged();
                      }}
                      aria-label={`Bỏ ${t.name}`}
                    >
                      <Icon name="x" size={10} />
                    </button>
                  </span>
                );
              })}
              <TagPicker
                kind={kind}
                tags={tags}
                selected={present.filter((t) => tagCount.get(t.id) === ids.length).map((t) => t.id)}
                onToggle={toggle}
                onCreate={createAndApply}
                allowCreate={kind !== "status"}
              />
            </div>
          </div>
        );
      })}
    </aside>
  );
}
