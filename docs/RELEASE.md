# RustBrowse — Complete GitHub Auto-Push & Desktop Auto-Update Guide

RustBrowse uses **Tauri v2 Official Signed Updater (`tauri-plugin-updater`)** paired with the **SuperBoost Auto Update Engine** (`src-tauri/src/updater_engine.rs`) and **GitHub Actions CI/CD** so installed Windows desktops update seamlessly in the background without manual installer downloads or losing user sessions.

---

## 1. End-to-End Architecture

```text
Code Change -> Pre-Push Gates (fmt, clippy, tests, secret scan)
            -> Git Commit & Push -> GitHub Actions CI (.github/workflows/ci.yml)
            -> SemVer Bump & Tag (vX.Y.Z) -> Release Pipeline (.github/workflows/release.yml)
            -> Signed NSIS Setup + Ed25519 Signature + SHA256SUMS.txt + latest.json
            -> Installed RustBrowse Desktop detects new SemVer in background (HTTPS)
            -> Downloads & verifies Ed25519 signature in staging directory
            -> Safely applies update & restores open tabs / cookies / logins / history
```

---

## 2. One-Time Initial Setup (30 Seconds)

Run this **once** on your development machine (requires `gh auth login`, already authenticated):

```powershell
powershell -ExecutionPolicy Bypass -File D:\RustBrowse\scripts\setup_github_once.ps1 -RepoName rustbrowse -Visibility public
```

What this script sets up automatically:
1. Creates (or links) `https://github.com/<your-account>/rustbrowse` as `origin`.
2. Generates an **Ed25519 signing keypair** in `$HOME\.tauri\rustbrowse.key` (**outside** the repository so the private key is never committed to Git).
3. Writes **only the public key (`pubkey`)** and HTTPS endpoint `https://github.com/<your-account>/rustbrowse/releases/latest/download/latest.json` into `src-tauri/tauri.conf.json`.
4. Uploads `TAURI_SIGNING_PRIVATE_KEY` directly to **GitHub Actions Secrets** via `gh secret set`.

> **Security Rule:** Keep `$HOME\.tauri\rustbrowse.key` backed up safely. Never commit it to Git or paste it into chat. Installed copies of RustBrowse verify every update against the matching public key.

---

## 3. Automated Commit, Push & Release Commands

### Push tested changes (no new release tag)
```powershell
powershell -ExecutionPolicy Bypass -File D:\RustBrowse\scripts\auto_push_release.ps1 -Message "feat: improve video sniffer detection"
```

### Bump Semantic Version, push & publish a signed auto-update release
- **Patch (`1.0.0 -> 1.0.1`)** for bug fixes & optimizations:
  ```powershell
  powershell -ExecutionPolicy Bypass -File D:\RustBrowse\scripts\auto_push_release.ps1 -Message "release: v1.0.1 stability fixes" -Bump patch -PublishRelease
  ```
- **Minor (`1.0.1 -> 1.1.0`)** for backward-compatible features:
  ```powershell
  powershell -ExecutionPolicy Bypass -File D:\RustBrowse\scripts\auto_push_release.ps1 -Message "release: v1.1.0 new dashboard" -Bump minor -PublishRelease
  ```
- **Major (`1.1.0 -> 2.0.0`)** for breaking changes:
  ```powershell
  powershell -ExecutionPolicy Bypass -File D:\RustBrowse\scripts\auto_push_release.ps1 -Message "release: v2.0.0 major architecture upgrade" -Bump major -PublishRelease
  ```

---

## 4. In-App Release Dashboard & Rollback Protection

- Open **`rustbrowse://updates`** (or **Menu -> Trung tâm Cập nhật Tự động**) inside RustBrowse to inspect:
  - Installed version vs. Latest GitHub Release version
  - Live download progress (`%` and MB), Ed25519 signature verification status, and release notes
  - Release channel selector (**Stable** vs. **Beta**) and **Auto-install on exit** policy
- **Data Preservation:** User profile data lives in `%LOCALAPPDATA%\RustBrowse\User Data` (`session.db`, WebView2 cookies, saved logins, history, settings) and is never overwritten or wiped during updates.
- **Crash-Loop Rollback Guard:** Before applying a staged update, RustBrowse backs up the current executable to `%LOCALAPPDATA%\RustBrowse\User Data\UpdateStaging\rollback\` and tracks post-update boot stability. If 3 consecutive startup crashes occur on a version, that version is automatically blocked from re-installing.
