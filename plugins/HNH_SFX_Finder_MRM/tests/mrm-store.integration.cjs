// Test tích hợp MrmStore với DB MRM thật (chạy bằng Electron của Resolve ở chế độ Node):
//   set ELECTRON_RUN_AS_NODE=1 & set MRM_DB=<fixture.db> & set OUT=<result.json>
//   "C:\Program Files\Blackmagic Design\DaVinci Resolve\Electron\electron.exe" tests\mrm-store.integration.cjs
'use strict';
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { MrmStore } = require('../mrm-store');

// Chỉ chạy trên DB mẫu — tuyệt đối không dùng DB thật của MRM
if (!process.env.MRM_DB || /com\.nink\.vault/i.test(process.env.MRM_DB)) {
  console.error('Cần MRM_DB trỏ tới DB mẫu (không phải DB thật trong %APPDATA%\com.nink.vault).');
  process.exit(2);
}

const results = [];
const check = (name, ok, detail) => results.push({ name, ok: !!ok, detail });

(async () => {
  try {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'hnh-mrm-'));
    const legacyDir = path.join(tmp, 'legacy'), privateDir = path.join(tmp, 'private');
    fs.mkdirSync(legacyDir, { recursive: true });

    const store = new MrmStore({ privateDir, legacyDir, version: '0.11.0-test', dbPath: process.env.MRM_DB });
    const st = store.open();
    check('open shared DB', st.ok, st.error);

    const index = store.loadIndex();
    const wav = index.files.find(f => f.extension === 'wav');
    const seq = index.files.find(f => f.sequence);
    check('index has assets', index.files.length > 0, index.files.length);
    check('file shape compatible', wav && wav.id === wav.path.toLowerCase() && Array.isArray(wav.folders) && typeof wav.searchText === 'string', wav && Object.keys(wav));
    check('parent resource linked', wav && wav.resource && wav.resource.name, wav && wav.resource);
    check('image sequence exposed', seq && /%0\dd/.test(seq.sequence.pattern) && store.sequenceFor(seq.path), seq && seq.sequence);

    // ---- chuyển dữ liệu v0.10.2
    const other = index.files.find(f => f.extension === 'wav' && f !== wav) || wav;
    fs.writeFileSync(path.join(legacyDir, 'settings.json'), JSON.stringify({
      libraries: [{ path: wav.root }],
      favorites: [wav.path, 'D:\\khong-ton-tai\\x.wav'],
      recent: [other.path, wav.path],
      usage: { [wav.path]: 5 },
      tags: { [wav.path]: ['gun', 'Whoosh Fast'] },
      collections: [{ id: 'c1', name: 'Whoosh nhanh', filters: { query: 'whoosh', extension: 'wav' } }],
      ui: { autoPreview: false, trackIndex: 3 },
    }));
    const mig = await store.migrateLegacy();
    check('legacy migrated', mig && mig.favorites === 2 && mig.unmatched.length === 1, mig);
    const again = await store.migrateLegacy();
    check('migration runs once', again === null, again);
    // MRM quét thêm (media_rev đổi) -> thử lại phần chưa khớp, gộp không nhân đôi
    store.db.exec("INSERT INTO settings (key, value) VALUES ('media_rev', '1') ON CONFLICT(key) DO UPDATE SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)");
    const retry = await store.migrateLegacy();
    const favAfter = (await store.loadSettings()).favorites.filter(f => f === wav.path).length;
    check('migration retries after rescan', retry && retry.retry === true && favAfter === 1, { retry, favAfter });
    check('no retry without new scan', (await store.migrateLegacy()) === null);

    // ---- thư mục yêu thích
    const favFolder = { root: wav.root, path: wav.relativePath.split(/[\\/]/).slice(0, -1).join('/'), name: 'SFX ghim' };
    // MRM cũ (chưa có bảng favorite_folders) -> lưu riêng trong plugin
    store._favFoldersTable = undefined;
    store.db.exec('DROP TABLE IF EXISTS favorite_folders');
    let sf = await store.saveSettings({ ...(await store.loadSettings()), favoriteFolders: [favFolder] });
    const privateSaved = JSON.parse(fs.readFileSync(path.join(privateDir, 'settings.json'), 'utf8'));
    check('fav folders private when MRM is old', (privateSaved.favoriteFolders || []).length === 1 && (await store.loadSettings()).favoriteFolders.length === 1, privateSaved.favoriteFolders);
    // MRM >= 0.11.0 -> dùng chung bảng favorite_folders
    store.db.exec(`CREATE TABLE favorite_folders (id INTEGER PRIMARY KEY, library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
                   path TEXT NOT NULL, name TEXT NOT NULL, created_at INTEGER NOT NULL, UNIQUE (library_id, path))`);
    store._favFoldersTable = undefined;
    sf = await store.saveSettings({ ...(await store.loadSettings()), favoriteFolders: [favFolder] });
    const rows = store.db.prepare('SELECT path, name FROM favorite_folders').all();
    check('fav folder written to shared table', rows.length === 1 && rows[0].name === 'SFX ghim' && !rows[0].path.includes('/'), rows);
    const back = (await store.loadSettings()).favoriteFolders;
    check('fav folder read back with / separators', back.length === 1 && back[0].path === favFolder.path && back[0].root === favFolder.root, back);
    await store.saveSettings({ ...(await store.loadSettings()), favoriteFolders: [] });
    check('fav folder removed', store.db.prepare('SELECT count(*) AS n FROM favorite_folders').get().n === 0);

    // ---- plugin từng chạy độc lập (chưa có MRM) rồi người dùng cài MRM
    const standaloneDir = path.join(tmp, 'standalone');
    const newLib = path.join(tmp, 'SFX moi');
    fs.mkdirSync(newLib, { recursive: true });
    const third = index.files.find(f => f.extension === 'wav' && f.path !== wav.path && f.path !== other.path) || other;
    fs.mkdirSync(standaloneDir, { recursive: true });
    fs.writeFileSync(path.join(standaloneDir, 'settings.json'), JSON.stringify({
      libraries: [{ path: newLib, enabled: true, types: ['audio'] }, { path: path.join(wav.root, 'con'), enabled: true }],
      favorites: [third.path], recent: [], usage: {}, tags: { [third.path]: ['tu doc lap'] }, ui: {},
    }));
    const libsBefore = store.db.prepare('SELECT count(*) AS n FROM libraries').get().n;
    const store2 = new MrmStore({ privateDir, standaloneDir, version: 'test', dbPath: process.env.MRM_DB });
    store2.open();
    const mig2 = await store2.migrateLegacy();
    const libsAfter = store2.db.prepare('SELECT path, media_types FROM libraries').all();
    check('standalone library imported, nested one skipped', libsAfter.length === libsBefore + 1 && libsAfter.some(l => l.path === newLib), libsAfter);
    const s2 = await store2.loadSettings();
    check('standalone tags + favorites migrated', mig2 && s2.favorites.includes(third.path) && (s2.tags[third.path] || []).includes('tu doc lap'), { mig2, fav: s2.favorites });
    check('standalone migration runs once', (await store2.migrateLegacy()) === null);
    store2.close();

    let s = await store.loadSettings();
    check('favorite shared', s.favorites.includes(wav.path), s.favorites);
    check('tags shared', (s.tags[wav.path] || []).includes('gun') && (s.tags[wav.path] || []).includes('Whoosh Fast'), s.tags[wav.path]);
    check('recent order kept', s.recent[0] === other.path && s.recent[1] === wav.path, s.recent.slice(0, 2));
    check('usage kept', s.usage[wav.path] === 5, s.usage[wav.path]);
    check('smart collection shared', s.collections.length === 1 && s.collections[0].filters.query === 'whoosh', s.collections);
    check('plugin-only UI kept private', s.ui.trackIndex === 3 && fs.existsSync(path.join(privateDir, 'settings.json')), s.ui);

    // ---- sửa từ plugin (giống UI gọi updateSettings)
    s = await store.saveSettings({ ...s, favorites: [], tags: { ...s.tags, [wav.path]: ['gun', 'Impact'] }, recent: [wav.path, other.path] });
    check('unfavorite', !s.favorites.includes(wav.path), s.favorites);
    check('tag replaced', JSON.stringify((s.tags[wav.path] || []).sort()) === JSON.stringify(['Impact', 'gun']), s.tags[wav.path]);
    check('recent reordered', s.recent[0] === wav.path, s.recent.slice(0, 2));
    s = await store.saveSettings({ ...s, favorites: [wav.path] });
    check('favorite again', s.favorites.includes(wav.path), s.favorites);

    // ---- nhịp tim + yêu cầu quét lại
    store.startHeartbeat('test');
    const beat = store.db.prepare("SELECT version FROM app_presence WHERE app = 'hnh-sfx-finder'").get();
    check('heartbeat written', beat && beat.version === '0.11.0-test', beat);
    const rq = store.requestRescan();
    check('rescan request queued', rq > 0, rq);
    const dv = store.dataVersion();
    check('data_version readable', Number.isInteger(dv), dv);

    // để bên MRM (python/rust) kiểm tra đọc chéo
    results.push({ name: 'probe', ok: true, detail: { wavPath: wav.path, assetId: wav.assetId } });
    store.stopHeartbeat();
    const gone = store.db.prepare("SELECT 1 FROM app_presence WHERE app = 'hnh-sfx-finder'").get();
    check('heartbeat cleared on close', !gone, gone);
    store.close();
  } catch (e) {
    check('exception', false, String(e && e.stack || e));
  }
  fs.writeFileSync(process.env.OUT || 'mrm-store-result.json', JSON.stringify({ node: process.versions.node, results }, null, 2));
})();
