# ============================================================================
# RustBrowse — Automated Gate, Commit, Push & Semantic Release Pipeline
# ============================================================================
# Performs all mandatory safety checks before committing & pushing to GitHub:
# 1. Secret & binary leak scan
# 2. Optional Semantic Version bump (patch | minor | major | X.Y.Z)
# 3. Rust formatting (`cargo fmt`), linter (`cargo clippy`), and `cargo test`
# 4. JavaScript syntax verification (`node --check`)
# 5. Conventional Commit + safe push (never force-pushes; falls back to PR if branch protected)
# 6. Annotated `vX.Y.Z` tag push to trigger signed GitHub Actions Release + Auto-Update
# ============================================================================

param(
    [Parameter(Mandatory = $true)]
    [string]$Message,

    [string]$Bump = "none",

    [switch]$PublishRelease
)

$ErrorActionPreference = "Stop"
$RootDir = Split-Path -Parent $PSScriptRoot
Set-Location $RootDir

# Ensure Rust/MinGW toolchain on D:\ is in PATH if present
if (Test-Path "D:\.cargo\bin") {
    $env:RUSTUP_HOME = "D:\.rustup"
    $env:CARGO_HOME  = "D:\.cargo"
    $env:PATH        = "D:\.cargo\bin;C:\ProgramData\mingw64\mingw64\bin;$env:PATH"
}

if ($Message -notmatch '^(feat|fix|perf|ui|security|release|docs|chore|refactor|test)(\([^)]+\))?:\s+.+') {
    throw "Commit message must follow Conventional Commits (feat:|fix:|perf:|ui:|security:|release: ...). Got: $Message"
}

if ($Bump -ne "none") {
    Write-Host "==> Bumping Semantic Version ($Bump)..." -ForegroundColor Cyan
    node scripts/bump_version.mjs $Bump
}

Write-Host "==> Checking version synchronization..." -ForegroundColor Cyan
node scripts/bump_version.mjs --check

Write-Host "==> Formatting & linting Rust code..." -ForegroundColor Cyan
Push-Location (Join-Path $RootDir "src-tauri")
try {
    cargo fmt --all
    cargo clippy --all-targets -- -D warnings
    Write-Host "==> Running Rust unit tests..." -ForegroundColor Cyan
    cargo test
} finally {
    Pop-Location
}

Write-Host "==> Checking JavaScript syntax..." -ForegroundColor Cyan
node --check ui/app.js
Get-ChildItem src-tauri/scripts/*.js | ForEach-Object {
    node --check $_.FullName
}

Write-Host "==> Staging source changes & scanning for forbidden files/secrets..." -ForegroundColor Cyan
git add -A

$StagedFiles = git diff --cached --name-only
$Forbidden = $StagedFiles | Where-Object { $_ -match '(\.env($|\.)|\.pem$|\.pfx$|\.key$|id_rsa|id_ed25519|\.exe$|\.dll$|\.pdb$|\.msi$)' }
if ($Forbidden) {
    git reset | Out-Null
    throw "ABORTED: Forbidden binary or secret file staged: $($Forbidden -join ', ')"
}

$DiffText = git diff --cached
if ($DiffText -match '(PRIVATE KEY-----|ghp_[A-Za-z0-9]{30,}|gho_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,})') {
    # Exclude regex definitions inside workflow/script files themselves
    $NonMetaSecret = git diff --cached -- . ':!.github/workflows/security-audit.yml' ':!scripts/auto_push_release.ps1'
    if ($NonMetaSecret -match '(PRIVATE KEY-----|ghp_[A-Za-z0-9]{30,}|gho_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,})') {
        git reset | Out-Null
        throw "ABORTED: Potential secret or private signing key detected in staged diff!"
    }
}

if ($StagedFiles) {
    git commit -m $Message
} else {
    Write-Host "    Working tree clean (no new file changes to commit)." -ForegroundColor Yellow
}

$OriginUrl = git remote get-url origin 2>$null
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($OriginUrl)) {
    throw "Git remote 'origin' is not configured yet. Run scripts\setup_github_once.ps1 once to connect GitHub."
}

$CurrentBranch = (git rev-parse --abbrev-ref HEAD).Trim()
Write-Host "==> Pushing branch '$CurrentBranch' to origin ($OriginUrl)..." -ForegroundColor Cyan
git push -u origin $CurrentBranch
if ($LASTEXITCODE -ne 0) {
    $FallbackBranch = "feat/auto-" + (Get-Date -Format "yyyyMMdd-HHmmss")
    Write-Host "    Direct push rejected (protected branch). Pushing to '$FallbackBranch' and opening PR..." -ForegroundColor Yellow
    git checkout -b $FallbackBranch
    git push -u origin $FallbackBranch
    gh pr create --fill --head $FallbackBranch
    exit 0
}

if ($PublishRelease -or $Bump -ne "none") {
    $TauriConf = Get-Content "src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json
    $Ver = $TauriConf.version
    $Tag = "v$Ver"

    $ExistingTag = git tag -l $Tag
    if (-not $ExistingTag) {
        Write-Host "==> Creating & pushing signed release tag $Tag..." -ForegroundColor Cyan
        git tag -a $Tag -m "Release $Tag"
        git push origin $Tag
        Write-Host "    Tag $Tag pushed! GitHub Actions is now building & publishing the signed auto-update." -ForegroundColor Green
    } else {
        Write-Host "    Tag $Tag already exists locally; skipping duplicate tag." -ForegroundColor Yellow
    }
}
