@echo off
title CHECK YOUR COMPILER VERSION
color 0B

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║           🥒 PICKLE VST3 - Compiler Version Check           ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.

echo Checking your actual compiler and tools...
echo.

REM Try to find and run Visual Studio environment
set "VS_FOUND=0"
set "VS_PATH="

REM Check for VS 2026
if exist "C:\Program Files\Microsoft Visual Studio\2026\Community\VC\Auxiliary\Build\vcvars64.bat" (
    echo [✓] Visual Studio 2026 Community found
    set "VS_PATH=C:\Program Files\Microsoft Visual Studio\2026\Community\VC\Auxiliary\Build\vcvars64.bat"
    set "VS_FOUND=1"
    set "VS_VERSION=2026"
    goto :test_compiler
)

REM Check for VS 2022
if exist "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" (
    echo [✓] Visual Studio 2022 Community found
    set "VS_PATH=C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
    set "VS_FOUND=1"
    set "VS_VERSION=2022"
    goto :test_compiler
)

REM Check for other VS installations
for %%p in ("Community" "Professional" "Enterprise") do (
    for %%y in ("2026" "2022" "2019") do (
        if exist "C:\Program Files\Microsoft Visual Studio\%%y\%%~p\VC\Auxiliary\Build\vcvars64.bat" (
            echo [✓] Visual Studio %%y %%~p found
            set "VS_PATH=C:\Program Files\Microsoft Visual Studio\%%y\%%~p\VC\Auxiliary\Build\vcvars64.bat"
            set "VS_FOUND=1"
            set "VS_VERSION=%%y"
            goto :test_compiler
        )
    )
)

if "%VS_FOUND%"=="0" (
    echo [✗] No Visual Studio found
    echo.
    echo Please install Visual Studio 2022 or 2026
    pause
    exit /b 1
)

:test_compiler
echo.
echo Setting up Visual Studio %VS_VERSION% environment...
call "%VS_PATH%" >nul 2>&1

echo.
echo [1/4] Testing C++ compiler...
cl 2>nul
if errorlevel 1 (
    echo [✗] C++ compiler not working
) else (
    echo [✓] C++ compiler found
    echo Compiler version:
    cl 2>&1 | findstr "Version"
)

echo.
echo [2/4] Testing CMake...
cmake --version 2>nul
if errorlevel 1 (
    echo [✗] CMake not found
) else (
    echo [✓] CMake found:
    cmake --version | findstr "cmake version"
)

echo.
echo [3/4] Checking compiler capabilities...
echo Testing C++17 support...
echo #include ^<memory^> > test_cpp17.cpp
echo int main() { return 0; } >> test_cpp17.cpp
cl /std:c++17 /EHsc test_cpp17.cpp >nul 2>&1
if exist test_cpp17.exe (
    echo [✓] C++17 supported
    del test_cpp17.* >nul 2>&1
) else (
    echo [✗] C++17 not supported
    del test_cpp17.* >nul 2>&1
)

echo.
echo [4/4] VST3 Development Readiness...
if exist "..\vst3sdk" (
    echo [✓] VST3 SDK found
) else (
    echo [!] VST3 SDK not found as submodule
    echo This is needed for VST3 development
)

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║                    🎯 BUILD READINESS SUMMARY                  ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.

if "%VS_FOUND%"=="1" (
    echo ✅ Visual Studio %VS_VERSION%: Ready
) else (
    echo ❌ Visual Studio: Not found
)

cmake --version >nul 2>&1
if errorlevel 1 (
    echo ❌ CMake: Not found
) else (
    echo ✅ CMake: Ready
)

echo.
echo What this means for PICKLE VST3:
echo.
if "%VS_VERSION%"=="2022" (
    echo ✅ VS 2022 is PERFECT for VST3 development
    echo ✅ Proven stability and reliability
    echo ✅ Excellent VST3 SDK support
    echo ✅ Industry standard for audio plugins
) else if "%VS_VERSION%"=="2026" (
    echo ✅ VS 2026 gives you latest features
    echo ✅ Better performance optimizations
    echo ✅ Modern C++ features
    echo ✅ AI assistance with Copilot
)

echo.
echo Ready to build PICKLE VST3!
echo.
echo Next step: Run build_all.bat or setup_and_build.bat
echo.
pause
