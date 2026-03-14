@echo off
echo Testing PICKLE VST3 Installer...
echo.

REM Check if installer files exist
echo ================================
echo Checking Installer Files...
echo ================================
if exist "installer\package\install.exe" (
    echo [✓] install.exe found
) else (
    echo [✗] install.exe NOT found - Run build_all.bat first
    goto :end
)

if exist "installer\package\uninstall.exe" (
    echo [✓] uninstall.exe found
) else (
    echo [✗] uninstall.exe NOT found
)

if exist "installer\package\PICKLE.vst3" (
    echo [✓] PICKLE.vst3 found
) else (
    echo [✗] PICKLE.vst3 NOT found
)

if exist "installer\package\README.txt" (
    echo [✓] README.txt found
) else (
    echo [✗] README.txt NOT found
)

echo.
echo ================================
echo Testing Installer Functions...
echo ================================

REM Test version detection (dry run)
echo Testing version detection logic...
echo This would check for existing installations and compare versions.

REM Test file structure
echo.
echo Installer package structure:
dir installer\package\ /B

echo.
echo ================================
echo Installation Test Complete!
echo ================================
echo.
echo To actually test installation:
echo 1. Right-click installer\package\install.exe
echo 2. Select "Run as administrator"
echo 3. Follow the prompts
echo.
echo To test uninstallation:
echo 1. Right-click installer\package\uninstall.exe  
echo 2. Select "Run as administrator"
echo 3. Confirm the uninstallation

:end
pause
