# Builds the Microsoft Store package (MSIX) from target\release\sora.exe.
# The identity values come from Partner Center > your app > Product management > Product identity.
# The package is left unsigned: the Store signs it when it is published.
#
#   .\msix\build-msix.ps1 -Name "12345Publisher.SoraNotch" -Publisher "CN=XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX" `
#       -PublisherDisplayName "Your name" -DisplayName "Sora Notch"
param(
  [Parameter(Mandatory)] [string] $Name,
  [Parameter(Mandatory)] [string] $Publisher,
  [Parameter(Mandatory)] [string] $PublisherDisplayName,
  [string] $DisplayName = 'Sora',
  [string] $Version,
  [string] $Exe,
  [string] $OutDir
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
if (-not $Exe) { $Exe = Join-Path $root 'target\release\sora.exe' }
if (-not $OutDir) { $OutDir = Join-Path (Split-Path $root -Parent) 'Store' }
if (-not (Test-Path $Exe)) { throw "Build Sora first (.\build.ps1): $Exe not found." }
if (-not $Version) {
  $line = Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "(\d+)\.(\d+)\.(\d+)"' | Select-Object -First 1
  $Version = '{0}.{1}.{2}.0' -f $line.Matches[0].Groups[1].Value, $line.Matches[0].Groups[2].Value, $line.Matches[0].Groups[3].Value
}
if ($Version -notmatch '^\d+\.\d+\.\d+\.0$') { throw 'The Store needs a version like 1.0.0.0 (the last number must be 0).' }

$makeappx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $makeappx) { throw 'makeappx.exe not found: install the Windows 10/11 SDK.' }

$stage = Join-Path $env:TEMP "sora-msix-$([guid]::NewGuid())"
New-Item -ItemType Directory -Force (Join-Path $stage 'Assets') | Out-Null
Copy-Item $Exe (Join-Path $stage 'Sora.exe')

# Logos from icons\icon.png (256 px), drawn with high-quality scaling; the wide tile centres the icon.
Add-Type -AssemblyName System.Drawing
$icon = [System.Drawing.Image]::FromFile((Join-Path $root 'icons\icon.png'))
function Save-Logo([string] $file, [int] $w, [int] $h, [int] $size) {
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.InterpolationMode = 'HighQualityBicubic'; $g.SmoothingMode = 'HighQuality'; $g.PixelOffsetMode = 'HighQuality'
  $g.Clear([System.Drawing.Color]::Transparent)
  $g.DrawImage($icon, [int](($w - $size) / 2), [int](($h - $size) / 2), $size, $size)
  $bmp.Save((Join-Path $stage "Assets\$file"), [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
}
Save-Logo 'StoreLogo.png' 50 50 50
Save-Logo 'Square44x44Logo.png' 44 44 44
Save-Logo 'Square150x150Logo.png' 150 150 120
Save-Logo 'Wide310x150Logo.png' 310 150 120
$icon.Dispose()

$xml = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'AppxManifest.template.xml'))
$escape = { param($v) [Security.SecurityElement]::Escape($v) }
$xml = $xml.Replace('{{NAME}}', (& $escape $Name)).Replace('{{PUBLISHER}}', (& $escape $Publisher)).Replace('{{VERSION}}', $Version)
$xml = $xml.Replace('{{DISPLAY_NAME}}', (& $escape $DisplayName)).Replace('{{PUBLISHER_DISPLAY_NAME}}', (& $escape $PublisherDisplayName))
[IO.File]::WriteAllText((Join-Path $stage 'AppxManifest.xml'), $xml, (New-Object Text.UTF8Encoding $false))

New-Item -ItemType Directory -Force $OutDir | Out-Null
$out = Join-Path $OutDir ("{0}_{1}_x64.msix" -f ($DisplayName -replace '[^A-Za-z0-9]', ''), $Version)
& $makeappx.FullName pack /d $stage /p $out /o
if ($LASTEXITCODE -ne 0) { throw "makeappx failed ($LASTEXITCODE)." }
Remove-Item -Recurse -Force $stage
Write-Host "Built: $out"
