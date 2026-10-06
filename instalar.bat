@echo off
setlocal
title Hermes Agent - Instalador Windows
echo ================================================
echo   Hermes Agent - Instalador rapido Windows 10/11
echo   GLM (Z.AI) + Telegram + WhatsApp por equipo
echo ================================================
echo.
where pwsh >nul 2>nul
if %errorlevel%==0 (
  pwsh -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\instalar.ps1" %*
) else (
  powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\instalar.ps1" %*
)
echo.
pause
