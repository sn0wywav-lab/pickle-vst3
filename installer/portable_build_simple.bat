@echo off
title PICKLE VST3 - Portable Build (Simple)
color 0B

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║      🥒 PICKLE VST3 - Portable Build (No Visual Studio!)      ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.

echo This will help you build PICKLE without installing Visual Studio!
echo.

REM Create portable tools directory
if not exist "portable_tools" mkdir portable_tools
cd portable_tools

echo [1/3] Setting up portable build environment...

REM Create a simple build script that works with common tools
(
echo @echo off
echo echo Portable PICKLE Build Attempt
echo echo.
echo echo Checking for common compilers...
echo.
echo if exist "C:\msys64\mingw64\bin\g++.exe" (
echo     echo [✓] MSYS2 MinGW found
echo     set "PATH=C:\msys64\mingw64\bin;%PATH%"
echo     goto :found_compiler
echo )
echo.
echo if exist "C:\mingw64\bin\g++.exe" (
echo     echo [✓] MinGW-w64 found
echo     set "PATH=C:\mingw64\bin;%PATH%"
echo     goto :found_compiler
echo )
echo.
echo if exist "C:\TDM-GCC-64\bin\g++.exe" (
echo     echo [✓] TDM-GCC found
echo     set "PATH=C:\TDM-GCC-64\bin;%PATH%"
echo     goto :found_compiler
echo )
echo.
echo echo [!] No C++ compiler found
echo echo.
echo echo Please install one of these:
echo echo 1. MSYS2: https://www.msys2.org/
echo echo 2. TDM-GCC: https://jmeubank.github.io/tdm-gcc/
echo echo 3. Or use Visual Studio option
echo echo.
echo pause
echo exit /b 1
echo.
echo :found_compiler
echo echo [✓] Compiler found!
echo g++ --version
echo.
echo echo Checking for CMake...
echo cmake --version 2>nul
echo if errorlevel 1 (
echo     echo [✗] CMake not found
echo     echo Please download CMake from: https://cmake.org/download/
echo     echo Add it to PATH during installation
echo     pause
echo     exit /b 1
echo ) else (
echo     echo [✓] CMake found!
echo )
echo.
echo echo Attempting to build PICKLE...
echo cd ..\..
echo mkdir build 2>nul
echo cd build
echo.
echo echo Configuring with portable compiler...
echo cmake .. -G "MinGW Makefiles" -DCMAKE_C_COMPILER=gcc.exe -DCMAKE_CXX_COMPILER=g++.exe
echo.
echo if errorlevel 1 (
echo     echo [!] CMake configuration failed
echo     echo Trying alternative approach...
echo     goto :alternative_build
echo )
echo.
echo echo Building plugin...
echo cmake --build . --config Release
echo.
echo if exist "Release\PICKLE.dll" (
echo     echo [✓] Build successful!
echo     echo Plugin created as PICKLE.dll
echo     echo Note: May need conversion to VST3 format
echo ) else if exist "Release\PICKLE.exe" (
echo     echo [✓] Build successful!
echo     echo Executable created
echo ) else (
echo     echo [!] Build failed
echo     goto :alternative_build
echo )
echo goto :end
echo.
echo :alternative_build
echo echo.
echo echo Alternative: Direct compilation attempt...
echo g++ -std=c++17 -I../vst3sdk -I../vst3sdk/pluginterfaces -I../vst3sdk/public.sdk/source ^
echo     ../source/pickle_processor.cpp ../source/pickle_controller.cpp ^
echo     ../source/pickle_factory.cpp ../source/pickle_entry.cpp ^
echo     -shared -o PICKLE.dll -static-libgcc -static-libstdc++
echo.
echo if exist "PICKLE.dll" (
echo     echo [✓] Direct compilation successful!
echo ) else (
echo     echo [✗] All build attempts failed
echo )
echo.
echo :end
echo echo.
echo echo Build attempt complete!
echo pause
) > portable_build.bat

echo [✓] Portable build script created

echo.
echo [2/3] Creating download guide...
(
echo # Portable Build Tools Guide
echo.
echo ## Quick Options for C++ Compiler (Choose ONE):
echo.
echo ### Option 1: MSYS2 (Recommended)
echo 1. Download: https://www.msys2.org/
echo 2. Run installer
echo 3. In MSYS2 terminal: pacman -S mingw-w64-x86_64-gcc
echo 4. Add C:\msys64\mingw64\bin to PATH
echo.
echo ### Option 2: TDM-GCC (Easiest)
echo 1. Download: https://jmeubank.github.io/tdm-gcc/
echo 2. Run installer
echo 3. Choose "64-bit" and "Add to PATH"
echo.
echo ### Option 3: MinGW-w64 (Manual)
echo 1. Download from: https://github.com/niXman/mingw-builds-binaries/releases/
echo 2. Extract to C:\mingw64\
echo 3. Add C:\mingw64\bin to PATH
echo.
echo ## CMake (Required):
echo 1. Download: https://cmake.org/download/
echo 2. Windows x64 Installer
echo 3. IMPORTANT: Check "Add CMake to system PATH"
echo.
echo ## After Installation:
echo 1. Run: portable_build.bat
echo 2. It will detect your compiler and build
echo.
echo ## Alternative: Use Chocolatey (if available)
echo choco install cmake mingw
echo.
echo ## Alternative: Use Scoop (if available)
echo scoop install cmake gcc
) > DOWNLOAD_GUIDE.txt

echo [✓] Download guide created

echo.
echo [3/3] Creating test script...
(
echo @echo off
echo echo Testing portable build environment...
echo echo.
echo echo Testing compiler...
echo g++ --version 2>nul
echo if errorlevel 1 (
echo     echo [✗] No C++ compiler found
echo ) else (
echo     echo [✓] C++ compiler available
echo )
echo.
echo echo Testing CMake...
echo cmake --version 2>nul
echo if errorlevel 1 (
echo     echo [✗] CMake not found
echo ) else (
echo     echo [✓] CMake available
echo )
echo.
echo echo Ready to build!
echo pause
) > test_environment.bat

echo [✓] Test script created

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║                    🎯 Setup Complete!                        ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.
echo Portable build setup is ready!
echo.
echo Created files:
echo - portable_build.bat     (Main build script)
echo - test_environment.bat   (Test your setup)
echo - DOWNLOAD_GUIDE.txt     (Installation guide)
echo.
echo Next steps:
echo 1. Install a C++ compiler (see DOWNLOAD_GUIDE.txt)
echo 2. Install CMake (if not already installed)
echo 3. Run: test_environment.bat
echo 4. Run: portable_build.bat
echo.
echo This avoids Visual Studio completely! 🚀
echo.
pause
