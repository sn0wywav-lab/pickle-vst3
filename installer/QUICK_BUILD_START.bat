@echo off
title PICKLE VST3 - Quick Build Start
color 0A

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║          🥒 PICKLE VST3 - Quick Build Start Guide           ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.

echo Since no C++ compiler is found, let's get you set up quickly!
echo.

echo OPTION 1: TDM-GCC (Easiest - 5 minutes)
echo ========================================
echo.
echo 1. Download: https://jmeubank.github.io/tdm-gcc/
echo 2. Click "Download" button
echo 3. Run the installer
echo 4. When prompted:
echo    - Select "64-bit" 
echo    - Check "Add to PATH"
echo    - Use default settings
echo 5. After installation, restart this script
echo.
echo OPTION 2: MSYS2 (More powerful - 10 minutes)  
echo ============================================
echo.
echo 1. Download: https://www.msys2.org/
echo 2. Run installer
echo 3. Open MSYS2 terminal
echo 4. Run: pacman -Syu
echo 5. Run: pacman -S mingw-w64-x86_64-gcc
echo 6. Add C:\msys64\mingw64\bin to PATH
echo.
echo OPTION 3: Visual Studio (Full setup - 1 hour)
echo ==========================================
echo.
echo 1. Download: https://visualstudio.microsoft.com/downloads/
echo 2. Visual Studio Community 2022 (Free)
echo 3. Select "Desktop development with C++"
echo 4. Install CMake from cmake.org
echo.
echo.
echo After installing any option above:
echo - Run this script again to build
echo - Or run: installer\build_all.bat
echo.

echo Which option do you want to try?
echo 1 = TDM-GCC (Recommended for quick start)
echo 2 = MSYS2 (More features)
echo 3 = Visual Studio (Full development)
echo.
set /p choice="Enter your choice (1-3): "

if "%choice%"=="1" (
    echo.
    echo Opening TDM-GCC download page...
    start https://jmeubank.github.io/tdm-gcc/
    echo.
    echo After installing TDM-GCC, run this script again!
) else if "%choice%"=="2" (
    echo.
    echo Opening MSYS2 download page...
    start https://www.msys2.org/
    echo.
    echo After installing MSYS2, run this script again!
) else if "%choice%"=="3" (
    echo.
    echo Opening Visual Studio download page...
    start https://visualstudio.microsoft.com/downloads/
    echo.
    echo After installing VS and CMake, run this script again!
) else (
    echo Invalid choice. Please run the script again.
)

echo.
echo Quick tips:
echo - TDM-GCC is fastest for just building
echo - MSYS2 gives you Linux-like tools  
echo - Visual Studio is best for development
echo.
pause
