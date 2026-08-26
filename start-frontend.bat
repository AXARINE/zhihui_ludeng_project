@echo off
rem ============================================
rem  Smart Street Light - Vue Frontend Launcher
rem  Uses the Node runtime bundled in this repo (node-win-x64).
rem  No Node installation or PATH config required.
rem  Prerequisite: backend already running on 8080 (see run.sh)
rem ============================================
title Smart Street Light - Vue Frontend
cd /d "%~dp0frontend_vue"

set "NPM_CMD=%~dp0node-win-x64\npm.cmd"

if not exist "%NPM_CMD%" (
    echo [ERROR] Bundled Node runtime not found: node-win-x64
    echo Please keep the folder node-win-x64 next to this script.
    pause
    exit /b 1
)

echo ==========================================
echo   Smart Street Light Vue Frontend
echo   URL:  http://localhost:5173
echo   Press Ctrl+C to stop
echo ==========================================
"%NPM_CMD%" run dev
pause
