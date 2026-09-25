param(
  [Parameter(Mandatory = $false)]
  [string]$LibraryPath,

  [Parameter(Mandatory = $false)]
  [switch]$Prompt
)

$ErrorActionPreference = 'Stop'

if ($Prompt) {
  Add-Type -AssemblyName System.Windows.Forms
  $dialog = New-Object System.Windows.Forms.FolderBrowserDialog
  $dialog.Description = 'Choose where Grimoire should store your ebook library'
  $dialog.SelectedPath = Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'Grimoire'
  $dialog.ShowNewFolderButton = $true
  if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK -and $dialog.SelectedPath) {
    $LibraryPath = $dialog.SelectedPath
  }
}

if ([string]::IsNullOrWhiteSpace($LibraryPath)) {
  $LibraryPath = Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'Grimoire'
}

New-Item -ItemType Directory -Force -Path $LibraryPath | Out-Null

$cfgDir = Join-Path $env:APPDATA 'grimoire'
New-Item -ItemType Directory -Force -Path $cfgDir | Out-Null

$cfgPath = Join-Path $cfgDir 'config.json'
if (Test-Path -LiteralPath $cfgPath) {
  Write-Host "Grimoire config already exists at $cfgPath; leaving library_path unchanged."
  exit 0
}

# UTF-8 without BOM. Windows PowerShell 5.1's Set-Content -Encoding utf8 adds a
# BOM that serde_json rejects ("expected value at line 1 column 1").
$json = @{ library_path = $LibraryPath } | ConvertTo-Json
[System.IO.File]::WriteAllText($cfgPath, $json, [System.Text.UTF8Encoding]::new($false))
Write-Host "Wrote initial Grimoire library path: $LibraryPath"
