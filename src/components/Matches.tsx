import { useEffect, useState } from "react";
import { api, MatchSuggestion, SourceBrief } from "../api";
import { errorText, formatSize } from "../format";
import { Icon } from "./Icon";

interface Props {
  reloadKey: number;
  onChanged: () => void;
  toast: (msg: string, kind?: "ok" | "err") => void;
}

export function Matches({ reloadKey, onChanged, toast }: Props) {
  const [list, setList] = useState<MatchSuggestion[] | null>(null);
  useEffect(() => {
    api.listMatchSuggestions().then(setList);
  }, [reloadKey]);

  const resolve = async (m: MatchSuggestion, accept: boolean) => {
    try {
      await api.resolveMatch(m.id, accept);
      setList((l) => l?.filter((x) => x.id !== m.id) ?? null);
      onChanged();
    } catch (err) {
      toast(errorText(err), "err");
    }
  };

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Match suggestions</h1>
          <p className="muted">
            Các cặp có độ tương đồng 75–94% (hoặc ≥95% nhưng khác thư mục). Ghép chỉ thay đổi metadata — file thật giữ nguyên.
          </p>
        </div>
      </div>
      {list === null ? null : list.length === 0 ? (
        <div className="empty-state">
          <Icon name="check" size={28} />
          <p>Không còn đề xuất nào cần xác nhận.</p>
        </div>
      ) : (
        <div className="match-list">
          {list.map((m) => (
            <div key={m.id} className="match-row">
              <SourceCell s={m.a} />
              <div className="match-score">
                <div className="score-big">{m.score.toFixed(0)}%</div>
                <div className="muted small">tương đồng</div>
              </div>
              <SourceCell s={m.b} />
              <div className="match-actions">
                <button className="btn primary" onClick={() => resolve(m, true)}>
                  <Icon name="link" size={14} /> Ghép
                </button>
                <button className="btn" onClick={() => resolve(m, false)}>
                  Không phải
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function SourceCell({ s }: { s: SourceBrief }) {
  return (
    <div className="match-source">
      <div className="match-name">
        <Icon name={s.kind === "folder" ? "folder" : "archive"} size={16} />
        <span className="truncate" title={s.name}>{s.name}</span>
      </div>
      <div className="muted small truncate" title={s.rel_path}>
        {s.library_name} › {s.rel_path}
      </div>
      <div className="muted small">{formatSize(s.size)}</div>
    </div>
  );
}
