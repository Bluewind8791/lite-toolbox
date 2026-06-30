@echo off
setlocal
cd /d "%~dp0"

echo ============================================
echo  Lite Toolbox - 빌드 / 정리 스크립트
echo ============================================
echo.

echo [1/3] Tauri 빌드 중... (수 분 소요)
call npm run tauri build
if errorlevel 1 (
    echo.
    echo [오류] 빌드 실패. 정리 건너뜀.
    goto :end
)

echo.
echo [2/3] exe 를 프로젝트 루트로 이동...
move /Y "src-tauri\target\release\lite-toolbox.exe" "lite-toolbox.exe" >nul
if errorlevel 1 (
    echo [오류] exe 이동 실패.
    goto :end
)
echo   -^> lite-toolbox.exe

echo.
echo [3/3] 불필요한 빌드 산출물 정리...
if exist "src-tauri\target\release\bundle" rmdir /S /Q "src-tauri\target\release\bundle"
if exist "build" rmdir /S /Q "build"
echo   -^> NSIS 인스톨러(bundle), 프론트 출력(build) 삭제
echo   (target 컴파일 캐시는 유지 - 다음 빌드 빠름)

echo.
echo ============================================
echo  완료: %~dp0lite-toolbox.exe
echo ============================================

:end
echo.
pause
endlocal
