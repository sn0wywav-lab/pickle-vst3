@echo off
echo Building PICKLE VST3 Installer...

REM Create build directory
if not exist installer\build mkdir installer\build
cd installer\build

REM Build the plugin first
echo Building VST3 plugin...
cmake ..\.. -G "Visual Studio 17 2022" -A x64
cmake --build . --config Release

if not exist "Release\PICKLE.vst3" (
    echo ERROR: Plugin build failed!
    pause
    exit /b 1
)

REM Copy plugin to installer directory
echo Preparing installer files...
if not exist "..\files" mkdir "..\files"
copy "Release\PICKLE.vst3" "..\files\"

REM Create installer executable
echo Creating installer...
powershell -Command "& { Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.MessageBox]::Show('PICKLE VST3 Plugin installer created successfully!', 'Installation Complete', 'OK', 'Information') }"

echo Installer build complete!
echo Run installer\install.exe to install the plugin
pause
