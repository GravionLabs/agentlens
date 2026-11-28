@echo off
rem deliver-loop for cmd.exe: runs deliver-loop.ps1 next to this file (written by skills-manage, Install-LoopShim.ps1)
pwsh -NoProfile -File "%~dp0deliver-loop.ps1" %*
exit /b %ERRORLEVEL%
