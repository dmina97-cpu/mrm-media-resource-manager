$ErrorActionPreference = 'Stop'
try {
  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = New-Object Security.Principal.WindowsPrincipal($identity)
  if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Right-click install_windows.bat and choose Run as administrator.' }
  if (Get-Process -Name Resolve -ErrorAction SilentlyContinue) { throw 'Fully quit DaVinci Resolve before installing.' }
  $pluginParent = Join-Path $env:PROGRAMDATA 'Blackmagic Design\DaVinci Resolve\Support\Workflow Integration Plugins'
  $pluginTarget = Join-Path $pluginParent 'HNH_SFX_Finder_MRM'
  $previousVersion = $null
  $oldPackage = Join-Path $pluginTarget 'package.json'
  if (Test-Path -LiteralPath $oldPackage) { try { $previousVersion = (Get-Content -LiteralPath $oldPackage -Raw | ConvertFrom-Json).version } catch {} }
  $items = @('mrm-store.js','smart-audio.js','sfx-drag.js','auto-bin.js','thumbnail-decoder.js','maintenance.js','tag-manager.js','audio-export.js','changelog.js','release-history.json','free-tracks.js','timeline-range.js','main.js','av-insert.js','bulk-metadata.js','updater.js','library.js','library-types.js','date-filters.js','portable.js','visual-media.js','media-types.js','trim.js','preload.js','package.json','manifest.xml','WorkflowIntegration.node','UI','README.md','CHANGELOG.md')
  foreach ($item in $items) { if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot $item))) { throw "Missing release file: $item" } }
  if (Test-Path -LiteralPath $pluginTarget) {
    $backupRoot = Join-Path $env:PROGRAMDATA 'HNH-SFX-Finder-Backups'
    New-Item -ItemType Directory -Force -Path $backupRoot | Out-Null
    $backupTarget = Join-Path $backupRoot ('HNH_SFX_Finder_MRM-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
    Copy-Item -LiteralPath $pluginTarget -Destination $backupTarget -Recurse
    Write-Host "Previous plugin backed up to: $backupTarget"
  }
  New-Item -ItemType Directory -Force -Path $pluginTarget | Out-Null
  foreach ($item in $items) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $item) -Destination $pluginTarget -Recurse -Force }
  if ((Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'WorkflowIntegration.node')).Hash -ne (Get-FileHash -LiteralPath (Join-Path $pluginTarget 'WorkflowIntegration.node')).Hash) { throw 'Installed bridge checksum mismatch.' }
  $newVersion = (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'package.json') -Raw | ConvertFrom-Json).version
  @{ from = $previousVersion; to = $newVersion; installedAt = (Get-Date).ToString('o') } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $pluginTarget 'install-receipt.json') -Encoding UTF8
  Write-Host 'HNH SFX Finder (MRM) 0.12.3 installed beside the original plugin. Works standalone, or shares data with MRM when MRM is installed.'
  Write-Host 'Open Resolve Studio 20.2 > Workspace > Workflow Integrations > HNH SFX Finder (MRM).'
  exit 0
} catch {
  Write-Host ('INSTALL FAILED: ' + $_.Exception.Message) -ForegroundColor Red
  exit 1
}
