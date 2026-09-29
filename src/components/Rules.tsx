import { useEffect, useState } from "react";
import { api, KIND_LABEL, Rule, RuleCondition, Tag, TagKind } from "../api";
import { errorText } from "../format";
import { Icon } from "./Icon";
import { TagPicker } from "./TagPicker";

interface Props {
  tags: Tag[];
  reloadKey: number;
  onChanged: () => void;
  toast: (msg: string, kind?: "ok" | "err") => void;
}

const FIELD_LABEL: Record<RuleCondition["field"], string> = {
  name: "Tên resource",
  source: "Tên file/folder nguồn",
  path: "Đường dẫn",
  ext: "Có file đuôi",
  library: "Library",
};
const OP_LABEL: Record<RuleCondition["op"], string> = {
  contains: "chứa",
  not_contains: "không chứa",
  equals: "bằng",
  starts_with: "bắt đầu bằng",
  ends_with: "kết thúc bằng",
};

const emptyRule = (): Rule => ({
  id: 0,
  name: "",
  enabled: true,
  auto_apply: false,
  match_all: true,
  conditions: [{ field: "name", op: "contains", value: "" }],
  tag_ids: [],
});

export function Rules({ tags, reloadKey, onChanged, toast }: Props) {
  const [rules, setRules] = useState<Rule[]>([]);
  const [editing, setEditing] = useState<Rule | null>(null);
  const load = () => api.listRules().then(setRules);
  useEffect(() => {
    load();
  }, [reloadKey]);
  const tagById = new Map(tags.map((t) => [t.id, t]));

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Rules phân loại</h1>
          <p className="muted">
            Tự gắn tag theo tên, đường dẫn hoặc loại file. Rule “Gợi ý” chỉ hiện đề xuất trong Inspector; rule “Tự động” gắn tag ngay khi quét.
          </p>
        </div>
        <button className="btn primary" onClick={() => setEditing(emptyRule())}>
          <Icon name="plus" size={14} /> Rule mới
        </button>
      </div>

      {editing && (
        <RuleEditor
          rule={editing}
          tags={tags}
          onCancel={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            load();
          }}
          toast={toast}
        />
      )}

      {rules.length === 0 && !editing && (
        <div className="empty-state">
          <Icon name="rule" size={28} />
          <p>Chưa có rule nào.</p>
          <p className="muted small">Ví dụ: đường dẫn chứa “\LUTs\” → type LUT; có file .drfx → app DaVinci Resolve.</p>
        </div>
      )}

      {rules.map((r) => (
        <section key={r.id} className={`panel rule-card ${r.enabled ? "" : "disabled"}`}>
          <div className="rule-head">
            <b>{r.name}</b>
            <span className={`badge ${r.auto_apply ? "warn" : ""}`}>{r.auto_apply ? "Tự động" : "Gợi ý"}</span>
            {!r.enabled && <span className="badge">Tắt</span>}
            <div className="row-actions">
              <button
                className="btn tiny"
                onClick={async () => {
                  const n = await api.applyRuleNow(r.id);
                  toast(`Đã áp dụng cho ${n} resource`);
                  onChanged();
                }}
                title="Gắn tag cho mọi resource hiện có khớp rule"
              >
                Áp dụng ngay
              </button>
              <button className="btn tiny" onClick={() => setEditing(r)}>Sửa</button>
              <button
                className="btn tiny danger"
                onClick={async () => {
                  if (!confirm(`Xóa rule "${r.name}"? Tag đã gắn không bị gỡ.`)) return;
                  await api.deleteRule(r.id);
                  load();
                }}
              >
                Xóa
              </button>
            </div>
          </div>
          <div className="muted small">
            Nếu {r.match_all ? "tất cả" : "một trong"}:{" "}
            {r.conditions.map((c, i) => (
              <span key={i}>
                {i > 0 && (r.match_all ? " và " : " hoặc ")}
                <b>{FIELD_LABEL[c.field]}</b> {OP_LABEL[c.op]} “{c.value}”
              </span>
            ))}
          </div>
          <div className="chips" style={{ marginTop: 6 }}>
            <span className="muted small">→</span>
            {r.tag_ids.map((id) => {
              const t = tagById.get(id);
              return t ? <span key={id} className={`chip k-${t.kind}`}>{t.name}</span> : null;
            })}
          </div>
        </section>
      ))}
    </div>
  );
}

function RuleEditor({
  rule,
  tags,
  onCancel,
  onSaved,
  toast,
}: {
  rule: Rule;
  tags: Tag[];
  onCancel: () => void;
  onSaved: () => void;
  toast: (msg: string, kind?: "ok" | "err") => void;
}) {
  const [r, setR] = useState<Rule>(rule);
  const [test, setTest] = useState<{ count: number; sample: string[] } | null>(null);
  const [localTags, setLocalTags] = useState(tags);
  useEffect(() => setLocalTags(tags), [tags]);

  useEffect(() => {
    if (r.conditions.every((c) => !c.value.trim())) {
      setTest(null);
      return;
    }
    const t = setTimeout(() => api.testRule({ ...r, tag_ids: r.tag_ids.length ? r.tag_ids : [0] }).then(setTest).catch(() => setTest(null)), 300);
    return () => clearTimeout(t);
  }, [r]);

  const setCond = (i: number, patch: Partial<RuleCondition>) =>
    setR({ ...r, conditions: r.conditions.map((c, j) => (j === i ? { ...c, ...patch } : c)) });

  return (
    <section className="panel rule-editor">
      <div className="form-row">
        <label>Tên rule</label>
        <input value={r.name} onChange={(e) => setR({ ...r, name: e.target.value })} placeholder="VD: LUT trong thư mục LUTs" autoFocus />
      </div>
      <div className="form-row">
        <label>Khớp khi</label>
        <select value={r.match_all ? "all" : "any"} onChange={(e) => setR({ ...r, match_all: e.target.value === "all" })}>
          <option value="all">tất cả điều kiện</option>
          <option value="any">một trong các điều kiện</option>
        </select>
      </div>
      {r.conditions.map((c, i) => (
        <div key={i} className="form-row cond">
          <select value={c.field} onChange={(e) => setCond(i, { field: e.target.value as RuleCondition["field"] })}>
            {Object.entries(FIELD_LABEL).map(([k, v]) => (
              <option key={k} value={k}>{v}</option>
            ))}
          </select>
          <select value={c.op} onChange={(e) => setCond(i, { op: e.target.value as RuleCondition["op"] })}>
            {Object.entries(OP_LABEL).map(([k, v]) => (
              <option key={k} value={k}>{v}</option>
            ))}
          </select>
          <input value={c.value} onChange={(e) => setCond(i, { value: e.target.value })} placeholder={c.field === "ext" ? ".cube" : "giá trị"} />
          {r.conditions.length > 1 && (
            <button className="icon-btn" onClick={() => setR({ ...r, conditions: r.conditions.filter((_, j) => j !== i) })} aria-label="Bỏ điều kiện">
              <Icon name="x" size={13} />
            </button>
          )}
        </div>
      ))}
      <button className="link-btn" onClick={() => setR({ ...r, conditions: [...r.conditions, { field: "name", op: "contains", value: "" }] })}>
        + Thêm điều kiện
      </button>

      <div className="form-row">
        <label>Gắn tag</label>
        <div className="chips">
          {r.tag_ids.map((id) => {
            const t = localTags.find((x) => x.id === id);
            return t ? (
              <span key={id} className={`chip k-${t.kind}`}>
                {t.name}
                <button className="chip-x" onClick={() => setR({ ...r, tag_ids: r.tag_ids.filter((x) => x !== id) })} aria-label="Bỏ">
                  <Icon name="x" size={10} />
                </button>
              </span>
            ) : null;
          })}
          {(["app", "type", "function", "personal", "status"] as TagKind[]).map((kind) => (
            <TagPicker
              key={kind}
              kind={kind}
              label={KIND_LABEL[kind]}
              tags={localTags}
              selected={r.tag_ids}
              allowCreate={kind !== "status"}
              onToggle={(t, on) => setR({ ...r, tag_ids: on ? [...r.tag_ids, t.id] : r.tag_ids.filter((x) => x !== t.id) })}
              onCreate={async (k, name) => {
                const id = await api.createTag(k, name);
                setLocalTags(await api.listTags());
                setR((cur) => ({ ...cur, tag_ids: [...cur.tag_ids, id] }));
              }}
            />
          ))}
        </div>
      </div>

      <div className="form-row">
        <label>Chế độ</label>
        <select value={r.auto_apply ? "auto" : "suggest"} onChange={(e) => setR({ ...r, auto_apply: e.target.value === "auto" })}>
          <option value="suggest">Gợi ý — hiện trong Inspector để bạn xác nhận</option>
          <option value="auto">Tự động — gắn tag khi quét resource mới/thay đổi</option>
        </select>
        <label className="check-label">
          <input type="checkbox" checked={r.enabled} onChange={(e) => setR({ ...r, enabled: e.target.checked })} /> Bật
        </label>
      </div>

      {test && (
        <div className="rule-test">
          Khớp <b>{test.count}</b> resource hiện có{test.sample.length > 0 && ": "}
          <span className="muted">{test.sample.join(", ")}{test.count > test.sample.length ? "…" : ""}</span>
        </div>
      )}

      <div className="row-actions">
        <button
          className="btn primary"
          onClick={async () => {
            try {
              await api.saveRule(r);
              toast("Đã lưu rule");
              onSaved();
            } catch (err) {
              toast(errorText(err), "err");
            }
          }}
        >
          Lưu rule
        </button>
        <button className="btn" onClick={onCancel}>Hủy</button>
      </div>
    </section>
  );
}
