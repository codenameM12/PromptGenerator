param(
    [string]$ReleaseDir = "src-tauri\target\release",
    [string]$OutDir = "dist-portable"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$name = "Prompt Studio"
$target = Join-Path $OutDir $name
if (Test-Path $target) { Remove-Item $target -Recurse -Force }
New-Item -ItemType Directory -Path $target -Force | Out-Null

$files = @(
    @{ Source = Join-Path $ReleaseDir "prompt-studio.exe"; Name = "Prompt Studio.exe" },
    @{ Source = Join-Path $ReleaseDir "ffmpeg.exe"; Name = "ffmpeg.exe" },
    @{ Source = Join-Path $ReleaseDir "ffprobe.exe"; Name = "ffprobe.exe" },
    @{ Source = "src-tauri\binaries\FFMPEG-NOTICE.txt"; Name = "FFMPEG-NOTICE.txt" },
    @{ Source = "README.md"; Name = "README.md" }
)

foreach ($file in $files) {
    if (-not (Test-Path $file.Source)) {
        throw "Missing release file: $($file.Source). Run npm run tauri build first."
    }
    Copy-Item $file.Source (Join-Path $target $file.Name)
}

# Presence of this marker switches AppPaths to <exe>/data.
New-Item -ItemType File -Path (Join-Path $target "portable.txt") -Force | Out-Null
@"
Prompt Studio $((Get-Content package.json | ConvertFrom-Json).version)
Portable build. Data, cache, config, and the database are stored in .\data.
Double-click "Prompt Studio.exe" to launch.
"@ | Set-Content (Join-Path $target "PORTABLE-README.txt")

$zip = Join-Path $OutDir "Prompt Studio_0.1.0_x64-portable.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path "$target\*" -DestinationPath $zip
Write-Host "Portable folder: $target"
Write-Host "Portable zip:    $zip"
