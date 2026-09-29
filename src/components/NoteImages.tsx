import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { api, NoteImage } from "../api";
import { errorText } from "../format";
import { Icon } from "./Icon";

export const MAX_NOTE_IMAGES = 3;
const IMAGE_EXT = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "heic", "avif", "psd", "exr", "tga"];
const VIDEO_EXT = ["mp4", "mov", "m4v", "mkv", "webm", "avi"];

function extOf(p: string) {
  return p.split(".").pop()?.toLowerCase() ?? "";
}

/** Đọc ảnh từ clipboard (Ctrl+V) -> base64 */
export async function imagesFromClipboard(e: React.ClipboardEvent): Promise<{ data_b64: string; ext: string }[]> {
  const files = Array.from(e.clipboardData.items)
    .filter((i) => i.kind === "file" && i.type.startsWith("image/"))
    .map((i) => i.getAsFile())
    .filter((f): f is File => !!f);
  return Promise.all(
    files.map(
      (f) =>
        new Promise<{ data_b64: string; ext: string }>((resolve, reject) => {
          const r = new FileReader();
          r.onload = () => resolve({ data_b64: String(r.result), ext: f.type.split("/")[1] || "png" });
          r.onerror = () => reject(r.error);
          r.readAsDataURL(f);
        }),
    ),
  );
}

interface Props {
  resourceId: number;
  images: NoteImage[];
  coverIsNote: boolean;
  onChanged: () => void;
  toast: (msg: string, kind?: "ok" | "err") => void;
}

export function NoteImages({ resourceId, images, coverIsNote, onChanged, toast }: Props) {
  const [viewer, setViewer] = useState<number | null>(null);
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);
  const room = MAX_NOTE_IMAGES - images.length;

  const addMany = async (inputs: Parameters<typeof api.addNoteImage>[1][]) => {
    if (inputs.length === 0) return;
    if (inputs.length > room) toast(`Chỉ thêm được ${room} ảnh nữa (tối đa ${MAX_NOTE_IMAGES})`, "err");
    setBusy(true);
    try {
      for (const input of inputs.slice(0, Math.max(0, room))) await api.addNoteImage(resourceId, input);
    } catch (err) {
      toast(errorText(err), "err");
    } finally {
      setBusy(false);
      onChanged();
    }
  };

  // Kéo thả file từ Explorer vào cửa sổ app khi đang xem resource này
  useEffect(() => {
    let un: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((ev) => {
        const p = ev.payload;
        if (p.type === "enter" || p.type === "over") setDragging(true);
        else if (p.type === "leave") setDragging(false);
        else if (p.type === "drop") {
          setDragging(false);
          const media = p.paths.filter((x) => IMAGE_EXT.includes(extOf(x)) || VIDEO_EXT.includes(extOf(x)));
          if (media.length === 0) {
            toast("Chỉ thả được file ảnh hoặc video", "err");
            return;
          }
          addMany(media.map((path) => ({ path })));
        }
      })
      .then((f) => (un = f));
    return () => un?.();
  }, [resourceId, images.length]); // eslint-disable-line react-hooks/exhaustive-deps

  const pick = async () => {
    const sel = await open({
      multiple: true,
      title: "Chọn ảnh minh họa (tối đa 3)",
      filters: [{ name: "Ảnh / video", extensions: [...IMAGE_EXT, ...VIDEO_EXT] }],
    });
    const paths = Array.isArray(sel) ? sel : sel ? [sel] : [];
    addMany(paths.map((path) => ({ path })));
  };

  return (
    <div className={`note-images ${dragging ? "dragging" : ""}`}>
      <div className="note-grid">
        {images.map((img, i) => (
          <div key={img.id} className="note-thumb">
            <img src={convertFileSrc(img.file)} alt="" onClick={() => setViewer(i)} draggable={false} />
            {i === 0 && coverIsNote && <span className="note-cover-badge">Ảnh bìa</span>}
            <div className="note-actions">
              {!(i === 0 && coverIsNote) && (
                <button
                  title="Dùng làm ảnh bìa"
                  onClick={async () => {
                    await api.noteImageToFront(img.id);
                    onChanged();
                  }}
                >
                  <Icon name="image" size={12} />
                </button>
              )}
              <button
                title="Xóa ảnh"
                onClick={async () => {
                  await api.removeNoteImage(img.id);
                  onChanged();
                }}
              >
                <Icon name="x" size={12} />
              </button>
            </div>
          </div>
        ))}
        {room > 0 && (
          <button className="note-add" onClick={pick} disabled={busy} title="Chọn ảnh, dán (Ctrl+V) hoặc kéo thả vào đây">
            {busy ? <span className="spinner" /> : <Icon name="plus" size={16} />}
            <span>{busy ? "Đang thêm…" : "Thêm ảnh"}</span>
          </button>
        )}
      </div>
      {dragging && <div className="note-drop">Thả ảnh để thêm vào Notes</div>}
      {images.length === 0 && !dragging && (
        <div className="muted tiny-text">Dán (Ctrl+V), kéo thả hoặc chọn tối đa {MAX_NOTE_IMAGES} ảnh minh họa — ảnh đầu tiên làm ảnh bìa.</div>
      )}
      {viewer !== null && images[viewer] && (
        <ImageViewer files={images.map((x) => x.file)} index={viewer} onIndex={setViewer} onClose={() => setViewer(null)} />
      )}
    </div>
  );
}

export function ImageViewer({ files, index, onIndex, onClose }: { files: string[]; index: number; onIndex: (i: number) => void; onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowRight") onIndex((index + 1) % files.length);
      else if (e.key === "ArrowLeft") onIndex((index - 1 + files.length) % files.length);
      else return;
      e.preventDefault();
      e.stopPropagation();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [index, files.length, onClose, onIndex]);
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="preview-modal">
        <header className="preview-head">
          <Icon name="image" size={16} />
          <div className="preview-title">Ảnh minh họa</div>
          {files.length > 1 && (
            <span className="muted small">
              {index + 1}/{files.length}
            </span>
          )}
          <button className="icon-btn" onClick={onClose} aria-label="Đóng">
            <Icon name="x" size={16} />
          </button>
        </header>
        <div className="preview-body">
          {files.length > 1 && (
            <button className="nav-arrow left" onClick={() => onIndex((index - 1 + files.length) % files.length)} aria-label="Trước">
              ‹
            </button>
          )}
          <img className="preview-img" src={convertFileSrc(files[index])} alt="" />
          {files.length > 1 && (
            <button className="nav-arrow right" onClick={() => onIndex((index + 1) % files.length)} aria-label="Sau">
              ›
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
