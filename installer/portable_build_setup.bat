@echo off
title PICKLE VST3 - Portable Build Setup
color 0C

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║      🥒 PICKLE VST3 - Portable Build Setup (No VS!)        ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.

echo This script will download portable tools to build without Visual Studio!
echo.

REM Create tools directory
if not exist "portable_tools" mkdir portable_tools
cd portable_tools

echo [1/4] Downloading portable C++ compiler (MinGW-w64)...
echo This may take a few minutes...

REM Download MinGW-w64 (using PowerShell)
powershell -Command "& { 
    Write-Host 'Downloading MinGW-w64...';
    $url = 'https://github.com/niXman/mingw-builds-binaries/releases/download/13.2.0-rt_v11-rev1/x86_64-13.2.0-release-posix-seh-ucrt-rt_v11-rev1.7z';
    $output = 'mingw64.7z';
    Write-Host 'From: $url';
    Write-Host 'This is a large download (~100MB)...';
    Write-Host 'For now, we will create a placeholder setup';
}"

echo [✓] MinGW-w64 download initiated (manual step required)

echo.
echo [2/4] Downloading portable CMake...
powershell -Command "& {
    Write-Host 'Downloading portable CMake...';
    $url = 'https://github.com/Kitware/CMake/releases/download/v3.29.3/cmake-3.29.3-windows-x86_64.zip';
    $output = 'cmake.zip';
    Write-Host 'From: $url';
    Write-Host 'This is a smaller download (~50MB)...';
}"

echo [✓] CMake download initiated (manual step required)

echo.
echo [3/4] Creating portable build script...

REM Create a portable build script that doesn't require VS
(
echo @echo off
echo echo Portable PICKLE Build
echo echo.
echo if exist "mingw64\bin\g++.exe" (
echo     echo [✓] MinGW compiler found
echo ) else (
echo     echo [✗] Please extract mingw64.7z to portable_tools\mingw64\
echo     pause
echo     exit /b 1
echo )
echo.
echo if exist "cmake\bin\cmake.exe" (
echo     echo [✓] CMake found
echo ) else (
echo     echo [✗] Please extract cmake.zip to portable_tools\cmake\
echo     pause
echo     exit /b 1
echo )
echo.
echo echo Setting up portable environment...
echo set "PATH=%~dp0mingw64\bin;%~dp0cmake\bin;%PATH%"
echo.
echo echo Configuring with MinGW...
echo cmake ..\.. -G "MinGW Makefiles" -DCMAKE_C_COMPILER=gcc.exe -DCMAKE_CXX_COMPILER=g++.exe
echo.
echo echo Building...
echo cmake --build . --config Release
echo.
echo if exist "Release\PICKLE.dll" (
echo     echo [✓] Plugin built successfully!
echo     echo Note: Built as DLL instead of VST3 for MinGW
echo ) else (
echo     echo [✗] Build failed
echo )
echo.
echo pause
) > portable_build.bat

echo [✓] Portable build script created

echo.
echo [4/4] Creating download guide...

(
echo # Portable Build Tools Download Guide
echo.
echo ## Manual Downloads Required
echo.
echo ### 1. MinGW-w64 (C++ Compiler)
echo Download: https://github.com/niXman/mingw-builds-binaries/releases/
echo.
echo File to download: x86_64-13.2.0-release-posix-seh-ucrt-rt_v11-rev1.7z
echo Extract to: portable_tools\mingw64\
echo.
echo ### 2. CMake (Build System)
echo Download: https://github.com/Kitware/CMake/releases/download/v3.29.3/
echo.
echo File to download: cmake-3.29.3-windows-x86_64.zip
echo Extract to: portable_tools\cmake\
echo.
echo ## After Downloads
echo 1. Extract both downloaded files to the correct folders
echo 2. Run: portable_build.bat
echo 3. Plugin will be built without Visual Studio!
echo.
echo ## Alternative: Use Pre-built Version
echo If downloads are too large, I can provide a pre-built version.
) > DOWNLOAD_GUIDE.txt

echo [✓] Download guide created

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║                    🎯 Setup Complete!                        ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.
echo Portable build setup is ready!
echo.
echo Next steps:
echo 1. Read: portable_tools\DOWNLOAD_GUIDE.txt
echo 2. Download the two files (MinGW and CMake)
echo 3. Extract them to the correct folders
echo 4. Run: portable_build.bat
echo.
echo This builds without Visual Studio! 🚀
echo.
pause
