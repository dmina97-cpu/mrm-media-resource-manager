'use strict';
// MRM Shared Core adapter — HNH SFX Finder dùng chung database với MRM (Media Resource Manager).
//
// - Đọc media index qua view `api_assets` (MRM là nơi duy nhất quét thư viện).
// - Ghi favorite / tag / recent / usage / smart collection vào bảng asset_* bằng transaction ngắn.
// - Giữ nguyên "hình dạng" dữ liệu cũ của plugin (settings + index) để UI và các module Resolve
//   (Auto Bin, Smart Track, Insert…) chạy như trước.
// - Setting riêng của plugin (UI, track, insert mode…) lưu trong thư mục dữ liệu riêng, không vào DB chung.
// - Gửi "nhịp tim" để MRM biết plugin đang mở trong DaVinci.

const fs = require('node:fs');
const fsp = fs.promises;
const path = require('node:path');

let sqlite = null;
try { sqlite = require('node:sqlite'); } catch { sqlite = null; }

const CONTRACT = 1;
const APP = 'hnh-sfx-finder';
const SHARED_KEYS = new Set(['libraries', 'favorites', 'recent', 'usage', 'tags', 'collections']);
const DEFAULT_UI = { autoPreview: true, sortMode: 'relevance', extension: 'all', library: 'all', trackIndex: 1 };

const norm = s => String(s || '').normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/[đĐ]/g, 'd').toLowerCase().replace(/[_\-.()[\]{}]+/g, ' ').replace(/\s+/g, ' ').trim();
const key = p => (process.platform === 'win32' ? path.resolve(p).toLowerCase() : path.resolve(p));

function defaultDbPath() {
  if (process.env.MRM_DB) return process.env.MRM_DB;
  const base = process.env.APPDATA || path.join(require('node:os').homedir(), 'AppData', 'Roaming');
  return path.join(base, 'com.nink.vault', 'nink-vault.db');
}

async function readJson(file, fallback) {
  try { return JSON.parse(await fsp.readFile(file, 'utf8')); } catch { return fallback; }
}
async function writeJson(file, data) {
  await fsp.mkdir(path.dirname(file), { recursive: true });
  const tmp = file + '.' + process.pid + '.' + Date.now() + '.tmp';
  await fsp.writeFile(tmp, JSON.stringify(data, null, 2), 'utf8');
  await fsp.rename(tmp, file);
}

class MrmStore {
  /**
   * @param {object} o
   * @param {string} o.privateDir  thư mục dữ liệu riêng của plugin (UI, cache waveform/thumbnail)
   * @param {string} [o.legacyDir] thư mục dữ liệu của HNH SFX Finder v0.10.2 (chỉ đọc, để chuyển dữ liệu)
   * @param {string} [o.standaloneDir] dữ liệu của chính plugin khi chạy độc lập (chưa có MRM) — chuyển sang MRM khi cài MRM
   * @param {string} o.version     phiên bản plugin
   * @param {string} [o.dbPath]
   */
  constructor({ privateDir, legacyDir, standaloneDir, version, dbPath }) {
    this.privateDir = privateDir;
    this.legacyDir = legacyDir;
    this.standaloneDir = standaloneDir;
    this.version = version;
    this.dbPath = dbPath || defaultDbPath();
    this.db = null;
    this.error = null;
    this._index = null;
    this._indexVersion = null;
    this._seq = new Map();
    this._byKey = new Map();
    this._heartbeat = null;
  }

  // ------------------------------------------------------------------ kết nối

  open() {
    if (this.db) return { ok: true };
    if (!sqlite) return this._fail('Resolve không có node:sqlite — cần DaVinci Resolve 20.2 trở lên.');
    if (!fs.existsSync(this.dbPath)) return this._fail('Chưa tìm thấy dữ liệu MRM. Hãy cài/mở MRM và thêm thư viện trước.');
    try {
      const db = new sqlite.DatabaseSync(this.dbPath);
      db.exec('PRAGMA busy_timeout = 5000; PRAGMA foreign_keys = ON;');
      const ver = db.prepare('PRAGMA user_version').get().user_version;
      const contract = db.prepare("SELECT value FROM settings WHERE key = 'plugin_contract'").get();
      if (ver < 7 || !contract) { db.close(); return this._fail('Phiên bản MRM quá cũ — hãy cập nhật MRM.'); }
      if (Number(contract.value) !== CONTRACT) { db.close(); return this._fail(`Plugin cần hợp đồng dữ liệu v${CONTRACT}, MRM đang dùng v${contract.value}. Hãy cập nhật plugin hoặc MRM cho khớp.`); }
      this.db = db;
      this.error = null;
      return { ok: true };
    } catch (e) {
      return this._fail('Không mở được dữ liệu MRM: ' + (e.message || e));
    }
  }
  _fail(msg) { this.error = msg; return { ok: false, error: msg }; }
  status() { return { ok: !!this.db, error: this.error, dbPath: this.dbPath }; }
  close() {
    this.stopHeartbeat();
    try { this.db?.close(); } catch {}
    this.db = null;
  }

  /** Ghi an toàn: BEGIN IMMEDIATE + thử lại khi MRM đang ghi. */
  write(fn, retries = 5) {
    if (!this.open().ok) throw new Error(this.error);
    for (let attempt = 0; ; attempt++) {
      try {
        this.db.exec('BEGIN IMMEDIATE');
        try {
          const r = fn(this.db);
          this.db.exec('COMMIT');
          return r;
        } catch (e) {
          try { this.db.exec('ROLLBACK'); } catch {}
          throw e;
        }
      } catch (e) {
        if (/busy|locked/i.test(String(e.message)) && attempt < retries) { const t = Date.now() + 200 * (attempt + 1); while (Date.now() < t); continue; }
        throw e;
      }
    }
  }

  /**
   * Phiên bản nội dung media: MRM tăng `media_rev` khi index/metadata thay đổi thật sự.
   * (Không dùng PRAGMA data_version vì nhịp tim của MRM ghi mỗi 2 giây sẽ làm plugin tải lại liên tục.)
   */
  contentVersion() {
    if (!this.open().ok) return null;
    const r = this.db.prepare("SELECT value FROM settings WHERE key = 'media_rev'").get();
    return r ? String(r.value) : '0';
  }

  /** Số thay đổi từ kết nối khác (MRM). */
  dataVersion() {
    if (!this.open().ok) return null;
    return this.db.prepare('PRAGMA data_version').get().data_version;
  }

  // ------------------------------------------------------------------ đọc

  loadIndex() {
    if (!this.open().ok) return { version: 3, generatedAt: null, files: [], mrm: this.status() };
    const dv = this.contentVersion();
    if (this._index && this._indexVersion === dv) return this._index;
    const rows = this.db.prepare(
      `SELECT id, uid, root, rel_path, path, filename, ext, media_type, size, modified_ms, added_at, duration,
              width, height, fps, codec, seq_pattern, seq_start, seq_end, seq_count, resource_id, resource_name
       FROM api_assets WHERE available = 1`
    ).all();
    const files = [];
    this._seq.clear();
    this._byKey.clear();
    for (const r of rows) {
      const rel = r.rel_path;
      const folders = path.dirname(rel).split(path.sep);
      const basename = path.basename(r.filename, path.extname(r.filename));
      const f = {
        id: key(r.path),
        assetId: r.id,
        uid: r.uid,
        path: r.path,
        root: r.root,
        relativePath: rel,
        filename: r.filename,
        basename,
        extension: r.ext,
        folders,
        size: r.size,
        mtimeMs: r.modified_ms,
        addedAt: r.added_at ? r.added_at * 1000 : null,
        searchText: norm([basename, ...folders].join(' ')),
      };
      if (Number.isFinite(r.duration)) f.duration = r.duration;
      if (r.width) { f.width = r.width; f.height = r.height; }
      if (r.fps) f.fps = r.fps;
      if (r.codec) f.codec = r.codec;
      if (r.resource_id) f.resource = { id: r.resource_id, name: r.resource_name };
      if (r.seq_pattern) {
        f.sequence = { pattern: path.join(path.dirname(r.path), r.seq_pattern), start: r.seq_start, end: r.seq_end, count: r.seq_count };
        f.searchText += ' ' + norm(r.seq_pattern.replace(/%0\d+d/, ' sequence'));
        this._seq.set(key(r.path), f.sequence);
      }
      this._byKey.set(f.id, r.id);
      files.push(f);
    }
    this._index = { version: 3, generatedAt: new Date().toISOString(), files, stats: { added: 0, changed: 0, removed: 0, unchanged: files.length }, warnings: [] };
    this._indexVersion = dv;
    return this._index;
  }

  /** asset id theo đường dẫn (không phân biệt hoa thường trên Windows). */
  assetId(p) {
    if (!this._byKey.size) this.loadIndex();
    const k = key(p);
    if (this._byKey.has(k)) return this._byKey.get(k);
    const row = this.db?.prepare('SELECT id FROM api_assets WHERE lower(path) = ?').get(k.toLowerCase());
    return row ? row.id : null;
  }

  sequenceFor(p) {
    if (!this._seq.size) this.loadIndex();
    return this._seq.get(key(p)) || null;
  }

  _sharedSettings() {
    const db = this.db;
    const full = (root, rel) => (/[\\/]$/.test(root) ? root + rel : root + '\\' + rel);
    const libraries = db.prepare('SELECT path, media_types FROM api_libraries ORDER BY name').all().map(l => {
      let types; try { types = JSON.parse(l.media_types); } catch { types = ['audio', 'image', 'video']; }
      return { path: l.path, enabled: true, types };
    });
    const favorites = db.prepare(
      `SELECT l.path AS root, a.rel_path AS rel FROM asset_favorites f
       JOIN media_assets a ON a.id = f.asset_id JOIN libraries l ON l.id = a.library_id ORDER BY f.created_at DESC`
    ).all().map(r => full(r.root, r.rel));
    const usageRows = db.prepare(
      `SELECT l.path AS root, a.rel_path AS rel, u.last_used_at, u.use_count FROM asset_usage u
       JOIN media_assets a ON a.id = u.asset_id JOIN libraries l ON l.id = a.library_id`
    ).all();
    const recent = usageRows.filter(r => r.last_used_at > 0).sort((a, b) => b.last_used_at - a.last_used_at).slice(0, 100).map(r => full(r.root, r.rel));
    const usage = {};
    for (const r of usageRows) if (r.use_count > 0) usage[full(r.root, r.rel)] = r.use_count;
    const tags = {};
    for (const r of db.prepare(
      `SELECT l.path AS root, a.rel_path AS rel, t.name FROM asset_tags at
       JOIN tags t ON t.id = at.tag_id JOIN media_assets a ON a.id = at.asset_id JOIN libraries l ON l.id = a.library_id ORDER BY t.name`
    ).all()) (tags[full(r.root, r.rel)] ||= []).push(r.name);
    const collections = db.prepare('SELECT id, name, filters FROM asset_smart_collections ORDER BY position, name').all().map(c => {
      let filters; try { filters = JSON.parse(c.filters); } catch { filters = {}; }
      return { id: c.id, name: c.name, filters };
    });
    const shared = { libraries, favorites, recent, usage, tags, collections };
    this._lastShared = JSON.stringify(shared);
    return shared;
  }

  async loadSettings() {
    const own = await readJson(path.join(this.privateDir, 'settings.json'), { ui: { ...DEFAULT_UI } });
    if (!this.open().ok) return { ...own, libraries: [], favorites: [], recent: [], usage: {}, tags: {}, collections: [], mrm: this.status() };
    return { ...own, ...this._sharedSettings() };
  }

  // ------------------------------------------------------------------ ghi

  _tagId(db, name) {
    const found = db.prepare(
      `SELECT id FROM tags WHERE name = ? COLLATE NOCASE
       ORDER BY CASE kind WHEN 'function' THEN 0 WHEN 'personal' THEN 1 WHEN 'type' THEN 2 ELSE 3 END LIMIT 1`
    ).get(name);
    if (found) return found.id;
    return Number(db.prepare("INSERT INTO tags (kind, name) VALUES ('function', ?)").run(name).lastInsertRowid);
  }

  /** Lưu settings theo kiểu cũ: tự tính phần thay đổi và ghi vào DB chung; phần còn lại lưu riêng. */
  async saveSettings(next) {
    const own = {};
    for (const [k, v] of Object.entries(next || {})) if (!SHARED_KEYS.has(k) && k !== 'mrm') own[k] = v;
    await writeJson(path.join(this.privateDir, 'settings.json'), own);
    if (!this.open().ok) return next;
    const pick = o => ({ libraries: o.libraries, favorites: o.favorites, recent: o.recent, usage: o.usage, tags: o.tags, collections: o.collections });
    const incoming = pick(next);
    if (this._lastShared && JSON.stringify({ ...JSON.parse(this._lastShared), ...Object.fromEntries(Object.entries(incoming).filter(([, v]) => v !== undefined)) }) === this._lastShared) {
      // chỉ UI/thiết lập riêng thay đổi -> không đụng DB chung
      return { ...own, ...JSON.parse(this._lastShared) };
    }
    const before = this._sharedSettings();
    const idOf = p => this.assetId(p);
    const now = Date.now();
    this.write(db => {
      // favorites
      if (Array.isArray(next.favorites)) {
        const oldSet = new Set(before.favorites.map(key)), newSet = new Set(next.favorites.map(key));
        const ins = db.prepare('INSERT OR IGNORE INTO asset_favorites (asset_id, created_at) VALUES (?, ?)');
        const del = db.prepare('DELETE FROM asset_favorites WHERE asset_id = ?');
        for (const p of next.favorites) if (!oldSet.has(key(p))) { const id = idOf(p); if (id) ins.run(id, Math.floor(now / 1000)); }
        for (const p of before.favorites) if (!newSet.has(key(p))) { const id = idOf(p); if (id) del.run(id); }
      }
      // tags
      if (next.tags && typeof next.tags === 'object') {
        const paths = new Set([...Object.keys(before.tags), ...Object.keys(next.tags)]);
        const addTag = db.prepare('INSERT OR IGNORE INTO asset_tags (asset_id, tag_id) VALUES (?, ?)');
        const confirm = db.prepare('UPDATE asset_tags SET source = NULL, reason = NULL WHERE asset_id = ? AND tag_id = ?');
        const delTag = db.prepare('DELETE FROM asset_tags WHERE asset_id = ? AND tag_id IN (SELECT id FROM tags WHERE name = ? COLLATE NOCASE)');
        for (const p of paths) {
          const id = idOf(p); if (!id) continue;
          const a = (before.tags[p] || []).map(s => s.toLowerCase()), b = next.tags[p] || [];
          const bl = b.map(s => s.toLowerCase());
          for (const t of b) if (!a.includes(t.toLowerCase())) { const tid = this._tagId(db, t); addTag.run(id, tid); confirm.run(id, tid); }
          for (const t of before.tags[p] || []) if (!bl.includes(t.toLowerCase())) delTag.run(id, t);
        }
      }
      // recent + usage
      const upsertRecent = db.prepare(
        `INSERT INTO asset_usage (asset_id, last_used_at, use_count) VALUES (?, ?, 0)
         ON CONFLICT(asset_id) DO UPDATE SET last_used_at = excluded.last_used_at`
      );
      if (Array.isArray(next.recent)) {
        const oldPos = new Map(before.recent.map((p, i) => [key(p), i]));
        next.recent.forEach((p, i) => { if (oldPos.get(key(p)) !== i) { const id = idOf(p); if (id) upsertRecent.run(id, Math.floor(now / 1000) - i); } });
        const keep = new Set(next.recent.map(key));
        const clear = db.prepare('UPDATE asset_usage SET last_used_at = 0 WHERE asset_id = ?');
        for (const p of before.recent) if (!keep.has(key(p))) { const id = idOf(p); if (id) clear.run(id); }
      }
      if (next.usage && typeof next.usage === 'object') {
        const setCount = db.prepare(
          `INSERT INTO asset_usage (asset_id, last_used_at, use_count) VALUES (?, 0, ?)
           ON CONFLICT(asset_id) DO UPDATE SET use_count = excluded.use_count`
        );
        for (const [p, n] of Object.entries(next.usage)) if ((before.usage[p] || 0) !== n && Number.isFinite(n)) { const id = idOf(p); if (id) setCount.run(id, Math.max(0, Math.floor(n))); }
      }
      // smart collections
      if (Array.isArray(next.collections) && JSON.stringify(next.collections) !== JSON.stringify(before.collections)) {
        db.exec('DELETE FROM asset_smart_collections');
        const ins = db.prepare('INSERT INTO asset_smart_collections (id, name, filters, position, updated_at) VALUES (?, ?, ?, ?, ?)');
        next.collections.forEach((c, i) => { if (c && c.id && c.name) ins.run(String(c.id), String(c.name), JSON.stringify(c.filters || {}), i, Math.floor(now / 1000)); });
      }
    });
    return this.loadSettings();
  }

  saveDuration(p, duration) {
    const id = this.assetId(p);
    if (!id || !Number.isFinite(duration)) return false;
    this.write(db => db.prepare('UPDATE media_assets SET duration = ? WHERE id = ? AND duration IS NULL').run(duration, id));
    const f = this._index?.files.find(x => x.assetId === id);
    if (f && !Number.isFinite(f.duration)) f.duration = duration;
    return true;
  }

  /** Thêm library vào MRM và yêu cầu MRM quét (plugin không tự quét). */
  addLibrary(dir, types) {
    this.write(db => {
      const exists = db.prepare('SELECT id FROM libraries WHERE lower(path) = lower(?)').get(dir);
      if (exists) db.prepare('UPDATE libraries SET media_types = ? WHERE id = ?').run(JSON.stringify(types), exists.id);
      else db.prepare('INSERT INTO libraries (name, path, scan_depth, created_at, media_types) VALUES (?, ?, 0, ?, ?)').run(path.basename(dir) || dir, dir, Math.floor(Date.now() / 1000), JSON.stringify(types));
    });
    return this.requestRescan();
  }

  setLibraryTypes(dir, types) {
    this.write(db => db.prepare('UPDATE libraries SET media_types = ? WHERE lower(path) = lower(?)').run(JSON.stringify(types), dir));
    return this.requestRescan();
  }

  requestRescan() {
    return this.write(db => Number(db.prepare("INSERT INTO app_requests (app, kind, created_at) VALUES (?, 'rescan', ?)").run(APP, Math.floor(Date.now() / 1000)).lastInsertRowid));
  }

  /** MRM có đang chạy không (MRM ghi nhịp tim 'mrm'). */
  mrmRunning() {
    if (!this.open().ok) return false;
    const r = this.db.prepare("SELECT last_seen FROM app_presence WHERE app = 'mrm'").get();
    return !!r && Date.now() / 1000 - r.last_seen <= 15;
  }

  /** Chờ MRM xử lý yêu cầu (tối đa timeoutMs). */
  async waitRequest(id, timeoutMs = 20000) {
    const end = Date.now() + timeoutMs;
    while (Date.now() < end) {
      const r = this.db.prepare('SELECT handled_at, result FROM app_requests WHERE id = ?').get(id);
      if (r && r.handled_at) { this._index = null; this._indexVersion = null; return { done: true, result: r.result }; }
      await new Promise(res => setTimeout(res, 500));
    }
    return { done: false };
  }

  // ------------------------------------------------------------------ nhịp tim

  beat(detail) {
    try {
      this.write(db => db.prepare(
        `INSERT INTO app_presence (app, pid, version, detail, last_seen) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(app) DO UPDATE SET pid = excluded.pid, version = excluded.version, detail = excluded.detail, last_seen = excluded.last_seen`
      ).run(APP, process.pid, this.version, detail || 'DaVinci Resolve', Math.floor(Date.now() / 1000)), 0);
    } catch { /* MRM đang ghi — bỏ qua nhịp này, không chặn giao diện */ }
  }
  startHeartbeat(detail) {
    if (!this.open().ok || this._heartbeat) return;
    this.beat(detail);
    this._heartbeat = setInterval(() => this.beat(detail), 5000);
    this._heartbeat.unref?.();
  }
  stopHeartbeat() {
    if (this._heartbeat) clearInterval(this._heartbeat);
    this._heartbeat = null;
    try { this.db?.prepare('DELETE FROM app_presence WHERE app = ?').run(APP); } catch {}
  }

  // ------------------------------------------------------------------ chuyển dữ liệu từ v0.10.2

  /**
   * Chuyển favorites/tags/recent/usage/collections/UI của v0.10.2 sang DB chung (một lần, gộp — không ghi đè).
   * File cũ giữ nguyên để có thể quay lại.
   */
  async migrateLegacy() {
    // 1. dữ liệu HNH SFX Finder 0.10.2 gốc; 2. dữ liệu plugin tự lưu khi chạy độc lập (trước khi cài MRM)
    const sources = [
      [this.legacyDir, 'migrated-from-0.10.2.json'],
      [this.standaloneDir, 'migrated-from-standalone.json'],
    ].filter(([dir]) => dir);
    let result = null;
    for (const [dir, marker] of sources) result = (await this._migrateFrom(dir, marker)) || result;
    return result;
  }

  /**
   * Thư viện của plugin chưa có trong MRM -> thêm vào MRM (MRM sẽ quét).
   * Bỏ qua thư mục đã nằm trong / chứa một library của MRM để không lập chỉ mục trùng.
   */
  _importLibraries(libraries) {
    const norm2 = p => path.resolve(String(p)).toLowerCase().replace(/[\\/]+$/, '');
    const have = this.db.prepare('SELECT path FROM libraries').all().map(r => norm2(r.path));
    const inside = (a, b) => a === b || a.startsWith(b + path.sep);
    let added = 0;
    for (const lib of libraries || []) {
      if (!lib || !lib.path || lib.enabled === false) continue;
      const p = norm2(lib.path);
      if (have.some(h => inside(p, h) || inside(h, p))) continue;
      if (!fs.existsSync(lib.path)) continue;
      this.addLibrary(lib.path, Array.isArray(lib.types) && lib.types.length ? lib.types : ['audio', 'image', 'video']);
      have.push(p);
      added++;
    }
    return added;
  }

  async _migrateFrom(dir, markerName) {
    const marker = path.join(this.privateDir, markerName);
    const prev = await readJson(marker, null);
    // đã xong, hoặc còn file chưa khớp nhưng MRM chưa quét gì mới kể từ lần thử trước
    if (prev && (!prev.retry || prev.rev === this.contentVersion())) return null;
    const legacy = await readJson(path.join(dir, 'settings.json'), null);
    if (!legacy) { await writeJson(marker, { at: new Date().toISOString(), note: 'no legacy data' }); return null; }
    if (!this.open().ok) return null; // thử lại lần sau khi MRM sẵn sàng
    // thư viện trước (MRM cần biết thư mục mới quét được), rồi mới chuyển tag/yêu thích
    try { this._importLibraries(legacy.libraries); } catch { /* MRM đang ghi — lần sau */ }
    // MRM chưa lập chỉ mục (vd: vừa cài MRM, vừa khôi phục DB) -> chưa chuyển, đợi lần quét sau
    if (!this.loadIndex().files.length) return null;
    const current = this._sharedSettings();
    const unmatched = new Set();
    const known = p => { const ok = !!this.assetId(p); if (!ok) unmatched.add(p); return ok; };
    const favorites = [...new Set([...current.favorites, ...(legacy.favorites || []).filter(known)])];
    const tags = { ...current.tags };
    for (const [p, list] of Object.entries(legacy.tags || {})) {
      if (!known(p) || !Array.isArray(list)) continue;
      const have = new Map((tags[p] || []).map(t => [t.toLowerCase(), t]));
      for (const t of list) if (typeof t === 'string' && t.trim()) have.set(t.toLowerCase(), have.get(t.toLowerCase()) || t.trim());
      tags[p] = [...have.values()];
    }
    const recent = [...new Set([...(legacy.recent || []).filter(known), ...current.recent])].slice(0, 100);
    const usage = { ...current.usage };
    for (const [p, n] of Object.entries(legacy.usage || {})) if (known(p)) usage[p] = Math.max(usage[p] || 0, Number(n) || 0);
    const ids = new Set(current.collections.map(c => c.id));
    const collections = [...current.collections, ...(legacy.collections || []).filter(c => c && c.id && !ids.has(c.id))];
    const own = await readJson(path.join(this.privateDir, 'settings.json'), null);
    await this.saveSettings({ ...(own || {}), ui: { ...DEFAULT_UI, ...(legacy.ui || {}), ...(own?.ui || {}) }, favorites, tags, recent, usage, collections });
    const summary = {
      at: new Date().toISOString(),
      favorites: (legacy.favorites || []).length,
      tags: Object.keys(legacy.tags || {}).length,
      recent: (legacy.recent || []).length,
      collections: (legacy.collections || []).length,
      unmatched: [...unmatched].slice(0, 200),
      // còn file chưa khớp (thư viện chưa quét tới) -> thử lại khi MRM quét xong lần sau
      retry: unmatched.size > 0,
      rev: this.contentVersion(),
    };
    await writeJson(marker, summary);
    return summary;
  }
}

module.exports = { MrmStore, defaultDbPath, CONTRACT, APP, norm, key };
