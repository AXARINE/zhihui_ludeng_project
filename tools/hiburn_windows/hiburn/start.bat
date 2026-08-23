@setlocal enableextensions enabledelayedexpansion
@echo off
if %ERRORLEVEL% == 0 echo start burn! 
set binFile=%2
if x%binFile:allinone.bin=%==x%binFile% (goto burnbin)  else (goto allinone)

:allinone
echo allinone
echo HiBurn.exe -com:%1 -bin:%2 -signalbaud:%3 -2ms
HiBurn.exe -com:%1 -bin:%2 -signalbaud:%3 -2ms -show
if %ERRORLEVEL% == 0 echo Finished successfully!
goto end

:burnbin
echo burnbin
echo HiBurn.exe -com:%1 -bin:%2 -signalbaud:%3 -2ms -loader:.\bin\Hi3861_loader_boot.bin
HiBurn.exe -com:%1 -bin:%2 -signalbaud:%3 -2ms -loader:.\bin\Hi3861_loader_boot.bin -show
if %ERRORLEVEL% == 0 echo Finished successfully!

:end
endlocal