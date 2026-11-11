<#
 Windows packages from the release executable (`tauri build --no-bundle`): the MSI installer and the portable ZIP.
 Usage: package.ps1 -Exe <ceh-desktop.exe> -License <LICENSE> -Icon <icon.ico> -Version <0.4.0-beta.1> -Out <folder>
 Needs the WiX 4 tool (`dotnet tool install --global wix`) for the MSI.
#>
param(
  [Parameter(Mandatory)][string]$Exe,
  [Parameter(Mandatory)][string]$License,
  [Parameter(Mandatory)][string]$Icon,
  [Parameter(Mandatory)][string]$Version,
  [Parameter(Mandatory)][string]$Out
)
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)(?:-beta\.(\d+))?$') { throw "Unsupported version: $Version" }
# Windows Installer versions are numeric: beta N becomes the fourth field (0.4.0-beta.2 -> 0.4.0.2).
$msiVersion = if ($Matches[4]) { "$($Matches[1]).$($Matches[2]).$($Matches[3]).$($Matches[4])" } else { "$($Matches[1]).$($Matches[2]).$($Matches[3])" }
foreach ($path in $Exe, $License, $Icon, "$PSScriptRoot\SantaCita.wxs") { if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing: $path" } }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$work = Join-Path ([IO.Path]::GetTempPath()) ("santacita-" + [guid]::NewGuid())
try {
  $payload = Join-Path $work 'payload'
  New-Item -ItemType Directory -Force -Path $payload | Out-Null
  Copy-Item -LiteralPath $Exe -Destination (Join-Path $payload 'SantaCita.exe')
  Copy-Item -LiteralPath $License -Destination (Join-Path $payload 'LICENSE.txt')

  $msi = Join-Path $Out "SantaCita-$Version-windows-x64.msi"
  & wix build "$PSScriptRoot\SantaCita.wxs" -arch x64 -d "Version=$msiVersion" -d "SourceDir=$payload" -d "IconFile=$((Resolve-Path $Icon).Path)" -pdbtype none -o $msi
  if ($LASTEXITCODE -ne 0) { throw 'wix build failed' }

  $portable = Join-Path $work 'portable\SantaCita'
  New-Item -ItemType Directory -Force -Path $portable | Out-Null
  Copy-Item "$payload\*" $portable
  New-Item -ItemType File -Path (Join-Path $portable 'portable.flag') | Out-Null   # data then lives in .\data beside the program
  $zip = Join-Path $Out "SantaCita-$Version-windows-x64-portable.zip"
  Remove-Item -LiteralPath $zip -ErrorAction SilentlyContinue
  # Written by hand so the entry names use `/` (Compress-Archive of Windows PowerShell 5.1 stores `\`).
  Add-Type -AssemblyName System.IO.Compression
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $parent = Join-Path $work 'portable'
  $archive = [IO.Compression.ZipFile]::Open($zip, 'Create')
  try {
    foreach ($file in Get-ChildItem (Join-Path $parent 'SantaCita') -Recurse -File) {
      $entry = $file.FullName.Substring($parent.Length + 1).Replace('\', '/')
      [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $file.FullName, $entry, [IO.Compression.CompressionLevel]::Optimal) | Out-Null
    }
  } finally { $archive.Dispose() }
  Get-ChildItem $Out -Filter "SantaCita-$Version-*" | ForEach-Object { "{0}  {1:N1} MB" -f $_.Name, ($_.Length / 1MB) }
}
finally { Remove-Item -Recurse -Force -LiteralPath $work -ErrorAction SilentlyContinue }
