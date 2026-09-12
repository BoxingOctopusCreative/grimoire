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

@{ library_path = $LibraryPath } | ConvertTo-Json | Set-Content -LiteralPath $cfgPath -Encoding utf8
Write-Host "Wrote initial Grimoire library path: $LibraryPath"
