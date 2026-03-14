@echo off
echo Building PICKLE VST3 Plugin...

REM Create build directory
if not exist build mkdir build
cd build

REM Generate Visual Studio solution
echo Generating project files...
cmake .. -G "Visual Studio 17 2022" -A x64

REM Build the plugin
echo Building plugin...
cmake --build . --config Release

echo Build complete!
echo Plugin will be in build/Release/PICKLE.vst3
pause
