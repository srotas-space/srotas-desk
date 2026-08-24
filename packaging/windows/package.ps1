# Builds a release .exe (icon already embedded via build.rs + assets/icon.ico)
# and zips it for distribution. Run from a Windows machine with Rust installed.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..")

cargo build --release

New-Item -ItemType Directory -Force -Path "dist\windows" | Out-Null
Copy-Item "target\release\srotas-desk.exe" "dist\windows\srotas-desk.exe" -Force
Compress-Archive -Path "dist\windows\srotas-desk.exe" -DestinationPath "dist\windows\srotas-desk-windows.zip" -Force

Write-Host "Built: dist\windows\srotas-desk-windows.zip"
