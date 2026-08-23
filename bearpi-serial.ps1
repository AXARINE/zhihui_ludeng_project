# BearPi-HM Nano 串口日志查看器
# 用法: pwsh -File bearpi-serial.ps1            # 默认 COM4, 115200
#       pwsh -File bearpi-serial.ps1 -Com 5     # 换 COM 号
# 退出: Ctrl+C
param([int]$Com = 4, [int]$Baud = 115200)

$p = New-Object System.IO.Ports.SerialPort "COM$Com", $Baud, None, 8, One
$p.ReadTimeout = 1000
try { $p.Open() } catch {
    Write-Host "打开 COM$Com 失败: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host "可能原因: 串口被 HiBurn/其他工具占用, 或设备管理器里 COM 号不是 $Com" -ForegroundColor Yellow
    exit 1
}
Write-Host "监听 COM$Com @ $Baud 波特率 (Ctrl+C 退出)" -ForegroundColor Green
Write-Host "现在按一下板子上的 RESET 键, 就能看到启动日志" -ForegroundColor Green
while ($true) {
    try { Write-Host $p.ReadLine() }
    catch [System.TimeoutException] { }
}
