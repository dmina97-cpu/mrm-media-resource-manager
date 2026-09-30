const { app, BrowserWindow, ipcMain, dialog } = require('electron');
const path = require('path');
const fs = require('fs');
const fsp = fs.promises;

// Resolve chỉ chấp nhận Initialize() với đúng <Id> trong manifest.xml -> đọc từ manifest
const PLUGIN_ID = (() => {
  try { return /<Id>\s*([^<\s]+)\s*<\/Id>/.exec(require('fs').readFileSync(require('path').join(__dirname, 'manifest.xml'), 'utf8'))[1]; }
  catch { return 'com.hnh.resolve.sfxfinder.mrm'; }
})();
const AUDIO_EXTS = new Set(['.wav', '.mp3', '.flac', '.aiff', '.aif', '.m4a', '.aac', '.ogg']);
const INDEX_VERSION = 3;
const library = require('./library');
const PLUGIN_VERSION = '0.12.2';
const { Portable } = require('./portable');

let WorkflowIntegration = null;
let resolveObj = null;
let initialized = false;
let mainWindow = null;
let thumbnailDecoder;
let bridgeStatus = { ok: false, error: 'NOT_INITIALIZED', detail: '', modulePath: null, moduleInfo: null };
let bridgeInitPromise = null;

// Thư mục riêng của plugin (không đụng dữ liệu v0.10.2 gốc)
const userDataDir = path.join(app.getPath('userData'), 'HNH-SFX-Finder-MRM');
const legacyDir = path.join(app.getPath('userData'), 'HNH-SFX-Finder');
// Chế độ độc lập (máy không có MRM): tự quét + lưu như bản gốc, trong thư mục con riêng
const standaloneDir = path.join(userDataDir, 'standalone');
const settingsPath = path.join(standaloneDir, 'settings.json');
const indexPath = path.join(standaloneDir, 'sfx-index.json');

/**
 * Plugin lai:
 *  - 'mrm'        : máy có MRM -> dùng chung dữ liệu với MRM (MRM quét, plugin đọc/ghi tag, yêu thích…).
 *  - 'standalone' : không có MRM -> chạy độc lập như HNH SFX Finder gốc (tự quét thư mục).
 * Khi người dùng cài MRM sau này, dữ liệu chế độ độc lập (thư viện, tag, yêu thích) tự chuyển sang MRM.
 */
let MODE = 'standalone';
const isMrm = () => MODE === 'mrm';

async function ensureDataDir() {
  await fsp.mkdir(userDataDir, { recursive: true });
}

async function readJson(file, fallback) {
  try { return JSON.parse(await fsp.readFile(file, 'utf8')); }
  catch { return fallback; }
}

async function writeJson(file, data) {
  await ensureDataDir();
  await fsp.mkdir(path.dirname(file), { recursive: true });
  const tmp = file + '.' + require('crypto').randomUUID() + '.tmp';
  await fsp.writeFile(tmp, JSON.stringify(data, null, 2), 'utf8');
  await fsp.rename(tmp, file);
}

function isIgnoredMacMetadata(name = '') {
  // macOS AppleDouble sidecar files copied to Windows/Linux. They are metadata, not playable audio.
  return String(name).startsWith('._');
}

let operation = Promise.resolve();
function exclusive(fn) { const next = operation.then(fn); operation = next.catch(()=>{}); return next; }
// ===== MRM Shared Core: dữ liệu dùng chung với MRM (Media Resource Manager) =====
const { MrmStore } = require('./mrm-store');
const store = new MrmStore({ privateDir: userDataDir, legacyDir, standaloneDir, version: PLUGIN_VERSION });

// ---- chế độ độc lập: y như bản gốc
const saWatcher = new library.Watcher(()=>exclusive(async()=>{
  const settings = await loadSettings();
  const scan = await rebuildIndex(settings.libraries || []);
  if (scan.added || scan.changed || scan.removed || scan.warnings.length) mainWindow?.webContents.send('library:changed', {index:await loadIndex(),scan});
}));
async function saRebuildIndex(libraries) {
  const started=Date.now(), index=await library.scan(libraries,await loadIndex());
  await writeJson(indexPath,index);
  return {count:index.files.length,ms:Date.now()-started,...index.stats,warnings:index.warnings};
}
async function saLoadSettings() {
  return readJson(settingsPath, { libraries: [], favorites: [], recent: [], usage: {}, ui: { autoPreview: true, sortMode: 'relevance', extension: 'all', library: 'all', trackIndex: 1 } });
}
async function saSaveSettings(settings) {
  await writeJson(settingsPath, settings);
  return settings;
}
async function saLoadIndex() {
  return readJson(indexPath, { version: INDEX_VERSION, generatedAt: null, files: [] });
}

// ---- chế độ MRM: MRM là nơi duy nhất lập chỉ mục. Plugin chỉ theo dõi thay đổi trong DB chung.
const mrmWatcher = {
  timer: null, last: null,
  start() {
    if (this.timer) return;
    this.last = store.contentVersion();
    this.timer = setInterval(() => {
      try {
        const v = store.contentVersion();
        if (v !== null && v !== this.last) {
          this.last = v;
          const index = store.loadIndex();
          const scan = { count: index.files.length, added: 0, changed: 0, removed: 0, warnings: [] };
          // MRM vừa quét xong -> thử chuyển nốt dữ liệu v0.10.2 chưa khớp, rồi gửi kèm settings mới
          store.migrateLegacy()
            .then(done => (done ? store.loadSettings() : null))
            .catch(() => null)
            .then(settings => mainWindow?.webContents.send('library:changed', { index, scan, settings }));
        }
      } catch { /* MRM đang ghi — lần sau */ }
    }, 2000);
    this.timer.unref?.();
  },
  close() { clearInterval(this.timer); this.timer = null; },
};
async function mrmRebuildIndex() {
  const started = Date.now();
  if (!store.open().ok) return { count: 0, ms: 0, added: 0, changed: 0, removed: 0, warnings: [store.error] };
  const warnings = [];
  const id = store.requestRescan();
  if (store.mrmRunning()) {
    const r = await store.waitRequest(id, 30000);
    if (!r.done) warnings.push('MRM đang bận — thư viện sẽ tự cập nhật khi MRM quét xong.');
  } else warnings.push('MRM chưa chạy — mở MRM để cập nhật thư viện (yêu cầu đã được lưu).');
  const index = store.loadIndex();
  return { count: index.files.length, ms: Date.now() - started, added: 0, changed: 0, removed: 0, unchanged: index.files.length, warnings };
}
// ---- điều phối theo chế độ
const watcher = {
  start(libraries) { (isMrm() ? mrmWatcher : saWatcher).start(libraries || []); },
  close() { mrmWatcher.close(); saWatcher.close(); },
};
async function rebuildIndex(libraries) { return isMrm() ? mrmRebuildIndex() : saRebuildIndex(libraries || []); }
async function loadSettings() { return isMrm() ? store.loadSettings() : saLoadSettings(); }
async function saveSettings(settings) { return isMrm() ? store.saveSettings(settings) : saSaveSettings(settings); }
async function loadIndex() { return isMrm() ? store.loadIndex() : saLoadIndex(); }

/** Chọn chế độ một lần khi khởi động. */
let modePromise = null;
function initMode() {
  return modePromise ||= (async () => {
    await ensureDataDir();
    await copyLegacyCache();
    const opened = store.open();
    if (opened.ok) {
      MODE = 'mrm';
      try { await store.migrateLegacy(); } catch (e) { console.error('migrate', e); }
      store.startHeartbeat('DaVinci Resolve');
    } else {
      MODE = 'standalone';
      // lần đầu chạy độc lập: lấy luôn dữ liệu của HNH SFX Finder 0.10.2 gốc (chỉ sao chép)
      if (!fs.existsSync(settingsPath)) {
        for (const name of ['settings.json', 'sfx-index.json']) {
          try { if (fs.existsSync(path.join(legacyDir, name))) { await fsp.mkdir(standaloneDir, { recursive: true }); await fsp.copyFile(path.join(legacyDir, name), path.join(standaloneDir, name)); } } catch {}
        }
      }
    }
    portable = new Portable(isMrm() ? userDataDir : standaloneDir, {loadSettings,loadIndex,progress:data=>send('backup:progress',data)});
    await portable.recover();
    return MODE;
  })();
}
function modeNotices() {
  if (isMrm()) return store.status().ok ? ['Đang dùng chung dữ liệu với MRM (MRM quét thư viện).'] : [store.error];
  // có file MRM nhưng không mở được (vd: khác phiên bản) -> nói rõ vì sao đang chạy độc lập
  return [fs.existsSync(store.dbPath) && store.error ? `${store.error} — plugin đang chạy độc lập.` : 'Chế độ độc lập (máy chưa cài MRM).'];
}
async function copyLegacyCache() {
  // tái dùng cache waveform/thumbnail của v0.10.2 (chỉ sao chép, không xóa bản cũ)
  for (const name of ['waveforms-v1', 'thumbnails-v1']) {
    const from = path.join(app.getPath('userData'), 'HNH-SFX-Finder', name), to = path.join(userDataDir, name);
    try { if (fs.existsSync(from) && !fs.existsSync(to)) await fsp.cp(from, to, { recursive: true }); } catch {}
  }
}

function indexNeedsRebuild(index) {
  if (isMrm()) return false; // MRM quản lý index

  if (!index || index.version !== INDEX_VERSION) return true;
  // Also clean indexes created by older versions without forcing the user to manually rescan.
  return Array.isArray(index.files) && index.files.some(file => isIgnoredMacMetadata(file?.filename));
}

async function getCurrentIndex(settings) {
  let index = await loadIndex();
  if (indexNeedsRebuild(index)) {
    await rebuildIndex(settings.libraries || []);
    index = await loadIndex();
  }
  return index;
}

function workflowIntegrationCandidates() {
  const programData = process.env.PROGRAMDATA || 'C:\\ProgramData';
  const supportRoot = path.join(programData, 'Blackmagic Design', 'DaVinci Resolve', 'Support');
  const examplesRoot = path.join(supportRoot, 'Developer', 'Workflow Integrations', 'Examples');

  // v0.1.3: Prefer the exact Resolve 20.2 WorkflowIntegration.node bundled with this build.
  // The user supplied this module directly from their Resolve 20.2 Developer package.
  // Only fall back to Developer examples if the bundled bridge cannot be loaded.
  return [
    path.join(__dirname, 'WorkflowIntegration.node'),
    path.join(examplesRoot, 'SamplePlugin', 'WorkflowIntegration.node'),
    path.join(examplesRoot, 'SamplePromisePlugin', 'WorkflowIntegration.node'),
    path.join(examplesRoot, 'CompatibleSamplePlugin', 'WorkflowIntegration.node')
  ];
}

function friendlyBridgeLoadError(errors) {
  if (!errors.length) return 'WorkflowIntegration.node not found';
  const text = errors.map(x => `${x.path}: ${x.error}`).join(' | ');
  if (/NODE_MODULE_VERSION|compiled against a different Node\.js version|was compiled against/i.test(text)) {
    return `Native module ABI mismatch. ${text}`;
  }
  if (/The specified module could not be found|Cannot find module/i.test(text)) {
    return `WorkflowIntegration.node or one of its native dependencies could not be loaded. ${text}`;
  }
  return text;
}

async function initializeResolve(force = false) {
  if (!force && resolveObj && initialized) return bridgeStatus;
  if (bridgeInitPromise) return bridgeInitPromise;

  if (force && initialized && WorkflowIntegration) {
    try { if (typeof WorkflowIntegration.CleanUp === 'function') WorkflowIntegration.CleanUp(); } catch {}
    initialized = false;
    resolveObj = null;
    WorkflowIntegration = null;
  }

  bridgeInitPromise = (async () => {
    const attempts = [];
    const candidates = [...new Set(workflowIntegrationCandidates())];
    const runtime = {
      electron: process.versions?.electron || null,
      node: process.versions?.node || null,
      chrome: process.versions?.chrome || null,
      napi: process.versions?.napi || null,
      platform: process.platform,
      arch: process.arch
    };

    for (const candidate of candidates) {
      if (!fs.existsSync(candidate)) {
        attempts.push({ path: candidate, stage: 'exists', ok: false, error: 'File not found' });
        continue;
      }

      let mod = null;
      let info = null;
      try {
        // Remove any stale require cache before a manual retry.
        try { delete require.cache[require.resolve(candidate)]; } catch {}
        mod = require(candidate);
        try { info = typeof mod.GetInfo === 'function' ? mod.GetInfo() : null; } catch (e) {
          attempts.push({ path: candidate, stage: 'GetInfo', ok: false, error: String(e?.message || e) });
        }
      } catch (error) {
        attempts.push({ path: candidate, stage: 'require', ok: false, error: String(error?.stack || error?.message || error) });
        continue;
      }

      try {
        if (typeof mod.SetAPITimeout === 'function') {
          try { mod.SetAPITimeout(8); } catch {}
        }

        // v0.1.3: use the synchronous API first. This is the canonical API used by
        // Blackmagic's SamplePlugin and avoids depending on promise-proxy startup.
        let didInit = false;
        let initMode = null;
        if (typeof mod.Initialize === 'function') {
          initMode = 'sync';
          didInit = !!mod.Initialize(PLUGIN_ID);
        } else if (typeof mod.InitializePromise === 'function') {
          initMode = 'promise';
          didInit = !!(await mod.InitializePromise(PLUGIN_ID));
        } else {
          attempts.push({ path: candidate, stage: 'Initialize', ok: false, error: 'Module exposes neither Initialize nor InitializePromise', info });
          continue;
        }

        if (!didInit) {
          attempts.push({ path: candidate, stage: `Initialize:${initMode}`, ok: false, error: `Initialize('${PLUGIN_ID}') returned false`, info });
          try { if (typeof mod.CleanUp === 'function') mod.CleanUp(); } catch {}
          continue;
        }

        let resolve = null;
        let resolveMode = null;
        if (initMode === 'sync' && typeof mod.GetResolve === 'function') {
          resolveMode = 'sync';
          resolve = mod.GetResolve();
        } else if (typeof mod.GetResolvePromise === 'function') {
          resolveMode = 'promise';
          resolve = await mod.GetResolvePromise();
        } else if (typeof mod.GetResolve === 'function') {
          resolveMode = 'sync';
          resolve = mod.GetResolve();
        }

        if (!resolve) {
          attempts.push({ path: candidate, stage: `GetResolve:${resolveMode || 'none'}`, ok: false, error: 'GetResolve returned no Resolve object', info });
          try { if (typeof mod.CleanUp === 'function') mod.CleanUp(); } catch {}
          continue;
        }

        // Validate that the returned object is alive, not just truthy.
        let productName = null;
        let resolveVersion = null;
        try { if (typeof resolve.GetProductName === 'function') productName = await resolve.GetProductName(); } catch {}
        try { if (typeof resolve.GetVersionString === 'function') resolveVersion = await resolve.GetVersionString(); } catch {}

        WorkflowIntegration = mod;
        resolveObj = resolve;
        initialized = true;
        bridgeStatus = {
          ok: true,
          error: null,
          detail: '',
          modulePath: candidate,
          moduleInfo: info,
          pluginVersion: PLUGIN_VERSION,
          initMode,
          resolveMode,
          resolveVersion,
          productName,
          runtime,
          attempts
        };
        return bridgeStatus;
      } catch (error) {
        attempts.push({ path: candidate, stage: 'bridge', ok: false, error: String(error?.stack || error?.message || error), info });
        try { if (typeof mod?.CleanUp === 'function') mod.CleanUp(); } catch {}
      }
    }

    const anyCandidateExists = candidates.some(x => fs.existsSync(x));
    const compact = attempts.map(a => `${a.stage} @ ${a.path}: ${a.error}`).join(' | ');
    bridgeStatus = {
      ok: false,
      error: anyCandidateExists ? 'RESOLVE_BRIDGE_ERROR' : 'MISSING_WORKFLOW_INTEGRATION_NODE',
      detail: compact || friendlyBridgeLoadError([]),
      modulePath: null,
      moduleInfo: null,
      candidates,
      runtime,
      attempts,
      pluginVersion: PLUGIN_VERSION
    };
    WorkflowIntegration = null;
    resolveObj = null;
    initialized = false;
    return bridgeStatus;
  })();

  try {
    return await bridgeInitPromise;
  } finally {
    bridgeInitPromise = null;
  }
}

async function getResolve() {
  if (resolveObj && initialized) return resolveObj;
  const status = await initializeResolve();
  return status.ok ? resolveObj : null;
}

function bridgeUnavailableResult() {
  return {
    ok: false,
    error: 'Resolve bridge unavailable',
    bridge: bridgeStatus
  };
}

function resultItems(result) {
  if (!result) return [];
  if (Array.isArray(result)) return result.filter(Boolean);
  // Workflow Integration can expose returned Resolve collections as numeric-keyed
  // objects rather than native JS Arrays. Do not rely only on `.length`.
  if (typeof result === 'object') {
    const numeric = Object.keys(result)
      .filter(k => /^\d+$/.test(k))
      .sort((a, b) => Number(a) - Number(b))
      .map(k => result[k])
      .filter(Boolean);
    if (numeric.length) return numeric;
  }
  return [];
}

function normalizedFsPath(p = '') {
  let out = path.resolve(String(p || ''));
  if (process.platform === 'win32') out = out.toLowerCase();
  return out.replace(/[\\/]+$/, '');
}

async function mediaPoolItemPath(item) {
  if (!item) return null;
  const keys = ['File Path', 'FilePath'];
  for (const key of keys) {
    try {
      if (typeof item.GetClipProperty === 'function') {
        const v = await item.GetClipProperty(key);
        if (typeof v === 'string' && v.trim()) return v.trim();
      }
    } catch {}
  }
  try {
    if (typeof item.GetClipProperty === 'function') {
      const props = await item.GetClipProperty();
      if (props && typeof props === 'object') {
        const v = props['File Path'] || props.FilePath || props['FilePath'];
        if (typeof v === 'string' && v.trim()) return v.trim();
      }
    }
  } catch {}
  return null;
}

async function folderClipItems(folder) {
  if (!folder) return [];
  for (const method of ['GetClipList', 'GetClips']) {
    try {
      if (typeof folder[method] === 'function') {
        const r = await folder[method]();
        const items = resultItems(r);
        if (items.length) return items;
        if (r && typeof r === 'object') {
          const vals = Object.values(r).filter(Boolean);
          if (vals.length) return vals;
        }
      }
    } catch {}
  }
  return [];
}

async function folderSubFolders(folder) {
  if (!folder) return [];
  for (const method of ['GetSubFolderList', 'GetSubFolders']) {
    try {
      if (typeof folder[method] === 'function') {
        const r = await folder[method]();
        const items = resultItems(r);
        if (items.length) return items;
        if (r && typeof r === 'object') {
          const vals = Object.values(r).filter(Boolean);
          if (vals.length) return vals;
        }
      }
    } catch {}
  }
  return [];
}

async function findExistingMediaPoolItem(mediaPool, filePath, maxFolders = 2000) {
  const wanted = normalizedFsPath(filePath);
  let root = null;
  try { root = await mediaPool.GetRootFolder(); } catch {}
  if (!root) return null;
  const stack = [root];
  let seen = 0;
  while (stack.length && seen < maxFolders) {
    const folder = stack.pop();
    seen++;
    const clips = await folderClipItems(folder);
    for (const clip of clips) {
      const clipPath = await mediaPoolItemPath(clip);
      if (clipPath && normalizedFsPath(clipPath) === wanted) return clip;
    }
    const subs = await folderSubFolders(folder);
    for (const sub of subs) stack.push(sub);
  }
  return null;
}

async function importSfxItem(resolve, project, filePath) {
  const attempts = [];
  const absPath = path.resolve(String(filePath || ''));

  try {
    const stat = await fsp.stat(absPath);
    if (!stat.isFile()) return { ok: false, error: 'SFX path is not a file', detail: absPath, attempts };
  } catch (e) {
    return { ok: false, error: 'SFX file not found', detail: `${absPath} • ${String(e?.message || e)}`, attempts };
  }

  const mediaPool = project && await project.GetMediaPool();
  if (!mediaPool) return { ok: false, error: 'Media pool unavailable', detail: 'project.GetMediaPool() returned no object', attempts };

  // Reuse an existing MediaPoolItem when the same SFX is already in the project.
  try {
    const existing = await findExistingMediaPoolItem(mediaPool, absPath);
    if (existing) return { ok: true, item: existing, count: 0, reused: true, method: 'existing-media-pool-item', attempts };
  } catch (e) {
    attempts.push({ method: 'find-existing', error: String(e?.message || e) });
  }

  // Preferred path: MediaPool.ImportMedia is the direct scripting API for importing
  // paths into the current Media Pool folder and is more reliable through the
  // Workflow Integration proxy than MediaStorage on some Resolve 20.x builds.
  // Chuỗi khung hình (MRM gom frame_0001…): import dạng image sequence
  const seq = isMrm() ? store.sequenceFor(absPath) : null;
  if (seq && typeof mediaPool.ImportMedia === 'function') {
    try {
      const r = await mediaPool.ImportMedia([{ FilePath: seq.pattern, StartIndex: seq.start, EndIndex: seq.end }]);
      const items = Array.isArray(r) ? r : Object.values(r || {});
      attempts.push({ method: 'MediaPool.ImportMedia(sequence)', returned: items.length });
      if (items.length) return { ok: true, item: items[0], count: items.length, reused: false, method: 'MediaPool.ImportMedia(sequence)', attempts };
    } catch (e) {
      attempts.push({ method: 'MediaPool.ImportMedia(sequence)', error: String(e?.message || e) });
    }
  }
  if (typeof mediaPool.ImportMedia === 'function') {
    try {
      const r = await mediaPool.ImportMedia([absPath]);
      const items = resultItems(r);
      attempts.push({ method: 'MediaPool.ImportMedia([path])', returned: items.length, rawType: Array.isArray(r) ? 'array' : typeof r });
      if (items.length) return { ok: true, item: items[0], count: items.length, reused: false, method: 'MediaPool.ImportMedia', attempts };
    } catch (e) {
      attempts.push({ method: 'MediaPool.ImportMedia([path])', error: String(e?.message || e) });
    }
  }

  const storage = await resolve.GetMediaStorage();
  if (storage && typeof storage.AddItemListToMediaPool === 'function') {
    // Try both documented call forms because the native Workflow Integration bridge
    // may marshal variadic and array arguments differently.
    for (const mode of ['array', 'scalar']) {
      try {
        const r = mode === 'array'
          ? await storage.AddItemListToMediaPool([absPath])
          : await storage.AddItemListToMediaPool(absPath);
        const items = resultItems(r);
        attempts.push({ method: `MediaStorage.AddItemListToMediaPool(${mode})`, returned: items.length, rawType: Array.isArray(r) ? 'array' : typeof r });
        if (items.length) return { ok: true, item: items[0], count: items.length, reused: false, method: `MediaStorage.${mode}`, attempts };
      } catch (e) {
        attempts.push({ method: `MediaStorage.AddItemListToMediaPool(${mode})`, error: String(e?.message || e) });
      }
    }
  } else {
    attempts.push({ method: 'MediaStorage', error: 'GetMediaStorage/AddItemListToMediaPool unavailable' });
  }

  // Some Resolve builds return an empty collection when the file already exists.
  // Make one final lookup before declaring failure.
  try {
    const existing = await findExistingMediaPoolItem(mediaPool, absPath);
    if (existing) return { ok: true, item: existing, count: 0, reused: true, method: 'existing-after-import', attempts };
  } catch (e) {
    attempts.push({ method: 'find-existing-after-import', error: String(e?.message || e) });
  }

  const detail = attempts.map(a => {
    if (a.error) return `${a.method}: ${a.error}`;
    return `${a.method}: returned ${a.returned ?? 0} item(s), raw=${a.rawType || 'unknown'}`;
  }).join(' | ');
  return { ok: false, error: 'Could not import SFX into Media Pool', detail, attempts };
}

async function importToMediaPool(filePath) {
  const resolve = await getResolve();
  if (!resolve) return bridgeUnavailableResult();
  const pm = await resolve.GetProjectManager();
  const project = pm && await pm.GetCurrentProject();
  if (!project) return { ok: false, error: 'No project open' };
  const imported = await importSfxItem(resolve, project, filePath);
  if (!imported.ok) return imported;
  return { ok: true, count: imported.count, reused: imported.reused, method: imported.method };
}

async function getAudioTracks() {
  const resolve = await getResolve();
  if (!resolve) return { ok: false, tracks: [], bridge: bridgeStatus };
  try {
    const pm = await resolve.GetProjectManager();
    const project = pm && await pm.GetCurrentProject();
    const timeline = project && await project.GetCurrentTimeline();
    if (!timeline) return { ok: false, tracks: [], error: 'No active timeline' };
    const count = Number(await timeline.GetTrackCount('audio')) || 0;
    const tracks = [];
    for (let i = 1; i <= count; i++) {
      let name = '';
      try { name = String(await timeline.GetTrackName('audio', i) || ''); } catch {}
      tracks.push({ index: i, name: name || `Audio ${i}` });
    }
    return { ok: true, tracks };
  } catch (e) {
    return { ok: false, tracks: [], error: String(e?.message || e) };
  }
}

function parseFps(fpsRaw) {
  const match = String(fpsRaw ?? '').match(/[0-9]+(?:\.[0-9]+)?/);
  return match ? Number(match[0]) : 24;
}

function tcToFrames(tc, fpsRaw) {
  const fps = parseFps(fpsRaw);
  const nominal = Math.round(fps);
  const raw = String(tc || '00:00:00:00').trim();
  const isDropFrame = raw.includes(';') && (Math.abs(fps - 29.97) < .02 || Math.abs(fps - 59.94) < .02);
  const parts = raw.replace(';', ':').split(':').map(Number);
  if (parts.length !== 4 || parts.some(Number.isNaN)) return 0;
  const [h, m, s, f] = parts;
  let frames = (h * 3600 + m * 60 + s) * nominal + f;
  if (isDropFrame) {
    const dropFrames = nominal === 60 ? 4 : 2;
    const totalMinutes = h * 60 + m;
    frames -= dropFrames * (totalMinutes - Math.floor(totalMinutes / 10));
  }
  return Math.round(frames);
}

async function getPlayheadRecordFrame(timeline) {
  const fps = await timeline.GetSetting('timelineFrameRate');
  const currentTc = await timeline.GetCurrentTimecode();
  let startTc = null;
  let timelineStartFrame = null;
  try { startTc = await timeline.GetStartTimecode(); } catch {}
  try { timelineStartFrame = await timeline.GetStartFrame(); } catch {}

  const currentAbs = tcToFrames(currentTc, fps);
  const startAbs = startTc ? tcToFrames(startTc, fps) : 0;
  // GetStartFrame is the safest anchor because custom timeline start TC can be non-zero.
  const recordFrame = Number.isFinite(Number(timelineStartFrame))
    ? Math.round(Number(timelineStartFrame) + (currentAbs - startAbs))
    : currentAbs;

  return { recordFrame, timecode: currentTc, startTimecode: startTc, timelineStartFrame, fps: parseFps(fps) };
}

async function insertAtPlayhead(filePath, requestedTrackIndex = 1, range = null) {
  const resolve = await getResolve();
  if (!resolve) return bridgeUnavailableResult();
  const pm = await resolve.GetProjectManager();
  const project = pm && await pm.GetCurrentProject();
  if (!project) return { ok: false, error: 'No project open' };
  const timeline = await project.GetCurrentTimeline();
  if (!timeline) return { ok: false, error: 'No active timeline' };
  const mediaPool = await project.GetMediaPool();
  if (!mediaPool) return { ok: false, error: 'Media pool unavailable' };

  if(range){try{require('./trim').validateRange(range);}catch(e){return {ok:false,error:e.message};}}
  const imported = await importSfxItem(resolve, project, filePath);
  if (!imported.ok) return imported;

  const pos = await getPlayheadRecordFrame(timeline);
  let trackIndex = Math.max(1, Math.round(Number(requestedTrackIndex) || 1));
  let audioTrackCount = 0;
  try { audioTrackCount = Number(await timeline.GetTrackCount('audio')) || 0; } catch {}
  if (audioTrackCount > 0) trackIndex = Math.min(trackIndex, audioTrackCount);

  const clipInfo = {
    mediaPoolItem: imported.item,
    mediaType: 2,
    trackIndex,
    recordFrame: pos.recordFrame
  };
  if(range){
    let sourceFps=pos.fps,frameCount=null;
    try{const raw=String(await imported.item.GetClipProperty('FPS')||'').trim();const parsed=Number(raw);if(Number.isFinite(parsed)&&parsed>0)sourceFps=parsed;}catch{}
    try{const n=Number(await imported.item.GetClipProperty('Frames'));if(Number.isFinite(n)&&n>0)frameCount=n;}catch{}
    try{Object.assign(clipInfo,require('./trim').trimFrames(range,sourceFps,frameCount));}catch(e){return {ok:false,error:e.message};}
  }
  let result = null;
  try {
    result = await mediaPool.AppendToTimeline([clipInfo]);
  } catch (e) {
    return { ok: false, error: 'Resolve rejected AppendToTimeline', detail: String(e?.message || e), importMethod: imported.method, reused: imported.reused, ...pos };
  }
  const timelineItems = resultItems(result);
  if (!timelineItems.length) {
    return { ok: false, error: 'Resolve rejected AppendToTimeline at the current playhead', detail: `import=${imported.method}; recordFrame=${pos.recordFrame}; raw=${Array.isArray(result) ? 'array' : typeof result}`, importMethod: imported.method, reused: imported.reused, ...pos };
  }
  return { ok: true, count: timelineItems.length, reused: imported.reused, importMethod: imported.method, trackIndex, ...pos };
}

let backgroundStarted=false,scanRunning=false,connectionStarted=false,startupTimer=null,connectionTimer=null,reconnectTimer=null,closing=false;
function send(channel,data){if(mainWindow&&!mainWindow.isDestroyed())mainWindow.webContents.send(channel,data);}
async function connectAutomatically(attempt=0){
  if(closing)return;
  let status;try{status=await initializeResolve(false);}catch(e){status={ok:false,error:String(e.message||e)};}
  if(closing)return;
  send('resolve:changed',{...status,retrying:!status.ok&&attempt<9});
  if(!status.ok&&attempt<9)reconnectTimer=setTimeout(()=>connectAutomatically(attempt+1),2000);
}
function startBackground(settings){
  if(closing)return;
  watcher.start(settings.libraries||[]);
  exclusive(async()=>{
    scanRunning=true;send('library:scan-status',{running:true});
    try{
      let index,scan;
      if(isMrm()){index=await loadIndex();scan={count:index.files.length,added:0,changed:0,removed:0,unchanged:index.files.length,warnings:store.status().ok?[]:[store.error]};}
      else{const current=await loadSettings();scan=await rebuildIndex(current.libraries||[]);index=await loadIndex();}
      send('library:changed',{index,scan});
      scanRunning=false;send('library:scan-status',{running:false,scan});
    }catch(e){scanRunning=false;send('library:scan-status',{running:false,error:String(e.message||e)});}
  }).catch(e=>send('library:scan-status',{running:false,error:String(e.message||e)}));
}

let portable = null; // tạo trong initMode() (thư mục phụ thuộc chế độ)
function registerIpc() {
  require('./sfx-drag').registerSfxDrag({ipcMain,loadIndex,nativeImage:require('electron').nativeImage,getWindow:()=>mainWindow});
  const smartInsert=require('./smart-audio').createSmartInserter({getResolve,loadIndex,importItem:importSfxItem,resultItems,getPosition:getPlayheadRecordFrame});
  require('./auto-bin').registerAutoBin({ipcMain,getResolve,dir:userDataDir,exclusive,progress:data=>mainWindow?.webContents.send('auto-bin:progress',data)});
  require('./audio-export').registerAudioExport({ipcMain,dialog,getWindow:()=>mainWindow,loadIndex});
  require('./timeline-range').registerTimelineRange({ipcMain,getResolve,loadIndex,importItem:importSfxItem,resultItems,exclusive});
  require('./updater').registerUpdater({ipcMain,dir:userDataDir,current:PLUGIN_VERSION,shell:require('electron').shell});
  thumbnailDecoder=require('./thumbnail-decoder').createThumbnailDecoder({BrowserWindow,ipcMain});
  require('./visual-media').registerVisualMedia({ipcMain,thumbnailDecoder,dir:userDataDir,loadIndex,getResolve,importItem:importSfxItem,resultItems,getPosition:getPlayheadRecordFrame,exclusive});
  const handle = (channel, fn) => ipcMain.handle(channel, (...args)=>exclusive(()=>fn(...args)));
  const maintenance=require('./maintenance');
  handle('maintenance:stats',()=>maintenance.stats(userDataDir));
  handle('maintenance:clear',(_e,kind)=>maintenance.clear(userDataDir,kind));
  handle('maintenance:export',async()=>{const chosen=await dialog.showSaveDialog(mainWindow,{title:'Xuất thông tin chẩn đoán HNH',defaultPath:'HNH-diagnostics-'+new Date().toISOString().slice(0,10)+'.json',filters:[{name:'JSON',extensions:['json']}]});if(chosen.canceled||!chosen.filePath)return null;const report=maintenance.diagnostic({version:PLUGIN_VERSION,versions:process.versions,platform:process.platform,arch:process.arch,release:require('os').release(),settings:await loadSettings(),index:await loadIndex(),bridge:bridgeStatus,cache:await maintenance.stats(userDataDir).catch(()=>({unavailable:true}))});return maintenance.writeReport(chosen.filePath,report);});
  ipcMain.handle('app:get-state', async () => {
    await initMode();
    if(!connectionStarted){connectionStarted=true;connectionTimer=setTimeout(()=>connectAutomatically(),100);}
    // Return the saved index immediately; disk reconciliation and bridge init run independently.
    const [settings,index] = await Promise.all([loadSettings(),loadIndex()]);
    if(!backgroundStarted){backgroundStarted=true;scanRunning=true;startupTimer=setTimeout(()=>startBackground(settings),150);}
    return {settings,index,resolve:bridgeStatus,storage:{directory:isMrm()?userDataDir:standaloneDir,notices:modeNotices()},mrm:{...store.status(),mode:MODE},backgroundScan:scanRunning};
  });

  handle('library:add', async (_e, requestedTypes = ['audio']) => {
    const selectedTypes=require('./library-types').validateTypes(requestedTypes);
    const result = await dialog.showOpenDialog(mainWindow, {
      title: 'Chọn thư mục chứa âm thanh, ảnh hoặc video',
      properties: ['openDirectory']
    });
    if (result.canceled || !result.filePaths.length) return null;
    const selected = result.filePaths[0];
    if (!isMrm()) {
      const settings = await loadSettings();
      const samePath=p=>path.resolve(p).toLowerCase();
      const existing=settings.libraries.find(x=>samePath(x.path)===samePath(selected));
      if(existing)existing.types=[...new Set([...require('./library-types').types(existing),...selectedTypes])];
      else settings.libraries.push({path:selected,enabled:true,types:selectedTypes});
      await saveSettings(settings);
      const scan = await rebuildIndex(settings.libraries);
      watcher.start(settings.libraries);
      return { settings, index: await loadIndex(), scan };
    }
    // Thư viện được quản lý trong MRM: thêm vào DB chung rồi nhờ MRM quét
    const reqId = store.addLibrary(selected, selectedTypes);
    let scan;
    if (store.mrmRunning()) { await store.waitRequest(reqId, 60000); scan = { count: store.loadIndex().files.length, added: 0, changed: 0, removed: 0, warnings: [] }; }
    else scan = { count: 0, added: 0, changed: 0, removed: 0, warnings: ['Đã thêm thư viện vào MRM. Mở MRM để quét lần đầu.'] };
    const settings = await loadSettings();
    watcher.start(settings.libraries);
    return { settings, index: await loadIndex(), scan };
  });

  handle('library:types', async (_e, libraryPath, requestedTypes) => {
    const selectedTypes=require('./library-types').validateTypes(requestedTypes),settings=await loadSettings();
    const lib=settings.libraries.find(l=>path.resolve(l.path).toLowerCase()===path.resolve(libraryPath).toLowerCase());
    if(!lib)throw Error('Thư mục không còn trong thư viện.');
    let scan;
    if(isMrm()){store.setLibraryTypes(lib.path,selectedTypes);scan=await rebuildIndex();}
    else{lib.types=selectedTypes;await saveSettings(settings);scan=await rebuildIndex(settings.libraries);watcher.start(settings.libraries);}
    return {settings,index:await loadIndex(),scan};
  });

  handle('library:remove', async (_e, libraryPath) => {
    // Gỡ thư viện sẽ xóa metadata của cả thư viện trong MRM -> chỉ cho làm trong MRM (có backup tự động)
    if (isMrm()) throw new Error('Gỡ thư viện trong MRM (Libraries & Settings) — MRM tự sao lưu trước khi gỡ.');
    const settings = await loadSettings();
    settings.libraries = settings.libraries.filter(x => path.resolve(x.path) !== path.resolve(libraryPath));
    await saveSettings(settings);
    const scan = await rebuildIndex(settings.libraries);
    watcher.start(settings.libraries);
    return { settings, index: await loadIndex(), scan };
  });

  handle('library:rescan', async () => {
    const settings = await loadSettings();
    const scan = await rebuildIndex(settings.libraries);
    watcher.start(settings.libraries);
    return { index: await loadIndex(), scan };
  });

  handle('metadata:rename-tag',async(_e,request)=>{const before=await loadSettings(),result=require('./tag-manager').renameTag(before,request);const backup=path.join(userDataDir,'before-tag-rename-'+Date.now()+'.json');await writeJson(backup,before);await saveSettings(result.settings);return {...result,backup};});
  handle('metadata:bulk',async(_e,request)=>{const result=require('./bulk-metadata').applyBulk(await loadSettings(),await loadIndex(),request);await saveSettings(result.settings);return result;});

  handle('settings:update', async (_e, patch) => {
    const settings = await loadSettings();
    const next = { ...settings, ...patch };
    return saveSettings(next);
  });

  async function saveDurations(entries) {
    const accepted=[];
    if(isMrm()){
      for(const {file,duration} of entries){ if(Number.isFinite(duration)&&duration>=0&&store.saveDuration(file.path,duration)) accepted.push(file.path); }
      return accepted;
    }
    const index=await loadIndex(),byPath=new Map(index.files.map(f=>[f.path,f]));
    for(const {file,duration} of entries){
      if(!Number.isFinite(duration)||duration<0)continue;
      const entry=byPath.get(file.path);
      if(!entry||library.fingerprint(entry)!==library.fingerprint(file))continue;
      const stat=await fsp.stat(entry.path).catch(()=>null);
      if(!stat||stat.size!==entry.size||stat.mtimeMs!==entry.mtimeMs)continue;
      entry.duration=duration;accepted.push(file.path);
    }
    if(accepted.length)await writeJson(indexPath,index);
    return accepted;
  }
  handle('audio:metadata',async(_e,file,duration)=>(await saveDurations([{file,duration}])).length>0);
  handle('audio:metadata-batch',async(_e,entries)=>saveDurations(entries.slice(0,32)));
  const cacheDir=path.join(userDataDir,'waveforms-v1');
  async function cacheFile(file) {
    const stat=await fsp.stat(file.path);
    if(stat.size!==file.size || stat.mtimeMs!==file.mtimeMs) throw new Error('File changed; rescan required');
    await fsp.mkdir(cacheDir,{recursive:true});
    return path.join(cacheDir,library.fingerprint(file)+'.json');
  }
  ipcMain.handle('waveform:get',async(_e,file)=>readJson(await cacheFile(file),null));
  handle('waveform:put',async(_e,file,data)=>{
    if(!Array.isArray(data.peaks)||data.peaks.length!==900||!data.peaks.every(n=>Number.isFinite(n)&&n>=0&&n<=1)||!Number.isFinite(data.duration)||data.duration<0)throw new Error('Invalid waveform');
    await writeJson(await cacheFile(file),data);
    // Bounded disk cache; most recent 2,000 waveforms survive restarts.
    const names=await fsp.readdir(cacheDir);
    if(names.length>2000){const entries=await Promise.all(names.map(async name=>({name,mtime:(await fsp.stat(path.join(cacheDir,name))).mtimeMs}))); entries.sort((a,b)=>a.mtime-b.mtime);for(const e of entries.slice(0,entries.length-2000))await fsp.unlink(path.join(cacheDir,e.name));}
    return true;
  });

  const transfer=(channel,fn)=>handle(channel,async(...args)=>{watcher.close();try{return await fn(...args);}finally{const s=await loadSettings();watcher.start(s.libraries||[]);}});
  ipcMain.handle('backup:cancel',()=>{portable.cancel();return true;});
  transfer('backup:export',async(_e,options)=>{
    const r=await dialog.showSaveDialog(mainWindow,{title:'Xuất bản sao lưu HNH',defaultPath:'HNH-SFX-'+new Date().toISOString().slice(0,10)+'.hnhbackup',filters:[{name:'HNH SFX backup',extensions:['hnhbackup']}]});
    if(r.canceled||!r.filePath)return null;
    return portable.exportFile(r.filePath,options);
  });
  transfer('backup:open',async()=>{
    const r=await dialog.showOpenDialog(mainWindow,{title:'Mở bản sao lưu HNH',properties:['openFile'],filters:[{name:'HNH SFX backup',extensions:['hnhbackup']}]});
    if(r.canceled||!r.filePaths.length)return null;
    return portable.openFile(r.filePaths[0]);
  });
  transfer('backup:library',(_e,root)=>portable.openLibrary(root));
  transfer('backup:pending',()=>portable.openPending());
  handle('backup:folder',async()=>{const r=await dialog.showOpenDialog(mainWindow,{title:'Chọn thư mục tương ứng trên máy này',properties:['openDirectory']});return r.canceled?null:r.filePaths[0];});
  transfer('backup:prepare',(_e,mappings)=>portable.prepare(mappings));
  const useMrmBackup=()=>{throw new Error('Đang dùng chung dữ liệu với MRM: khôi phục dữ liệu bằng Backup trong MRM (Libraries & Settings).');};
  transfer('backup:apply',(_e,token,overrides,options)=>isMrm()?useMrmBackup():portable.apply(token,overrides,options));
  transfer('backup:undo',()=>isMrm()?useMrmBackup():portable.undo());

  ipcMain.handle('resolve:status', async () => initializeResolve(false));
  ipcMain.handle('resolve:retry', async () => initializeResolve(true));
  ipcMain.handle('resolve:tracks', async () => getAudioTracks());
  ipcMain.handle('resolve:import', async (_e, filePath) => importToMediaPool(filePath));
  ipcMain.handle('resolve:insert', (_e,filePath,trackIndex,range,options={}) => exclusive(()=>options.smart?smartInsert(filePath,trackIndex,range,options):insertAtPlayhead(filePath,trackIndex,range)));
  ipcMain.handle('audio:read', async (_e, filePath) => {
    const abs = path.resolve(String(filePath || ''));
    const stat = await fsp.stat(abs);
    if (!stat.isFile()) throw new Error('Audio path is not a file');
    if (stat.size > 120 * 1024 * 1024) throw new Error('Audio file is too large for waveform preview');
    const buf = await fsp.readFile(abs);
    return new Uint8Array(buf);
  });

  ipcMain.handle('help:developer-contact', async () => { await require('electron').shell.openExternal('https://www.facebook.com/phamnam22s/'); return true; });

  ipcMain.handle('shell:reveal', async (_e, filePath) => {
    const { shell } = require('electron');
    shell.showItemInFolder(filePath);
    return true;
  });
}

async function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1040,
    height: 820,
    minWidth: 760,
    minHeight: 680,
    backgroundColor: '#16181c',
    title: 'HNH SFX Finder',
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true
    }
  });
  mainWindow.once('closed',()=>thumbnailDecoder?.dispose());
  await mainWindow.loadFile(path.join(__dirname, 'UI', 'index.html'));
}

app.whenReady().then(async () => {
  await initMode();
  registerIpc();
  await createWindow();
});

app.on('window-all-closed', () => {
  closing=true;clearTimeout(startupTimer);clearTimeout(connectionTimer);clearTimeout(reconnectTimer);
  watcher.close();
  store.close(); // dừng nhịp tim -> MRM mở lại Media Browser
  try { if (initialized && WorkflowIntegration?.CleanUp) WorkflowIntegration.CleanUp(); } catch {}
  app.quit();
});
