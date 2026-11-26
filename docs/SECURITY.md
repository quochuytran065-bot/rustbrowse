# Security Standards

## Boundaries
1. **Untrusted Web Content**: Every webpage is treated as hostile. No native bridge (`AddHostObjectToScript`) is exposed without strict origin checking.
2. **Chromium Sandboxing**:
   - Strictly forbidden flags: `--no-sandbox`, `--disable-web-security`, `--ignore-certificate-errors`.
   - TLS certificates validated with default platform checks.
3. **Privilege Separation**:
   - RustBrowse never requests or runs under elevated Administrator tokens.
   - User profile stays in `%LOCALAPPDATA%`.
4. **Crash Isolation**:
   - `ProcessFailed` handles renderer crashes cleanly by offering a reload without terminating the browser application.
