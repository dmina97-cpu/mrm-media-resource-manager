import { invoke } from "@tauri-apps/api/core";

export type TagKind = "app" | "type" | "function" | "personal" | "status";

export interface Tag {
  id: number;
  kind: TagKind;
  name: string;
  color: string | null;
  count: number;
}

export interface Library {
  id: number;
  name: string;
  path: string;
  scan_depth: number;
  last_scan_at: number | null;
  online: boolean;
  source_count: number;
  size: number;
}

export interface LibraryImpact {
  resources: number;
  curated_resources: number;
  shared_resources: number;
  media: number;
  curated_media: number;
}

export interface RelinkPreview {
  path: string;
  checked: number;
  found: number;
}

export interface ResourceSummary {
  id: number;
  name: string;
  version: string | null;
  favorite: boolean;
  size: number;
  kinds: string[];
  source_count: number;
  unavailable_count: number;
  tag_ids: number[];
  /** tag gắn tự động, chưa xác nhận */
  auto_tag_ids: number[];
  has_notes: boolean;
  created_at: number;
  updated_at: number;
  modified_at: number | null;
}

export interface SourceInfo {
  id: number;
  library_id: number;
  library_name: string;
  abs_path: string;
  rel_path: string;
  name: string;
  kind: "folder" | "archive" | "file";
  size: number;
  file_count: number;
  modified_at: number | null;
  extensions: [string, number][];
  available: boolean;
  match_state: string;
  match_score: number | null;
  library_auto: boolean;
  override_mode: "pack" | "container" | null;
}

export interface ScanOverride {
  rel_path: string;
  mode: "pack" | "container";
}

export interface SourceBrief {
  source_id: number;
  resource_id: number;
  resource_name: string;
  name: string;
  kind: string;
  rel_path: string;
  library_name: string;
  size: number;
}

export interface MatchSuggestion {
  id: number;
  score: number;
  a: SourceBrief;
  b: SourceBrief;
}

export interface SuggestedTag {
  kind: TagKind;
  name: string;
  tag_id: number | null;
  source: "content" | "path" | "rule" | "learned" | "ai";
  reason: string;
  score: number;
}

export interface VersionItem {
  id: number;
  name: string;
  version: string | null;
  latest: boolean;
}

export type RelationKind = "addon_of" | "requires" | "alternative" | "related";

export interface Relation {
  other_id: number;
  other_name: string;
  kind: RelationKind;
  outgoing: boolean;
}

export interface AiAnalysis {
  description: string;
  apps: string[];
  types: string[];
  function_tags: string[];
  install_notes: string;
  model: string;
}

export interface ResourceDetail {
  id: number;
  name: string;
  version: string | null;
  notes: string;
  favorite: boolean;
  created_at: number;
  updated_at: number;
  tag_ids: number[];
  suggested: SuggestedTag[];
  sources: SourceInfo[];
  matches: MatchSuggestion[];
  versions: VersionItem[];
  relations: Relation[];
  cover_user: boolean;
  ai: AiAnalysis | null;
  note_images: NoteImage[];
  ai_applied_at: number | null;
  auto_tags: AutoTag[];
}

export interface AutoTag {
  tag_id: number;
  source: string;
  reason: string | null;
  applied_at: number | null;
}

export interface SuggestApplyOptions {
  sources: string[];
  kinds: string[];
  min_score: number;
  dry_run: boolean;
}

export interface SuggestApplyResult {
  resources: number;
  tags: number;
  by_kind: Record<string, number>;
}

export const TAG_SOURCE_LABEL: Record<string, string> = {
  content: "Nội dung file",
  path: "Tên thư mục",
  rule: "Rule",
  learned: "Học từ bạn",
  ai: "AI",
  scan: "Máy quét",
};

export interface NoteImage {
  id: number;
  file: string;
  position: number;
}

export interface NoteImageInput {
  path?: string;
  data_b64?: string;
  ext?: string;
  source_id?: number;
  inner?: string;
}

export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  kind: "dir" | "image" | "video" | "audio" | "text" | "pdf" | "archive" | "other";
}

export interface MediaInfo {
  duration: number;
  width: number;
  height: number;
  video_codec: string | null;
  audio_codec: string | null;
  fps: number | null;
}

export interface PreviewInfo {
  kind: FileEntry["kind"];
  name: string;
  file: string | null;
  text: string | null;
  truncated: boolean;
  media: MediaInfo | null;
  playable: boolean;
  size: number;
}

// ---------------------------------------------------------------- Media Browser (Phase 3)

export type MediaType = "audio" | "image" | "video";
export type AssetSort = "name" | "added" | "size" | "duration" | "recent" | "relevance";

export interface AssetQuery {
  media_type?: MediaType | null;
  text?: string;
  library_id?: number | null;
  resource_id?: number | null;
  tag_id?: number | null;
  ai_category?: string | null;
  collection_id?: number | null;
  folder?: string | null;
  semantic?: boolean;
  ext?: string | null;
  favorites?: boolean;
  recent?: boolean;
  include_missing?: boolean;
  sort?: AssetSort;
  desc?: boolean;
  offset?: number;
  limit?: number;
}

export interface AssetItem {
  id: number;
  media_type: MediaType;
  filename: string;
  ext: string;
  rel_path: string;
  library_id: number;
  size: number;
  modified_ms: number;
  added_at: number;
  available: boolean;
  duration: number | null;
  width: number | null;
  height: number | null;
  seq_count: number | null;
  favorite: boolean;
  resource_id: number | null;
  resource_name: string | null;
  ai_category: string | null;
}

export interface AssetDetail extends AssetItem {
  ai_score: number | null;
  ai_caption: string | null;
  uid: string;
  path: string;
  library_name: string;
  meta_state: number;
  fps: number | null;
  codec: string | null;
  sample_rate: number | null;
  channels: number | null;
  seq_pattern: string | null;
  seq_start: number | null;
  seq_end: number | null;
  use_count: number;
  last_used_at: number | null;
  tags: { id: number; name: string }[];
}

export interface AssetCounts {
  audio: number;
  image: number;
  video: number;
  favorites: number;
  recent: number;
  pending_meta: number;
}

export interface AssetFacets {
  exts: { key: string; id: null; count: number }[];
  tags: { key: string; id: number; count: number }[];
  ai: { key: string; id: null; count: number }[];
}

export interface FolderNode {
  library_id: number;
  name: string;
  path: string;
  count: number;
  has_children: boolean;
}

export interface FavoriteFolder {
  id: number;
  library_id: number;
  path: string;
  name: string;
  count: number;
}

export interface CollectionInfo {
  id: number;
  name: string;
  resources: number;
  assets: number;
}

export interface UpdateInfo {
  current: string;
  latest: {
    version: string;
    tag: string;
    notes: string;
    published_at: string;
    asset_name: string;
    asset_url: string;
    size: number;
    digest: string | null;
    page_url: string;
  } | null;
  available: boolean;
  skipped: boolean;
  auto: boolean;
  checked_at: number | null;
  error: string | null;
}

export interface VisionStatus {
  enabled: boolean;
  installed: boolean;
  model: string;
  done: number;
  total: number;
  paused: boolean;
  running: boolean;
  installing: boolean;
}

export interface AssetAiStatus {
  indexed: number;
  total: number;
  categorized: number;
}

export interface Presence {
  plugin_active: boolean;
  version: string | null;
  last_seen: number | null;
}

export interface CoverInfo {
  thumb: string;
  source_id: number;
  path: string;
  kind: string;
}

export interface ResourceBrief {
  id: number;
  name: string;
  version: string | null;
  size: number;
  locations: string[];
}

export interface DuplicateGroup {
  reason: "content" | "name";
  resources: ResourceBrief[];
}

export interface ExistingHit {
  id: number;
  name: string;
  version: string | null;
  score: number;
}

export interface RuleCondition {
  field: "name" | "source" | "path" | "ext" | "library";
  op: "contains" | "not_contains" | "equals" | "starts_with" | "ends_with";
  value: string;
}

export interface Rule {
  id: number;
  name: string;
  enabled: boolean;
  auto_apply: boolean;
  match_all: boolean;
  conditions: RuleCondition[];
  tag_ids: number[];
}

export interface AiProgress {
  phase: "idle" | "downloading" | "extracting" | "starting" | "pulling" | "indexing" | "analyzing" | "error" | "";
  message: string;
  done: number;
  total: number;
}

export interface AiStatus {
  enabled: boolean;
  installed: boolean;
  running: boolean;
  embed_model: string;
  chat_model: string;
  embed_ready: boolean;
  chat_ready: boolean;
  progress: AiProgress;
  busy: boolean;
  gpu: string | null;
  storage_dir: string;
  free_bytes: number | null;
  indexed: number;
  total: number;
  analyzing: boolean;
}

export interface StorageInfo {
  dir: string;
  free_bytes: number | null;
  default_dir: string;
}

export interface CacheStats {
  dir: string;
  size: number;
  limit_mb: number;
}

export type ListView = "all" | "unclassified" | "review" | "favorites" | "missing" | "tag" | "library" | "collection";
export type SortKey = "name" | "size" | "updated" | "created" | "modified" | "relevance";

export interface ResourceQuery {
  view: ListView;
  tag_id?: number | null;
  library_id?: number | null;
  collection_id?: number | null;
  search: string;
  sort: SortKey;
  desc: boolean;
  semantic?: boolean;
}

export interface ScanResult {
  library_id: number;
  online: boolean;
  found: number;
  added: number;
  missing: number;
  regrouped: number;
  auto_merged: number;
  suggested: number;
}

export interface ScanProgress {
  library_id: number;
  phase: "listing" | "inspecting" | "saving" | "done";
  done: number;
  total: number;
  current: string;
}

export interface Bucket {
  id: number;
  name: string;
  count: number;
  size: number;
}

export interface Dashboard {
  resources: number;
  total_size: number;
  archives: number;
  folders: number;
  matched: number;
  unclassified: number;
  favorites: number;
  missing: number;
  pending_matches: number;
  review: number;
  by_app: Bucket[];
  by_type: Bucket[];
  by_status: Bucket[];
  by_library: Bucket[];
}

export interface BackupInfo {
  path: string;
  name: string;
  size: number;
  modified: number;
}

export const api = {
  listLibraries: () => invoke<Library[]>("list_libraries"),
  addLibrary: (path: string, name?: string, scanDepth?: number) =>
    invoke<Library>("add_library", { path, name, scanDepth }),
  updateLibrary: (id: number, name: string, scanDepth: number) =>
    invoke<void>("update_library", { id, name, scanDepth }),
  removeLibrary: (id: number) => invoke<void>("remove_library", { id }),
  libraryImpact: (id: number) => invoke<LibraryImpact>("library_impact", { id }),
  previewRelinkLibrary: (id: number, path: string) => invoke<RelinkPreview>("preview_relink_library", { id, path }),
  relinkLibrary: (id: number, path: string) => invoke<Library>("relink_library", { id, path }),
  scanLibrary: (id: number, full = false) => invoke<ScanResult>("scan_library", { id, full }),
  scanAll: () => invoke<ScanResult[]>("scan_all"),

  queryResources: (q: ResourceQuery) => invoke<ResourceSummary[]>("query_resources", { q }),
  getResource: (id: number) => invoke<ResourceDetail | null>("get_resource", { id }),
  updateResource: (id: number, patch: { name?: string; notes?: string; favorite?: boolean }) =>
    invoke<void>("update_resource", { id, patch }),
  setFavorite: (ids: number[], favorite: boolean) => invoke<void>("set_favorite", { ids, favorite }),
  setTag: (resourceIds: number[], tagId: number, on: boolean) =>
    invoke<void>("set_tag", { resourceIds, tagId, on }),
  mergeResources: (ids: number[]) => invoke<number>("merge_resources", { ids }),
  splitSource: (sourceId: number) => invoke<number>("split_source", { sourceId }),
  revealSource: (sourceId: number) => invoke<void>("reveal_source", { sourceId }),

  listTags: () => invoke<Tag[]>("list_tags"),
  createTag: (kind: TagKind, name: string) => invoke<number>("create_tag", { kind, name }),
  updateTag: (id: number, name: string, color: string | null) => invoke<void>("update_tag", { id, name, color }),
  deleteTag: (id: number) => invoke<void>("delete_tag", { id }),

  listMatchSuggestions: () => invoke<MatchSuggestion[]>("list_match_suggestions"),
  resolveMatch: (id: number, accept: boolean) => invoke<number | null>("resolve_match", { id, accept }),

  getDashboard: () => invoke<Dashboard>("get_dashboard"),

  backupNow: () => invoke<string>("backup_now"),
  listBackups: () => invoke<BackupInfo[]>("list_backups"),
  restoreBackup: (path: string) => invoke<void>("restore_backup", { path }),
  exportJson: (path: string) => invoke<void>("export_json", { path }),
  openDataDir: () => invoke<void>("open_data_dir"),
  getDataDir: () => invoke<string>("get_data_dir"),
  getStorageInfo: () => invoke<StorageInfo>("get_storage_info"),
  setStorageDir: (path: string) => invoke<void>("set_storage_dir", { path }),
  listScanOverrides: (libraryId: number) => invoke<ScanOverride[]>("list_scan_overrides", { libraryId }),
  setScanOverride: (libraryId: number, relPath: string, mode: "pack" | "container" | null) =>
    invoke<void>("set_scan_override", { libraryId, relPath, mode }),

  // Phase 3 — preview
  listSourceFiles: (sourceId: number, dir: string) => invoke<FileEntry[]>("list_source_files", { sourceId, dir }),
  getThumbnail: (sourceId: number, path: string, size?: number) => invoke<string>("get_thumbnail", { sourceId, path, size }),
  getVideoSprite: (sourceId: number, path: string) => invoke<string>("get_video_sprite", { sourceId, path }),
  previewFile: (sourceId: number, path: string) => invoke<PreviewInfo>("preview_file", { sourceId, path }),
  getWaveform: (sourceId: number, path: string) => invoke<number[]>("get_waveform", { sourceId, path }),
  getVideoProxy: (sourceId: number, path: string) => invoke<string>("get_video_proxy", { sourceId, path }),
  // Media Browser — các lệnh xem trước dùng chung pipeline, truyền assetId thay cho sourceId
  queryAssets: (query: AssetQuery) => invoke<{ total: number; items: AssetItem[] }>("query_assets", { query }),
  assetCounts: () => invoke<AssetCounts>("asset_counts"),
  assetFacets: (mediaType: MediaType | null) => invoke<AssetFacets>("asset_facets", { mediaType }),
  getAsset: (id: number) => invoke<AssetDetail>("get_asset", { id }),
  setAssetFavorite: (ids: number[], on: boolean) => invoke<void>("set_asset_favorite", { ids, on }),
  setAssetTag: (ids: number[], name: string, on: boolean) => invoke<void>("set_asset_tag", { ids, name, on }),
  listAssetTagNames: () => invoke<string[]>("list_asset_tag_names"),
  useAssets: (ids: number[]) => invoke<string[]>("use_assets", { ids }),
  revealAsset: (id: number) => invoke<void>("reveal_asset", { id }),
  dragIcon: () => invoke<string>("drag_icon"),
  similarAssets: (id: number) => invoke<AssetItem[]>("similar_assets", { id }),
  tagAssetsByQuery: (query: AssetQuery, name: string) => invoke<number>("tag_assets_by_query", { query, name }),
  assetAiStatus: () => invoke<AssetAiStatus>("asset_ai_status"),
  visionStatus: () => invoke<VisionStatus>("vision_status"),
  assetFolders: (libraryId: number | null, parent: string, mediaType: MediaType | null) =>
    invoke<FolderNode[]>("asset_folders", { libraryId, parent, mediaType }),
  listFavoriteFolders: (mediaType: MediaType | null) => invoke<FavoriteFolder[]>("list_favorite_folders", { mediaType }),
  setFavoriteFolder: (libraryId: number, path: string, name: string, on: boolean) =>
    invoke<void>("set_favorite_folder", { libraryId, path, name, on }),
  revealResource: (id: number) => invoke<void>("reveal_resource", { id }),
  resourcePaths: (id: number) => invoke<string[]>("resource_paths", { id }),
  listCollections: () => invoke<CollectionInfo[]>("list_collections"),
  createCollection: (name: string) => invoke<number>("create_collection", { name }),
  renameCollection: (id: number, name: string) => invoke<void>("rename_collection", { id, name }),
  deleteCollection: (id: number) => invoke<void>("delete_collection", { id }),
  setCollectionResources: (collectionId: number, ids: number[], on: boolean) => invoke<void>("set_collection_resources", { collectionId, ids, on }),
  setCollectionAssets: (collectionId: number, ids: number[], on: boolean) => invoke<void>("set_collection_assets", { collectionId, ids, on }),
  collectionsOf: (resourceId: number | null, assetId: number | null) => invoke<number[]>("collections_of", { resourceId, assetId }),
  exportDiagnostics: (path: string) => invoke<void>("export_diagnostics", { path }),
  setCoverImage: (resourceId: number, input: NoteImageInput) => invoke<NoteImage>("set_cover_image", { resourceId, input }),
  setCoverFromAsset: (id: number) => invoke<number>("set_cover_from_asset", { id }),
  updateCheck: (manual: boolean) => invoke<UpdateInfo>("update_check", { manual }),
  updateConfigure: (auto: boolean | null, skip: string | null) => invoke<UpdateInfo>("update_configure", { auto, skip }),
  updateInstall: () => invoke<void>("update_install"),
  getScanExcludes: () => invoke<{ rules: string[]; defaults: string[] }>("get_scan_excludes"),
  setScanExcludes: (rules: string[]) => invoke<void>("set_scan_excludes", { rules }),
  visionSetEnabled: (enabled: boolean) => invoke<void>("vision_set_enabled", { enabled }),
  visionSetPaused: (paused: boolean) => invoke<void>("vision_set_paused", { paused }),
  resourceAssetCounts: (resourceId: number) => invoke<{ audio: number; image: number; video: number }>("resource_asset_counts", { resourceId }),
  assetThumbnail: (assetId: number, size?: number) => invoke<string>("get_thumbnail", { assetId, path: "", size }),
  assetSprite: (assetId: number) => invoke<string>("get_video_sprite", { assetId, path: "" }),
  assetPreview: (assetId: number) => invoke<PreviewInfo>("preview_file", { assetId, path: "" }),
  assetWaveform: (assetId: number) => invoke<number[]>("get_waveform", { assetId, path: "" }),
  assetProxy: (assetId: number) => invoke<string>("get_video_proxy", { assetId, path: "" }),
  getPresence: () => invoke<Presence>("get_presence"),
  getCover: (resourceId: number) => invoke<CoverInfo | null>("get_cover", { resourceId }),
  setCover: (resourceId: number, sourceId: number | null, path: string | null) =>
    invoke<void>("set_cover", { resourceId, sourceId, path }),
  cacheStats: () => invoke<CacheStats>("cache_stats"),
  clearCache: () => invoke<void>("clear_cache"),
  setCacheLimit: (mb: number) => invoke<void>("set_cache_limit", { mb }),
  addNoteImage: (resourceId: number, input: NoteImageInput) => invoke<NoteImage>("add_note_image", { resourceId, input }),
  removeNoteImage: (id: number) => invoke<void>("remove_note_image", { id }),
  noteImageToFront: (id: number) => invoke<void>("note_image_to_front", { id }),

  // Phase 4 — automation
  listRules: () => invoke<Rule[]>("list_rules"),
  saveRule: (rule: Rule) => invoke<number>("save_rule", { rule }),
  deleteRule: (id: number) => invoke<void>("delete_rule", { id }),
  testRule: (rule: Rule) => invoke<{ count: number; sample: string[] }>("test_rule", { rule }),
  applyRuleNow: (id: number) => invoke<number>("apply_rule_now", { id }),
  findDuplicates: () => invoke<DuplicateGroup[]>("find_duplicates"),
  dismissDuplicates: (ids: number[]) => invoke<void>("dismiss_duplicates", { ids }),
  checkExisting: (name: string) => invoke<ExistingHit[]>("check_existing", { name }),
  listVersionFamilies: () => invoke<VersionItem[][]>("list_version_families"),
  markOutdatedVersions: () => invoke<number>("mark_outdated_versions"),
  addRelation: (a: number, b: number, kind: RelationKind) => invoke<void>("add_relation", { a, b, kind }),
  removeRelation: (a: number, b: number, kind: RelationKind) => invoke<void>("remove_relation", { a, b, kind }),
  setAutoWatch: (enabled: boolean) => invoke<void>("set_auto_watch", { enabled }),
  getAutoWatch: () => invoke<boolean>("get_auto_watch"),

  // Phase 5 — AI
  aiInstall: () => invoke<void>("ai_install"),
  aiSetEnabled: (enabled: boolean) => invoke<void>("ai_set_enabled", { enabled }),
  aiStatus: () => invoke<AiStatus>("ai_status"),
  aiAnalyze: (ids: number[]) => invoke<void>("ai_analyze", { ids }),
  aiDismiss: (id: number) => invoke<void>("ai_dismiss", { id }),
  applySuggestions: (ids: number[], opts: SuggestApplyOptions) => invoke<SuggestApplyResult>("apply_suggestions", { ids, opts }),
  /** tagId bỏ trống = xác nhận mọi tag tự động của các resource này */
  confirmAutoTags: (ids: number[], tagId?: number) => invoke<number>("confirm_auto_tags", { ids, tagId: tagId ?? null }),
  getPref: (key: string) => invoke<boolean>("get_pref", { key }),
  takeDbNotice: () => invoke<string | null>("take_db_notice"),
  setPref: (key: string, on: boolean) => invoke<void>("set_pref", { key, on }),
  /** ids bỏ trống = áp dụng mọi kết quả AI đang chờ */
  aiApply: (ids?: number[]) => invoke<number>("ai_apply", { ids: ids ?? null }),
  aiGetAutoApply: () => invoke<boolean>("ai_get_auto_apply"),
  aiSetAutoApply: (enabled: boolean) => invoke<void>("ai_set_auto_apply", { enabled }),
  aiPendingCount: () => invoke<number>("ai_pending_count"),
  aiSetModels: (embed: string, chat: string) => invoke<void>("ai_set_models", { embed, chat }),
  aiSimilar: (id: number) => invoke<[number, number][]>("ai_similar", { id }),
};

export const RELATION_LABEL: Record<RelationKind, [string, string]> = {
  // [khi resource hiện tại là a, khi là b]
  addon_of: ["Là add-on của", "Có add-on"],
  requires: ["Cần có", "Được cần bởi"],
  alternative: ["Thay thế cho", "Thay thế cho"],
  related: ["Liên quan", "Liên quan"],
};

export const KIND_LABEL: Record<TagKind, string> = {
  app: "Applications",
  type: "Resource Types",
  function: "Function Tags",
  personal: "Personal Tags",
  status: "Status",
};
