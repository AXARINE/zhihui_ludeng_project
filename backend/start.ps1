# 一键启动后端（PowerShell）
# 用法：在 backend 目录下执行 .\start.ps1

# 加载 .env 文件到当前进程的环境变量
Get-Content "$PSScriptRoot\.env" | Where-Object { $_ -notmatch '^\s*#' -and $_ -match '=' } | ForEach-Object {
    $parts = $_ -split '=', 2
    [Environment]::SetEnvironmentVariable($parts[0].Trim(), $parts[1].Trim(), 'Process')
}

Write-Host "环境变量已加载，启动后端..." -ForegroundColor Green
cargo run
