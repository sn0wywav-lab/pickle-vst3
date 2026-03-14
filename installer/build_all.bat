@echo off
echo Building PICKLE VST3 Plugin Installer...

REM Create necessary directories
if not exist installer\files mkdir installer\files
if not exist installer\build mkdir installer\build

REM Build the VST3 plugin first
echo.
echo ================================
echo Building VST3 Plugin...
echo ================================
cd installer\build

REM Configure and build plugin
cmake ..\.. -G "Visual Studio 17 2022" -A x64
if errorlevel 1 goto :error

cmake --build . --config Release
if errorlevel 1 goto :error

REM Check if plugin was built
if not exist "Release\PICKLE.vst3" (
    echo ERROR: Plugin build failed - PICKLE.vst3 not found!
    goto :error
)

echo Plugin built successfully!

REM Copy plugin to installer files directory
copy "Release\PICKLE.vst3" "..\files\"
if errorlevel 1 goto :error

cd ..\..

REM Build installer executable
echo.
echo ================================
echo Building Installer...
echo ================================
cl /EHsc /Fe:installer\install.exe installer\install.cpp /link user32.lib advapi32.lib shell32.lib
if errorlevel 1 goto :error

REM Build uninstaller executable
echo.
echo ================================
echo Building Uninstaller...
echo ================================
cl /EHsc /Fe:installer\uninstall.exe installer\uninstall.cpp /link user32.lib advapi32.lib shell32.lib
if errorlevel 1 goto :error

REM Create installation package
echo.
echo ================================
echo Creating Installation Package...
echo ================================
if not exist installer\package mkdir installer\package

copy installer\install.exe installer\package\
copy installer\uninstall.exe installer\package\
copy installer\files\PICKLE.vst3 installer\package\

REM Create README for installer
echo Creating documentation...
echo PICKLE VST3 Plugin Installer > installer\package\README.txt
echo ========================= >> installer\package\README.txt
echo. >> installer\package\README.txt
echo Installation Instructions: >> installer\package\README.txt
echo 1. Right-click install.exe and select "Run as administrator" >> installer\package\README.txt
echo 2. Follow the on-screen prompts >> installer\package\README.txt
echo 3. Restart your DAW to use the plugin >> installer\package\README.txt
echo. >> installer\package\README.txt
echo Uninstallation Instructions: >> installer\package\README.txt
echo 1. Right-click uninstall.exe and select "Run as administrator" >> installer\package\README.txt
echo 2. Confirm the uninstallation >> installer\package\README.txt
echo 3. Restart your DAW >> installer\package\README.txt
echo. >> installer\package\README.txt
echo Version: 1.0.0 >> installer\package\README.txt
echo Plugin: PICKLE VST3 >> installer\package\README.txt

echo.
echo ================================
echo BUILD SUCCESSFUL!
echo ================================
echo.
echo Installer package created in: installer\package\
echo.
echo Contents:
echo - install.exe    : Main installer with version detection
echo - uninstall.exe  : Uninstaller to remove the plugin
echo - PICKLE.vst3    : The plugin file
echo - README.txt     : Installation instructions
echo.
echo To install: Run installer\package\install.exe as administrator
echo.

goto :end

:error
echo.
echo ================================
echo BUILD FAILED!
echo ================================
echo.
echo Please check the error messages above and ensure:
echo 1. Visual Studio 2022 is installed with C++ tools
echo 2. CMake is available in PATH
echo 3. VST3 SDK is properly configured
echo.
pause
exit /b 1

:end
pause
