@echo off
title PUSH PICKLE TO GITHUB
color 0A

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║        🥒 PUSH PICKLE VST3 TO GITHUB - CLOUD BUILD!       ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.

echo This script will help you push your PICKLE plugin to GitHub
echo for automatic cloud building - no local tools needed!
echo.

REM Check if git is available
git --version >nul 2>&1
if errorlevel 1 (
    echo [✗] Git not found
    echo.
    echo Please install Git first:
    echo 1. Download: https://git-scm.com/download/win
    echo 2. Run installer with default settings
    echo 3. Restart Command Prompt and run this script again
    echo.
    pause
    exit /b 1
) else (
    for /f "tokens=3" %%a in ('git --version') do echo [✓] Git %%a found
)

echo.
echo [1/4] Getting your GitHub information...
set /p GITHUB_USERNAME="Enter your GitHub username: "
if "%GITHUB_USERNAME%"=="" (
    echo Username cannot be empty!
    pause
    exit /b 1
)

set /p GITHUB_EMAIL="Enter your GitHub email: "
if "%GITHUB_EMAIL%"=="" (
    echo Email cannot be empty!
    pause
    exit /b 1
)

echo.
echo [2/4] Configuring Git...
git config --global user.name "%GITHUB_USERNAME%"
git config --global user.email "%GITHUB_EMAIL%"
echo [✓] Git configured for %GITHUB_USERNAME%

echo.
echo [3/4] Initializing repository...
if exist .git (
    echo [✓] Repository already exists
) else (
    echo Initializing new repository...
    git init
    echo [✓] Repository initialized
)

echo.
echo [4/4] Adding and committing files...
echo Adding all PICKLE files...
git add .
echo.
echo Creating initial commit...
git commit -m "🥒 Add PICKLE VST3 plugin with all features

- Multiband compression (Pickle Processor)
- Enhanced crunch distortion with bitcrush
- Beautiful blurred glass UI
- Smart installer system
- GitHub Actions for cloud building

Features:
- OTT-style multiband compression for forward/fat sound
- Low/high frequency focused distortion
- Subtle bitcrush effect (below Crunch knob)
- Professional VST3 architecture
- Modern C++ implementation"
if errorlevel 1 (
    echo [!] Nothing to commit - files may already be committed
) else (
    echo [✓] Files committed successfully
)

echo.
echo  ╔══════════════════════════════════════════════════════════════╗
echo  ║                    🎯 READY FOR GITHUB!                        ║
echo  ╚══════════════════════════════════════════════════════════════╝
echo.
echo Your PICKLE plugin is ready to push to GitHub!
echo.
echo NEXT STEPS:
echo.
echo 1. Create a new repository on GitHub:
echo    - Go to: https://github.com/new
echo    - Name: pickle-vst3
echo    - Description: PICKLE VST3 Plugin - Audio effects
echo    - Click: "Create repository"
echo.
echo 2. Push your code:
echo    - Copy the commands below
echo    - Replace YOUR_USERNAME with %GITHUB_USERNAME%
echo    - Run in Command Prompt
echo.
echo    git remote add origin https://github.com/YOUR_USERNAME/pickle-vst3.git
echo    git branch -M main
echo    git push -u origin main
echo.
echo 3. Automatic build will start!
echo    - GitHub Actions will build your plugin
echo    - Check the "Actions" tab on GitHub
echo    - Download the built plugin when complete
echo.
echo Repository URL for your reference:
echo https://github.com/%GITHUB_USERNAME%/pickle-vst3
echo.
pause
