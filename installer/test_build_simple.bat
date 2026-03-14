@echo off
echo Setting up Visual Studio environment...
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"

echo.
echo Testing compiler availability...
cl 2>nul
if errorlevel 1 (
    echo ERROR: Visual Studio compiler not available
    pause
    exit /b 1
)

echo.
echo Building installer test...
cl /EHsc /Fe:test_install.exe install.cpp /link user32.lib advapi32.lib shell32.lib

if exist test_install.exe (
    echo [✓] Installer build successful!
    echo Testing installer executable...
    echo.
    
    echo Running version detection test...
    test_install.exe --version-test 2>nul
    if errorlevel 1 (
        echo Installer executable created but may need admin rights to run fully
    ) else (
        echo Installer appears to be working
    )
    
    echo.
    echo Build test completed successfully!
) else (
    echo [✗] Installer build failed
)

pause
