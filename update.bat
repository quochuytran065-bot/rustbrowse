@echo off
setlocal
echo ========================================================
echo  RustBrowse Extreme — In-Place Atomic Updater
echo ========================================================

set DIST_DIR=D:\RustBrowse\dist\RustBrowse

echo [1/4] Kiem tra va dung tien trinh RustBrowse cu neu dang chay...
taskkill /f /im RustBrowse.exe >nul 2>&1

echo [2/4] Cap nhat ban build moi nhat tu D:\RustBrowse\target\release...
if exist "D:\RustBrowse\target\release\rustbrowse.exe" (
    copy /y "D:\RustBrowse\target\release\rustbrowse.exe" "%DIST_DIR%\RustBrowse.exe" >nul
    echo       Da ghi de ban binary moi thanh cong tai %DIST_DIR%\RustBrowse.exe.
) else (
    echo       Khong tim thay target release. Giu nguyen binary hien tai.
)

echo [3/4] Dong bo scripts va UI vao duy nhat thu muc %DIST_DIR%...
if not exist "%DIST_DIR%\scripts" mkdir "%DIST_DIR%\scripts"

if exist "D:\RustBrowse\scripts\ad_nuker.js" (
    copy /y "D:\RustBrowse\scripts\ad_nuker.js" "%DIST_DIR%\scripts\ad_nuker.js" >nul
)
if exist "D:\RustBrowse\scripts\web_optimizer.js" (
    copy /y "D:\RustBrowse\scripts\web_optimizer.js" "%DIST_DIR%\scripts\web_optimizer.js" >nul
)
if exist "D:\RustBrowse\scripts\ad_sweeper.js" (
    copy /y "D:\RustBrowse\scripts\ad_sweeper.js" "%DIST_DIR%\scripts\ad_sweeper.js" >nul
)
if exist "D:\RustBrowse\scripts\popup_guard.js" (
    copy /y "D:\RustBrowse\scripts\popup_guard.js" "%DIST_DIR%\scripts\popup_guard.js" >nul
)
if exist "D:\RustBrowse\scripts\media_adblock.js" (
    copy /y "D:\RustBrowse\scripts\media_adblock.js" "%DIST_DIR%\scripts\media_adblock.js" >nul
)
if exist "D:\RustBrowse\scripts\reader_mode.js" (
    copy /y "D:\RustBrowse\scripts\reader_mode.js" "%DIST_DIR%\scripts\reader_mode.js" >nul
)
if exist "D:\RustBrowse\scripts\element_zapper.js" (
    copy /y "D:\RustBrowse\scripts\element_zapper.js" "%DIST_DIR%\scripts\element_zapper.js" >nul
)
if exist "D:\RustBrowse\apps\rustbrowse\src\topbar.html" (
    copy /y "D:\RustBrowse\apps\rustbrowse\src\topbar.html" "%DIST_DIR%\topbar.html" >nul
)

echo [4/4] Hoan tat! Tat ca du lieu gom vao duy nhat 1 thu muc %DIST_DIR% (KHONG tao them thu muc phien ban rieng).
echo ========================================================
echo Khoi dong RustBrowse:
start "" "%DIST_DIR%\RustBrowse.exe"
pause
