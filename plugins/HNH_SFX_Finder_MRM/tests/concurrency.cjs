// Ghi đồng thời từ Electron của Resolve (node:sqlite) để kiểm tra phối hợp khóa với MRM (rusqlite)
'use strict';
const fs = require('node:fs');
const { DatabaseSync } = require('node:sqlite');
const db = new DatabaseSync(process.env.MRM_CONC_DB);
db.exec('PRAGMA busy_timeout = 5000');
const out = { writes: 0, errors: [], sqlite: db.prepare('select sqlite_version() v').get().v, journal: db.prepare('pragma journal_mode').get().journal_mode };
const end = Date.now() + Number(process.env.MRM_CONC_SECS || 20) * 1000;
function tick() {
  if (Date.now() > end) {
    try { out.count = db.prepare('select count(*) c from media_assets').get().c; out.integrity = db.prepare('pragma integrity_check').get().integrity_check; } catch (e) { out.errors.push('final ' + e.message); }
    db.close();
    fs.writeFileSync(process.env.OUT, JSON.stringify(out, null, 2));
    return;
  }
  try {
    db.exec('BEGIN IMMEDIATE');
    db.prepare("INSERT INTO app_presence (app, last_seen) VALUES ('hnh-sfx-finder', ?) ON CONFLICT(app) DO UPDATE SET last_seen = excluded.last_seen").run(Date.now());
    db.prepare('INSERT OR REPLACE INTO asset_smart_collections (id, name, filters, updated_at) VALUES (?, ?, ?, ?)').run('c' + (out.writes % 50), 'n', '{}', Date.now());
    db.exec('COMMIT');
    out.writes++;
    db.prepare('select count(*) c from media_assets').get();
  } catch (e) { try { db.exec('ROLLBACK'); } catch {} if (out.errors.length < 20) out.errors.push(e.message); }
  setTimeout(tick, 25);
}
tick();
