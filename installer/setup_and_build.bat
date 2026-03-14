@echo off
title PICKLE VST3 - Setup and Build Assistant
color 0A

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║         🥒 PICKLE VST3 Plugin - Setup and Build Assistant      ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.

REM Check if Visual Studio is installed
echo [1/5] Checking for Visual Studio 2022...
set "VS_PATH="
for %%p in ("Community" "Professional" "Enterprise") do (
    if exist "C:\Program Files\Microsoft Visual Studio\2022\%%~p\VC\Auxiliary\Build\vcvars64.bat" (
        set "VS_PATH=C:\Program Files\Microsoft Visual Studio\2022\%%~p\VC\Auxiliary\Build\vcvars64.bat"
        echo [✓] Visual Studio 2022 %%~p found
        goto :vs_found
    )
)

echo [✗] Visual Studio 2022 not found!
echo.
echo Please install Visual Studio 2022 Community first:
echo 1. Download from: https://visualstudio.microsoft.com/downloads/
echo 2. Run installer and select "Desktop development with C++"
echo 3. After installation, run this script again
echo.
pause
exit /b 1

:vs_found

REM Check if CMake is available
echo [2/5] Checking for CMake...
cmake --version >nul 2>&1
if errorlevel 1 (
    echo [✗] CMake not found in PATH!
    echo.
    echo Please install CMake first:
    echo 1. Download from: https://cmake.org/download/
    echo 2. Run Windows x64 Installer
    echo 3. IMPORTANT: Check "Add CMake to system PATH"
    echo 4. After installation, run this script again
    echo.
    pause
    exit /b 1
) else (
    for /f "tokens=3" %%a in ('cmake --version ^| findstr /r "cmake version"') do set CMAKE_VERSION=%%a
    echo [✓] CMake %CMAKE_VERSION% found
)

REM Check if VST3 SDK is available
echo [3/5] Checking for VST3 SDK...
if exist "..\vst3sdk" (
    echo [✓] VST3 SDK found
) else (
    echo [!] VST3 SDK not found as submodule
    echo Attempting to clone VST3 SDK...
    git submodule add https://github.com/steinbergmedia/vst3sdk.git ..\vst3sdk
    if errorlevel 1 (
        echo [✗] Failed to clone VST3 SDK
        echo Please install Git and run this script again
        pause
        exit /b 1
    )
    echo [✓] VST3 SDK cloned successfully
)

REM Setup build environment
echo [4/5] Setting up build environment...
call "%VS_PATH%"
if errorlevel 1 (
    echo [✗] Failed to setup Visual Studio environment
    pause
    exit /b 1
)
echo [✓] Build environment ready

REM Build everything
echo [5/5] Building PICKLE VST3 Plugin...
echo.

REM Create build directories
if not exist "files" mkdir files
if not exist "build" mkdir build

cd build

echo Configuring project...
cmake ..\.. -G "Visual Studio 17 2022" -A x64
if errorlevel 1 (
    echo [✗] CMake configuration failed
    cd ..
    pause
    exit /b 1
)

echo.
echo Building plugin (Release)...
cmake --build . --config Release
if errorlevel 1 (
    echo [✗] Plugin build failed
    cd ..
    pause
    exit /b 1
)

REM Check if plugin was built
if not exist "Release\PICKLE.vst3" (
    echo [✗] Plugin file not found after build
    cd ..
    pause
    exit /b 1
)

echo [✓] Plugin built successfully!

REM Copy plugin to installer directory
copy "Release\PICKLE.vst3" "..\files\" >nul
if errorlevel 1 (
    echo [✗] Failed to copy plugin to installer directory
    cd ..
    pause
    exit /b 1
)

cd ..

REM Build installer executables
echo.
echo Building installer...
cl /EHsc /Fe:install.exe install.cpp /link user32.lib advapi32.lib shell32.lib
if errorlevel 1 (
    echo [✗] Installer build failed
    pause
    exit /b 1
)

echo Building uninstaller...
cl /EHsc /Fe:uninstall.exe uninstall.cpp /link user32.lib advapi32.lib shell32.lib
if errorlevel 1 (
    echo [✗] Uninstaller build failed
    pause
    exit /b 1
)

REM Create package
echo.
echo Creating installation package...
if not exist "package" mkdir package
copy install.exe package\ >nul
copy uninstall.exe package\ >nul
copy files\PICKLE.vst3 package\ >nul
copy ..\README.md package\ >nul

REM Create package README
echo PICKLE VST3 Plugin Installer > package\README.txt
echo ========================= >> package\README.txt
echo. >> package\README.txt
echo 1. Right-click install.exe and "Run as administrator" >> package\README.txt
echo 2. Follow the prompts to install the plugin >> package\README.txt
echo 3. Restart your DAW to use PICKLE >> package\README.txt
echo. >> package\README.txt
echo To uninstall: Run uninstall.exe as administrator >> package\README.txt
echo. >> package\README.txt
echo Version: 1.0.0 >> package\README.txt

echo.
echo ╔══════════════════════════════════════════════════════════════╗
echo ║                    🎉 BUILD SUCCESSFUL! 🎉                   ║
echo ╚══════════════════════════════════════════════════════════════╝
echo.
echo Your PICKLE VST3 plugin has been built and packaged!
echo.
echo Package contents:
dir package\ /B
echo.
echo Next steps:
echo 1. Navigate to: installer\package\
echo 2. Right-click install.exe and select "Run as administrator"
echo 3. Follow the prompts to install PICKLE
echo 4. Restart your DAW and enjoy! 🥒
echo.
echo Installation will:
echo - Detect existing PICKLE installations
echo - Handle updates and version management
echo - Install to the correct VST3 directory
echo - Create registry entries for tracking
echo.
pause
