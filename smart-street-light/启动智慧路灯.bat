@echo off
chcp 65001 >nul
rem =====================================================================
rem  智慧路灯系统 - 一键启动
rem  自动检测 Python、用脚本所在目录定位 backend，克隆到任意目录均可运行
rem =====================================================================
title 智慧路灯系统 - 一键启动
cd /d "%~dp0"

echo ==========================================
echo    智慧路灯系统 一键启动
echo ==========================================
echo.

rem ---------- [1/3] 启动 PostgreSQL ----------
rem 若本机存在绿色版 D:\pgsql，则自动启动；否则请确认已装 PostgreSQL 并自行启动
if exist "D:\pgsql\pgsql\bin\pg_ctl.exe" (
    "D:\pgsql\pgsql\bin\pg_ctl.exe" -D "D:\pgsql\data" status >nul 2>nul
    if errorlevel 1 (
        echo [1/3] 启动 PostgreSQL ...
        "D:\pgsql\pgsql\bin\pg_ctl.exe" -D "D:\pgsql\data" -l "D:\pgsql\logfile.txt" -w start
    ) else (
        echo [1/3] PostgreSQL 已在运行 ...
    )
) else (
    echo [1/3] 未找到 PostgreSQL，请确认已安装并启动（默认 127.0.0.1:5432）...
)
timeout /t 1 /nobreak >nul

rem ---------- [2/3] 自动检测 Python ----------
set "PYTHON="
python --version >nul 2>nul && set "PYTHON=python"
if not defined PYTHON if exist "%LOCALAPPDATA%\Programs\Python\Python312\python.exe" set "PYTHON=%LOCALAPPDATA%\Programs\Python\Python312\python.exe"
if not defined PYTHON if exist "%LOCALAPPDATA%\Programs\Python\Python313\python.exe" set "PYTHON=%LOCALAPPDATA%\Programs\Python\Python313\python.exe"
if not defined PYTHON if exist "%LOCALAPPDATA%\Programs\Python\Python311\python.exe" set "PYTHON=%LOCALAPPDATA%\Programs\Python\Python311\python.exe"

if not defined PYTHON (
    echo [错误] 未找到 Python，请先安装 Python 3.11+ 并勾选 "Add Python to PATH"
    pause
    exit /b 1
)

rem 结束占用 8000 端口的旧后端，避免端口冲突导致旧代码继续运行
for /f "tokens=5" %%a in ('netstat -ano ^| findstr :8000 ^| findstr LISTENING') do taskkill /F /PID %%a >nul 2>nul

echo [2/3] 启动后端服务 ...
start "智慧路灯后端" /min "%PYTHON%" -m uvicorn main:app --app-dir "%~dp0backend" --host 0.0.0.0 --port 8000
timeout /t 4 /nobreak >nul

rem ---------- [3/3] 打开管理页面 ----------
echo [3/3] 打开管理页面 ...
start "" http://127.0.0.1:8000/

echo.
echo 启动完成：管理页面 http://127.0.0.1:8000/
echo 接口文档   http://127.0.0.1:8000/docs
echo 本窗口可关闭，服务在后台运行。
echo.
pause
