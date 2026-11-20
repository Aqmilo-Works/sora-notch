# Builds the GitHub edition (every feature) to target\release\sora.exe.
# The chat helpers (sora-wa.exe, sora-tg.exe, tdjson.dll, tg-key.json) are packed into Sora.exe when they are found in .\modules
# (they are never in the repository). Pass -Helpers <folder> to copy them from there first.
param([string]$Helpers = (Join-Path (Split-Path $PSScriptRoot -Parent) 'Sora-Full\modules'))
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
if ($Helpers -and (Test-Path $Helpers)) {
  New-Item -ItemType Directory -Force modules | Out-Null
  foreach ($f in 'sora-wa.exe', 'sora-tg.exe', 'tdjson.dll', 'tg-key.json') { if (Test-Path (Join-Path $Helpers $f)) { Copy-Item (Join-Path $Helpers $f) modules -Force } }
}
# Remapping keeps the builder's user folder and project path out of the executable (panic messages, debug path).
$env:RUSTFLAGS = "-C target-feature=+crt-static --remap-path-prefix=$env:USERPROFILE=~ --remap-path-prefix=$PSScriptRoot=sora -C link-arg=/PDBALTPATH:sora.pdb"
cargo build --release --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "Built: $PSScriptRoot\target\release\sora.exe"
