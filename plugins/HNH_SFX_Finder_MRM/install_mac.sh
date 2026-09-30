#!/bin/bash
# HNH SFX Finder (MRM) — cài đặt trên macOS (Mac chip Apple M).
# Chạy: mở Terminal trong thư mục này rồi gõ:  sh install_mac.sh
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
RESOLVE="/Library/Application Support/Blackmagic Design/DaVinci Resolve"
TARGET="$RESOLVE/Workflow Integration Plugins/HNH_SFX_Finder_MRM"
EXAMPLES="$RESOLVE/Developer/Workflow Integrations/Examples"
ITEMS="mrm-store.js smart-audio.js sfx-drag.js auto-bin.js thumbnail-decoder.js maintenance.js tag-manager.js audio-export.js changelog.js release-history.json free-tracks.js timeline-range.js main.js av-insert.js bulk-metadata.js updater.js library.js library-types.js date-filters.js portable.js visual-media.js media-types.js trim.js preload.js package.json manifest.xml UI README.md CHANGELOG.md"

fail() { echo "INSTALL FAILED: $1" >&2; exit 1; }

# cần quyền quản trị để ghi vào /Library
if [ "$(id -u)" -ne 0 ]; then
  echo "Cần quyền quản trị — nhập mật khẩu máy Mac nếu được hỏi."
  exec sudo /bin/bash "$0" "$@"
fi

pgrep -x Resolve >/dev/null 2>&1 && fail "Hãy thoát hẳn DaVinci Resolve trước khi cài."
for i in $ITEMS; do [ -e "$HERE/$i" ] || fail "Thiếu file: $i"; done

# Bridge WorkflowIntegration.node bản macOS: lấy từ gói Developer đi kèm Resolve
NODE=""
for s in SamplePlugin SamplePromisePlugin CompatibleSamplePlugin; do
  if [ -f "$EXAMPLES/$s/WorkflowIntegration.node" ]; then NODE="$EXAMPLES/$s/WorkflowIntegration.node"; break; fi
done
if [ -z "$NODE" ] && [ -f "$HERE/WorkflowIntegration.node" ] && file "$HERE/WorkflowIntegration.node" | grep -q "Mach-O"; then
  NODE="$HERE/WorkflowIntegration.node"
fi
[ -n "$NODE" ] || fail "Không tìm thấy WorkflowIntegration.node của Resolve (thường ở: $EXAMPLES/SamplePlugin). Cần DaVinci Resolve Studio 18.5 trở lên."

PREV=""
if [ -f "$TARGET/package.json" ]; then
  PREV="$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$TARGET/package.json" | head -1)"
  BACKUP="$RESOLVE/HNH-SFX-Finder-Backups/HNH_SFX_Finder_MRM-$(date +%Y%m%d-%H%M%S)"
  mkdir -p "$(dirname "$BACKUP")"
  cp -R "$TARGET" "$BACKUP"
  echo "Đã sao lưu bản cũ vào: $BACKUP"
fi

mkdir -p "$TARGET"
for i in $ITEMS; do rm -rf "$TARGET/$i"; cp -R "$HERE/$i" "$TARGET/"; done
cp "$NODE" "$TARGET/WorkflowIntegration.node"
cmp -s "$NODE" "$TARGET/WorkflowIntegration.node" || fail "Sao chép bridge bị lỗi."

NEW="$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$HERE/package.json" | head -1)"
if [ -n "$PREV" ]; then FROM="\"$PREV\""; else FROM="null"; fi
printf '{ "from": %s, "to": "%s", "installedAt": "%s" }\n' "$FROM" "$NEW" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$TARGET/install-receipt.json"

# bỏ cờ "tải từ Internet" để macOS không chặn plugin
xattr -dr com.apple.quarantine "$TARGET" 2>/dev/null || true
chmod -R a+rX "$TARGET"

echo "Đã cài HNH SFX Finder (MRM) $NEW."
echo "Mở DaVinci Resolve Studio > Workspace > Workflow Integrations > HNH SFX Finder (MRM)."
