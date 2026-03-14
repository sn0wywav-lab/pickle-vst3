@echo off
title PICKLE VST3 - After VS Installation
color 0A

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║        🥒 PICKLE VST3 - After Visual Studio Install         ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.

echo Checking if Visual Studio and CMake are properly installed...
echo.

REM Check Visual Studio
echo [1/4] Checking Visual Studio 2022...
set "VS_FOUND=0"
for %%p in ("Community" "Professional" "Enterprise") do (
    if exist "C:\Program Files\Microsoft Visual Studio\2022\%%~p\VC\Auxiliary\Build\vcvars64.bat" (
        echo [✓] Visual Studio 2022 %%~p found
        set "VS_PATH=C:\Program Files\Microsoft Visual Studio\2022\%%~p\VC\Auxiliary\Build\vcvars64.bat"
        set "VS_FOUND=1"
        goto :vs_check_done
    )
)

:vs_check_done
if "%VS_FOUND%"=="0" (
    echo [✗] Visual Studio 2022 not found
    echo.
    echo Please ensure:
    echo 1. Visual Studio 2022 is installed
    echo 2. "Desktop development with C++" workload was selected
    echo.
    echo Download: https://visualstudio.microsoft.com/downloads/
    pause
    exit /b 1
)

REM Check CMake
echo [2/4] Checking CMake...
cmake --version >nul 2>&1
if errorlevel 1 (
    echo [✗] CMake not found or not in PATH
    echo.
    echo Please ensure:
    echo 1. CMake is installed from cmake.org
    echo 2. "Add CMake to system PATH" was checked during installation
    echo.
    echo Download: https://cmake.org/download/
    pause
    exit /b 1
) else (
    for /f "tokens=3" %%a in ('cmake --version ^| findstr /r "cmake version"') do set CMAKE_VERSION=%%a
    echo [✓] CMake %CMAKE_VERSION% found
)

REM Test compiler
echo [3/4] Testing C++ compiler...
call "%VS_PATH%" >nul 2>&1
cl 2>nul
if errorlevel 1 (
    echo [✗] C++ compiler not working
    pause
    exit /b 1
) else (
    echo [✓] C++ compiler working
)

REM Check VST3 SDK
echo [4/4] Checking VST3 SDK...
if exist "..\vst3sdk" (
    echo [✓] VST3 SDK found
) else (
    echo [!] VST3 SDK not found as submodule
    echo Attempting to clone...
    git submodule add https://github.com/steinbergmedia/vst3sdk.git ..\vst3sdk 2>nul
    if errorlevel 1 (
        echo [!] Please install Git from: https://git-scm.com/download/win
        echo Then run: git submodule update --init --recursive
    ) else (
        echo [✓] VST3 SDK cloned
    )
)

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║                    🎉 Ready to Build!                        ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.
echo All requirements satisfied! Ready to build PICKLE VST3.
echo.
echo Choose your build method:
echo.
echo [A] Automated build (Recommended)
echo     - Builds everything automatically
echo     - Creates installer package
echo     - Easy and foolproof
echo.
echo [B] Manual build
echo     - Step by step control
echo     - Good for learning
echo     - More verbose output
echo.
echo [C] Test build only
echo     - Quick test compilation
echo     - Doesn't create installer
echo.
set /p choice="Choose build method (A/B/C): "

if /i "%choice%"=="A" (
    echo.
    echo Starting automated build...
    call setup_and_build.bat
) else if /i "%choice%"=="B" (
    echo.
    echo Starting manual build...
    call build_all.bat
) else if /i "%choice%"=="C" (
    echo.
    echo Starting test build...
    call "%VS_PATH%"
    mkdir build 2>nul
    cd build
    echo Configuring project...
    cmake ..\.. -G "Visual Studio 17 2022" -A x64
    if errorlevel 1 (
        echo [!] Configuration failed
        cd ..
        pause
        exit /b 1
    )
    echo.
    echo Building test...
    cmake --build . --config Release --config Release
    if exist "Release\PICKLE.vst3" (
        echo.
        echo [✓] Test build successful!
        echo Plugin created: build\Release\PICKLE.vst3
    ) else (
        echo [!] Test build failed
    )
    cd ..
    pause
) else (
    echo Invalid choice. Please run the script again.
)

echo.
echo Build complete! Check the results above.
echo.
echo If successful:
echo - Your plugin is ready to use
echo - Run installer\package\install.exe to install
echo - Restart your DAW to find PICKLE
echo.
echo If failed:
echo - Check error messages above
echo - Ensure all tools are properly installed
echo - Try running the script again
echo.
pause
