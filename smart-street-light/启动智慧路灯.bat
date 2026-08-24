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

rem ---------- [1/3] 启动 MySQL ----------
rem 若本机存在绿色版 D:\mysql，则自动启动；否则默认已装 MySQL 服务并自行运行
if exist "D:\mysql\bin\mysqld.exe" (
    echo [1/3] 启动 MySQL 数据库 ...
    start "MySQL" /min "D:\mysql\bin\mysqld.exe" --defaults-file=D:\mysql\my.ini
) else (
    echo [1/3] 使用本机已安装的 MySQL（请确认其服务已启动）...
)
timeout /t 3 /nobreak >nul

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
start "智慧路灯后端" /min "%PYTHON%" -m uvicorn main:app --app-dir "%~dp0backend" --host 127.0.0.1 --port 8000
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
