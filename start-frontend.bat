@echo off
rem ============================================
rem  Smart Street Light - Vue Frontend Launcher
rem  Uses the Node runtime bundled in this repo (node-win-x64).
rem  No Node installation or PATH config required.
rem  Auto-cleans stale vite processes and temp files before start.
rem  Prerequisite: backend already running on 8080 (see run.sh)
rem ============================================
title Smart Street Light - Vue Frontend
cd /d "%~dp0frontend_vue"

rem ---- put bundled node into PATH so npm/vite can find "node" ----
set "PATH=%~dp0node-win-x64;%PATH%"

set "NPM_CMD=%~dp0node-win-x64\npm.cmd"

if not exist "%NPM_CMD%" (
    echo [ERROR] Bundled Node runtime not found: node-win-x64
    echo Please keep the folder node-win-x64 next to this script.
    pause
    exit /b 1
)

rem ---- cleanup: kill process occupying 5173/5174 (stale vite) ----
for /f "tokens=5" %%a in ('netstat -ano ^| findstr ":5173 :5174" ^| findstr LISTENING') do (
    taskkill /F /PID %%a >nul 2>nul
)
rem ---- cleanup: remove vite config temp dir (Windows file lock fix) ----
if exist "%~dp0frontend_vue\node_modules\.vite-temp" rd /s /q "%~dp0frontend_vue\node_modules\.vite-temp"

echo ==========================================
echo   Smart Street Light Vue Frontend
echo   URL:  http://localhost:5173
echo   Press Ctrl+C to stop
echo ==========================================
"%NPM_CMD%" run dev
pause
