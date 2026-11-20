param(
  # Defaults to the version in Cargo.toml.
  [string]$Version = "",
  # Build Sora.exe first (.\build.ps1: path-remapped, chat helpers packed inside).
  [switch]$Build,
  # WiX 3.14 binaries (candle.exe, light.exe, WixUIExtension.dll): https://github.com/wixtoolset/wix3/releases (wix314-binaries.zip), unzipped.
  [string]$Wix = (Join-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) 'wix3\bin')
)
# Builds dist\SoraNotch-<version>-x64.msi with WiX: a per-user installer (no administrator rights) with a Welcome > Destination
# folder > Install > Finish wizard. Installs to %LOCALAPPDATA%\Programs\Sora: just Sora.exe (the chat helpers, converter engines,
# cities list and fonts are inside it) and a Start menu shortcut. The same app data folder (%APPDATA%\com.nook.desktop) is used as by
# the portable Sora.exe, so settings carry over; the installed copy takes over "Start with Windows" itself (startup::adopt_installed).
# A newer package replaces an older one; an older one refuses to install over a newer. A running Sora is closed first. Uninstall
# removes the program, the shortcut, the "Start with Windows" entry when it starts this installed copy, and all of Sora's data
# (%APPDATA% and %LOCALAPPDATA%\com.nook.desktop), so a new install starts fresh.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

if (-not $Version) { $Version = (Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value }
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Version must look like 1.2.3 (got '$Version')" }
foreach ($t in 'candle.exe', 'light.exe', 'WixUIExtension.dll', 'WixUtilExtension.dll') { if (-not (Test-Path (Join-Path $Wix $t))) { throw "WiX not found in $Wix (missing $t). Unzip wix314-binaries.zip there or pass -Wix." } }

if ($Build) {
  & (Join-Path $root 'build.ps1')
  if ($LASTEXITCODE -ne 0) { throw "build failed" }
}
$exe = Join-Path $root 'target\release\sora.exe'
if (-not (Test-Path $exe)) { throw "No $exe (run with -Build)" }
# The builder's user name must not be inside the package (build.ps1 remaps paths; this proves it).
$text = [Text.Encoding]::GetEncoding(28591).GetString([IO.File]::ReadAllBytes($exe))
foreach ($p in @("Users\$env:USERNAME", "Users/$env:USERNAME", (Split-Path (Split-Path $root -Parent) -Leaf))) {
  foreach ($pattern in @($p, ($p.ToCharArray() -join "`0"))) {
    if ($text.IndexOf($pattern, [StringComparison]::OrdinalIgnoreCase) -ge 0) { throw "$exe still contains '$p'. Build with .\build.ps1." }
  }
}
$text = $null

$work = Join-Path $root 'target\msi'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work, (Join-Path $root 'dist') | Out-Null

# Wizard pictures: installer\art\*@3x.png (made from art\glow.html), scaled down to the sizes Windows Installer expects.
Add-Type -AssemblyName System.Drawing
function Save-Bmp([string]$file, [int]$w, [int]$h, [scriptblock]$paint) {
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.InterpolationMode = 'HighQualityBicubic'; $g.SmoothingMode = 'HighQuality'; $g.PixelOffsetMode = 'HighQuality'
  $g.Clear([System.Drawing.Color]::White)
  & $paint $g
  $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Bmp); $g.Dispose(); $bmp.Dispose()
}
$welcome = [System.Drawing.Image]::FromFile((Join-Path $PSScriptRoot 'art\welcome@3x.png'))
$bannerArt = [System.Drawing.Image]::FromFile((Join-Path $PSScriptRoot 'art\banner@3x.png'))
$banner = Join-Path $work 'banner.bmp'; $dialog = Join-Path $work 'dialog.bmp'
Save-Bmp $banner 493 58 { param($g) $g.DrawImage($bannerArt, 0, 0, 493, 58) }
Save-Bmp $dialog 493 312 { param($g) $g.DrawImage($welcome, 0, 0, 164, 312) }
$welcome.Dispose(); $bannerArt.Dispose()

$msi = Join-Path $root "dist\SoraNotch-$Version-x64.msi"
Remove-Item $msi -ErrorAction SilentlyContinue
& (Join-Path $Wix 'candle.exe') -nologo -arch x64 -ext (Join-Path $Wix 'WixUIExtension.dll') -ext (Join-Path $Wix 'WixUtilExtension.dll') "-dVersion=$Version" "-dSoraExe=$exe" "-dIcon=$root\icons\icon.ico" "-dBanner=$banner" "-dDialog=$dialog" -out "$work\Sora.wixobj" (Join-Path $PSScriptRoot 'Sora.wxs')
if ($LASTEXITCODE -ne 0) { throw "candle failed" }
# ICE38/64/91 complain about per-user components with registry key paths in the profile, which is the intended design.
& (Join-Path $Wix 'light.exe') -nologo -ext (Join-Path $Wix 'WixUIExtension.dll') -ext (Join-Path $Wix 'WixUtilExtension.dll') -cultures:en-us -sice:ICE38 -sice:ICE64 -sice:ICE91 -out $msi "$work\Sora.wixobj"
if ($LASTEXITCODE -ne 0) { throw "light failed" }
"Built $msi ($([math]::Round((Get-Item $msi).Length / 1MB, 1)) MB)"
