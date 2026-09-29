@echo off
setlocal DisableDelayedExpansion
rem Do not rely on PATH: Windows PowerShell can be installed but absent from PATH.
set "HNH_PS_EXE=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
rem A 32-bit launcher on 64-bit Windows must use the native system directory.
if exist "%SystemRoot%\Sysnative\WindowsPowerShell\v1.0\powershell.exe" set "HNH_PS_EXE=%SystemRoot%\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
if not exist "%~dp0install_windows.ps1" goto missing_script
if not exist "%HNH_PS_EXE%" goto missing_powershell
"%HNH_PS_EXE%" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0install_windows.ps1"
set "HNH_INSTALL_RESULT=%ERRORLEVEL%"
goto finish

:missing_script
echo INSTALL FAILED: install_windows.ps1 was not found beside this BAT file.
echo Extract the entire ZIP first. Keep both installer files in the plugin folder.
set "HNH_INSTALL_RESULT=2"
goto finish

:missing_powershell
echo INSTALL FAILED: Windows PowerShell was not found at its system location.
echo Expected: "%HNH_PS_EXE%"
echo Restore Windows PowerShell, then run this installer again.
set "HNH_INSTALL_RESULT=3"

:finish
echo.
pause
exit /b %HNH_INSTALL_RESULT%
