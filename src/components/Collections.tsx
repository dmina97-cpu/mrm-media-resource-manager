import { useEffect, useState } from "react";
import { api, CollectionInfo } from "../api";
import { errorText } from "../format";
import { Icon } from "./Icon";

type Toast = (msg: string, kind?: "ok" | "err") => void;

/** Hỏi tên rồi tạo collection mới; trả về id (hoặc null nếu hủy). */
export async function promptNewCollection(toast: Toast): Promise<number | null> {
  const name = window.prompt("Tên collection mới (vd: SFX vlog, LUT điện ảnh):");
  if (!name?.trim()) return null;
  try {
    return await api.createCollection(name.trim());
  } catch (err) {
    toast(errorText(err), "err");
    return null;
  }
}

/**
 * Gắn / gỡ collection cho resource hoặc file media.
 * - 1 mục: hiện các collection đang chứa (bấm × để gỡ).
 * - nhiều mục: chỉ thêm vào collection.
 */
export function CollectionPicker({
  kind,
  ids,
  onChanged,
  toast,
}: {
  kind: "resource" | "asset";
  ids: number[];
  onChanged: () => void;
  toast: Toast;
}) {
  const [all, setAll] = useState<CollectionInfo[]>([]);
  const [mine, setMine] = useState<number[]>([]);
  const single = ids.length === 1;
  const key = ids.join(",");

  const load = () => {
    api.listCollections().then(setAll).catch(() => undefined);
    if (single) {
      api
        .collectionsOf(kind === "resource" ? ids[0] : null, kind === "asset" ? ids[0] : null)
        .then(setMine)
        .catch(() => setMine([]));
    } else setMine([]);
  };
  useEffect(load, [key, kind]); // eslint-disable-line react-hooks/exhaustive-deps

  const set = async (collectionId: number, on: boolean) => {
    try {
      if (kind === "resource") await api.setCollectionResources(collectionId, ids, on);
      else await api.setCollectionAssets(collectionId, ids, on);
      const name = all.find((c) => c.id === collectionId)?.name ?? "collection";
      if (!single) toast(on ? `Đã thêm ${ids.length} mục vào “${name}”` : `Đã gỡ khỏi “${name}”`);
      load();
      onChanged();
    } catch (err) {
      toast(errorText(err), "err");
    }
  };

  const available = all.filter((c) => !mine.includes(c.id));
  return (
    <div className="insp-block">
      <div className="insp-label">Collections</div>
      <div className="chips">
        {all
          .filter((c) => mine.includes(c.id))
          .map((c) => (
            <span key={c.id} className="chip coll-chip">
              <Icon name="layers" size={11} /> {c.name}
              <button className="chip-x" onClick={() => set(c.id, false)} aria-label={`Gỡ khỏi ${c.name}`}>
                <Icon name="x" size={10} />
              </button>
            </span>
          ))}
        <select
          className="coll-add"
          value=""
          onChange={async (e) => {
            const v = e.target.value;
            if (v === "new") {
              const id = await promptNewCollection(toast);
              if (id != null) set(id, true);
            } else if (v) set(Number(v), true);
          }}
          aria-label="Thêm vào collection"
        >
          <option value="">{single ? "+ Thêm vào collection…" : `+ Thêm ${ids.length} mục vào collection…`}</option>
          {available.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
          <option value="new">＋ Tạo collection mới…</option>
        </select>
      </div>
    </div>
  );
}
