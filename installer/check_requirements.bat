@echo off
title PICKLE VST3 - Requirements Checker
color 0B

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║           🥒 PICKLE VST3 - Requirements Checker              ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.

echo Checking build requirements...
echo.

REM Check Visual Studio
echo [1/3] Visual Studio 2022:
set "VS_FOUND=0"
for %%p in ("Community" "Professional" "Enterprise") do (
    if exist "C:\Program Files\Microsoft Visual Studio\2022\%%~p\VC\Auxiliary\Build\vcvars64.bat" (
        echo     [✓] Visual Studio 2022 %%~p installed
        set "VS_FOUND=1"
        goto :vs_check_done
    )
)
:vs_check_done
if "%VS_FOUND%"=="0" (
    echo     [✗] Visual Studio 2022 not found
    echo.
    echo     To install:
    echo     1. Download: https://visualstudio.microsoft.com/downloads/
    echo     2. Run installer
    echo     3. Select "Desktop development with C++"
    echo.
)

REM Check CMake
echo [2/3] CMake:
cmake --version >nul 2>&1
if errorlevel 1 (
    echo     [✗] CMake not found or not in PATH
    echo.
    echo     To install:
    echo     1. Download: https://cmake.org/download/
    echo     2. Run Windows x64 Installer
    echo     3. IMPORTANT: Check "Add CMake to system PATH"
    echo.
) else (
    for /f "tokens=3" %%a in ('cmake --version ^| findstr /r "cmake version"') do echo     [✓] CMake %%a found
)

REM Check Git (for VST3 SDK)
echo [3/3] Git:
git --version >nul 2>&1
if errorlevel 1 (
    echo     [✗] Git not found (needed for VST3 SDK)
    echo.
    echo     To install:
    echo     1. Download: https://git-scm.com/download/win
    echo     2. Run installer
    echo     3. Use default settings
    echo.
) else (
    for /f "tokens=3" %%a in ('git --version') do echo     [✓] Git %%a found
)

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║                      Next Steps                             ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.

echo If all requirements show [✓], run:
echo     setup_and_build.bat
echo.

echo If any show [✗], install the missing items first.
echo.

echo Quick download links:
echo - Visual Studio: https://visualstudio.microsoft.com/downloads/
echo - CMake: https://cmake.org/download/
echo - Git: https://git-scm.com/download/win
echo.

pause
