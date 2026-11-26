# ============================================================================
# RustBrowse — One-Time GitHub Repository & Ed25519 Signing Setup
# ============================================================================
# Uses official GitHub CLI (`gh`) and `@tauri-apps/cli signer` so:
# 1. Your Ed25519 private signing key lives ONLY in `$HOME\.tauri\rustbrowse.key`
#    (OUTSIDE the Git repository — never committed, never printed).
# 2. GitHub Actions Secret `TAURI_SIGNING_PRIVATE_KEY` is uploaded securely.
# 3. `src-tauri/tauri.conf.json` receives ONLY the public key (`pubkey`) and
#    the exact HTTPS endpoint for your GitHub repository.
# ============================================================================

param(
    [string]$RepoName = "rustbrowse",
    [ValidateSet("public", "private")]
    [string]$Visibility = "public"
)

$ErrorActionPreference = "Stop"
$RootDir = Split-Path -Parent $PSScriptRoot
Set-Location $RootDir

Write-Host "==> [1/5] Checking GitHub CLI authentication..." -ForegroundColor Cyan
gh auth status | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw "GitHub CLI is not authenticated. Run 'gh auth login' once, then re-run this script."
}

$Owner = (gh api user --jq .login).Trim()
if ([string]::IsNullOrWhiteSpace($Owner)) {
    throw "Could not resolve authenticated GitHub username."
}
$FullRepo = "$Owner/$RepoName"
Write-Host "    Authenticated as: $Owner (Target repository: $FullRepo)" -ForegroundColor Green

Write-Host "==> [2/5] Ensuring Git repository & remote origin..." -ForegroundColor Cyan
if (-not (Test-Path (Join-Path $RootDir ".git"))) {
    git init -b main | Out-Null
}

$ErrorActionPreference = "Continue"
gh repo view $FullRepo *>$null
$RepoExists = ($LASTEXITCODE -eq 0)
$ErrorActionPreference = "Stop"

if (-not $RepoExists) {
    Write-Host "    Creating GitHub repository $FullRepo ($Visibility)..." -ForegroundColor Yellow
    gh repo create $FullRepo "--$Visibility" --description "RustBrowse — Lightweight Windows 11 Desktop Browser with Auto-Updater"
}

$RemoteUrl = "https://github.com/$FullRepo.git"
$ErrorActionPreference = "Continue"
$ExistingOrigin = (git remote get-url origin 2>$null)
$HasOrigin = ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($ExistingOrigin))
$ErrorActionPreference = "Stop"
if ($HasOrigin) {
    if ($ExistingOrigin -ne $RemoteUrl) {
        git remote set-url origin $RemoteUrl
    }
} else {
    git remote add origin $RemoteUrl
}
Write-Host "    Remote origin -> $RemoteUrl" -ForegroundColor Green

Write-Host "==> [3/5] Generating or loading Ed25519 Updater Signing Keypair (outside repo)..." -ForegroundColor Cyan
$KeyDir = Join-Path $env:USERPROFILE ".tauri"
New-Item -ItemType Directory -Path $KeyDir -Force | Out-Null
$PrivKeyPath = Join-Path $KeyDir "rustbrowse.key"
$PubKeyPath  = Join-Path $KeyDir "rustbrowse.key.pub"

if (-not (Test-Path $PrivKeyPath) -or -not (Test-Path $PubKeyPath)) {
    npx @tauri-apps/cli signer generate --ci -w $PrivKeyPath | Out-Null
    if (-not (Test-Path $PrivKeyPath) -or -not (Test-Path $PubKeyPath)) {
        throw "Failed to generate Tauri updater signing keypair."
    }
    Write-Host "    Generated new Ed25519 keypair in $KeyDir (never delete $PrivKeyPath!)" -ForegroundColor Green
} else {
    Write-Host "    Reusing existing signing keypair in $KeyDir" -ForegroundColor Green
}

$PubKey = (Get-Content $PubKeyPath -Raw).Trim()
if ([string]::IsNullOrWhiteSpace($PubKey)) {
    throw "Public key file $PubKeyPath is empty!"
}

Write-Host "==> [4/5] Updating public key & HTTPS release endpoint in src-tauri/tauri.conf.json..." -ForegroundColor Cyan
$TauriConfPath = Join-Path $RootDir "src-tauri\tauri.conf.json"
$Conf = Get-Content $TauriConfPath -Raw | ConvertFrom-Json
$Endpoint = "https://github.com/$FullRepo/releases/latest/download/latest.json"
$Conf.plugins.updater.pubkey = $PubKey
$Conf.plugins.updater.endpoints = @($Endpoint)
$Conf | ConvertTo-Json -Depth 10 | Set-Content -Path $TauriConfPath -Encoding UTF8
Write-Host "    Endpoint: $Endpoint" -ForegroundColor Green

Write-Host "==> [5/5] Uploading TAURI_SIGNING_PRIVATE_KEY to GitHub Actions Secrets..." -ForegroundColor Cyan
Get-Content $PrivKeyPath -Raw | gh secret set TAURI_SIGNING_PRIVATE_KEY --repo $FullRepo
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --body "" --repo $FullRepo
Write-Host "    GitHub Actions signing secrets configured securely!" -ForegroundColor Green
Write-Host "`nDONE: One-time setup complete. You can now run scripts\auto_push_release.ps1 anytime." -ForegroundColor Cyan
