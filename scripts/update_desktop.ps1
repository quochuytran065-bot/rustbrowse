<#
.SYNOPSIS
    1-Click Local Build & Desktop Updater for RustBrowse (Windows 11, Tauri v2).
.DESCRIPTION
    1. Builds RustBrowse release binary + NSIS installer via `npx @tauri-apps/cli build`.
    2. Gracefully closes any running `rustbrowse.exe` instances.
    3. Runs silent NSIS installer (`/S`) if present, or updates local binary + Desktop shortcut.
    4. Relaunches RustBrowse automatically (unless `-NoLaunch` is specified).
.PARAMETER SkipBuild
    Skip `npx @tauri-apps/cli build` and install from existing `src-tauri\target\release` artifacts.
.PARAMETER NoLaunch
    Do not automatically launch RustBrowse after updating.
#>
[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$NoLaunch
)

$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Write-Host "========================================================" -ForegroundColor Cyan
Write-Host " RustBrowse 1-Click Desktop Updater" -ForegroundColor Cyan
Write-Host " Repo: $RepoRoot" -ForegroundColor DarkGray
Write-Host "========================================================" -ForegroundColor Cyan

# Configure local Rust toolchain paths if present
if (Test-Path 'D:\.rustup') { $env:RUSTUP_HOME = 'D:\.rustup' }
if (Test-Path 'D:\.cargo')  { $env:CARGO_HOME  = 'D:\.cargo' }
$extraPaths = @('D:\.cargo\bin', 'C:\ProgramData\mingw64\mingw64\bin') | Where-Object { Test-Path $_ }
if ($extraPaths.Count -gt 0) {
    $env:PATH = ($extraPaths -join ';') + ';' + $env:PATH
}

# Step 1: Build Tauri release + NSIS bundle
if (-not $SkipBuild) {
    Write-Host "`n[1/4] Building RustBrowse (npx @tauri-apps/cli build)..." -ForegroundColor Yellow
    Push-Location $RepoRoot
    $tempBuildConfig = $null
    try {
        $buildArgs = @('@tauri-apps/cli', 'build')
        if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY)) {
            Write-Host "      No TAURI_SIGNING_PRIVATE_KEY set; disabling updater artifact signing for local build." -ForegroundColor DarkGray
            $tempBuildConfig = Join-Path $env:TEMP "rustbrowse-local-build-$PID.json"
            [System.IO.File]::WriteAllText(
                $tempBuildConfig,
                '{"bundle":{"createUpdaterArtifacts":false}}',
                (New-Object System.Text.UTF8Encoding($false))
            )
            $buildArgs += @('--config', $tempBuildConfig)
        }
        & npx @buildArgs
        if ($LASTEXITCODE -ne 0) {
            throw "Tauri build failed with exit code $LASTEXITCODE"
        }
    } finally {
        if ($tempBuildConfig -and (Test-Path $tempBuildConfig)) {
            Remove-Item -Path $tempBuildConfig -Force -ErrorAction SilentlyContinue
        }
        Pop-Location
    }
} else {
    Write-Host "`n[1/4] Skipping build (-SkipBuild)." -ForegroundColor DarkGray
}

# Step 2: Gracefully close running rustbrowse.exe
Write-Host "`n[2/4] Closing running RustBrowse instances gracefully..." -ForegroundColor Yellow
$running = Get-Process -Name 'rustbrowse' -ErrorAction SilentlyContinue
if ($running) {
    foreach ($proc in $running) {
        try {
            if ($proc.MainWindowHandle -ne 0) {
                $null = $proc.CloseMainWindow()
            }
        } catch {}
    }

    $deadline = (Get-Date).AddSeconds(6)
    while ((Get-Date) -lt $deadline) {
        $still = Get-Process -Name 'rustbrowse' -ErrorAction SilentlyContinue
        if (-not $still) { break }
        Start-Sleep -Milliseconds 300
    }

    $remaining = Get-Process -Name 'rustbrowse' -ErrorAction SilentlyContinue
    if ($remaining) {
        Write-Host "      Forcing remaining process(es) to exit..." -ForegroundColor DarkYellow
        $remaining | Stop-Process -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 500
    }
    Write-Host "      Closed running RustBrowse." -ForegroundColor Green
} else {
    Write-Host "      No running RustBrowse instance found." -ForegroundColor DarkGray
}

# Step 3: Silent NSIS install OR direct executable + Desktop shortcut update
Write-Host "`n[3/4] Installing updated RustBrowse..." -ForegroundColor Yellow
$NsisDir = Join-Path $RepoRoot 'src-tauri\target\release\bundle\nsis'
$NsisInstaller = $null
if (Test-Path $NsisDir) {
    $NsisInstaller = Get-ChildItem -Path $NsisDir -Filter '*setup.exe' -File |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
}

$InstalledExe = Join-Path $env:LOCALAPPDATA 'RustBrowse\rustbrowse.exe'
$AltInstalledExe = Join-Path $env:LOCALAPPDATA 'Programs\RustBrowse\rustbrowse.exe'
$ReleaseExe = Join-Path $RepoRoot 'src-tauri\target\release\rustbrowse.exe'
$IconPath = Join-Path $RepoRoot 'src-tauri\icons\icon.ico'

if ($NsisInstaller) {
    Write-Host "      Running silent NSIS installer: $($NsisInstaller.Name)" -ForegroundColor Cyan
    $proc = Start-Process -FilePath $NsisInstaller.FullName -ArgumentList '/S' -Wait -PassThru
    if ($proc.ExitCode -ne 0) {
        throw "NSIS silent install failed with exit code $($proc.ExitCode)"
    }
    Write-Host "      NSIS silent installation completed." -ForegroundColor Green
} elseif (Test-Path $ReleaseExe) {
    Write-Host "      NSIS installer not found; updating local binary + Desktop shortcut directly..." -ForegroundColor DarkYellow
    $InstallDir = Join-Path $env:LOCALAPPDATA 'RustBrowse'
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $ReleaseExe -Destination $InstalledExe -Force

    # Create or update Desktop shortcut
    $DesktopDir = [Environment]::GetFolderPath('Desktop')
    $ShortcutPath = Join-Path $DesktopDir 'RustBrowse.lnk'
    $WshShell = New-Object -ComObject WScript.Shell
    $Shortcut = $WshShell.CreateShortcut($ShortcutPath)
    $Shortcut.TargetPath = $InstalledExe
    $Shortcut.WorkingDirectory = $InstallDir
    $Shortcut.Description = 'RustBrowse - Lightweight High-Performance Browser'
    if (Test-Path $IconPath) {
        $Shortcut.IconLocation = $IconPath
    }
    $Shortcut.Save()
    Write-Host "      Updated binary and Desktop shortcut: $ShortcutPath" -ForegroundColor Green
} else {
    throw "Neither NSIS installer nor release binary ($ReleaseExe) was found."
}

# Locate final executable to launch
$LaunchTarget = @($AltInstalledExe, $InstalledExe, $ReleaseExe) |
    Where-Object { Test-Path $_ } |
    Select-Object -First 1

# Step 4: Relaunch RustBrowse
if (-not $NoLaunch -and $LaunchTarget) {
    Write-Host "`n[4/4] Launching RustBrowse: $LaunchTarget" -ForegroundColor Green
    Start-Process -FilePath $LaunchTarget
} else {
    Write-Host "`n[4/4] Update complete." -ForegroundColor Green
}
