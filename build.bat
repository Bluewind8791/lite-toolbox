@echo off
setlocal
cd /d "%~dp0"

echo ============================================
echo  Lite Toolbox - build ^& stage
echo ============================================
echo.

echo [1/3] Closing running app (release exe lock)...
taskkill /IM lite-toolbox.exe /F >nul 2>&1

echo [2/3] Tauri release build... (takes a while)
call npm run tauri build
if errorlevel 1 (
    echo.
    echo [ERROR] build failed. aborting.
    goto :end
)

echo.
echo [3/3] Move portable exe to repo root...
move /Y "src-tauri\target\release\lite-toolbox.exe" "lite-toolbox.exe" >nul
if errorlevel 1 (
    echo [ERROR] exe move failed.
    goto :end
)

echo.
echo ============================================
echo  Done.
echo    portable : %~dp0lite-toolbox.exe
echo    installer: src-tauri\target\release\bundle\nsis\Lite Toolbox_^<ver^>_x64-setup.exe
echo  (installer kept for release upload)
echo ============================================

:end
echo.
pause
endlocal
