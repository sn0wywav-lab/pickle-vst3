#include <windows.h>
#include <string>
#include <shlobj.h>
#include <iostream>

#pragma comment(lib, "shell32.lib")
#pragma comment(lib, "advapi32.lib")

const std::string PLUGIN_NAME = "PICKLE";
const std::string PLUGIN_FILENAME = "PICKLE.vst3";
const std::string REGISTRY_KEY = "SOFTWARE\\Pickle Audio\\PICKLE";

// Function to get VST3 plugin directory
std::string getVST3Directory() {
    char path[MAX_PATH];
    ExpandEnvironmentStringsA("%PROGRAMFILES%\\Common Files\\VST3\\", path, MAX_PATH);
    return std::string(path);
}

// Function to remove plugin file
bool removePluginFile() {
    std::string vst3Dir = getVST3Directory();
    std::string pluginPath = vst3Dir + PLUGIN_FILENAME;
    
    if (DeleteFileA(pluginPath.c_str())) {
        return true;
    }
    
    // Try other common locations
    const char* locations[] = {
        "%PROGRAMFILES%\\Common Files\\VST3\\",
        "%PROGRAMFILES(X86)%\\Common Files\\VST3\\",
        "%LOCALAPPDATA%\\Programs\\Common\\VST3\\",
        nullptr
    };
    
    for (int i = 0; locations[i]; i++) {
        char expandedPath[MAX_PATH];
        ExpandEnvironmentStringsA(locations[i], expandedPath, MAX_PATH);
        std::string testPath = std::string(expandedPath) + PLUGIN_FILENAME;
        
        if (DeleteFileA(testPath.c_str())) {
            return true;
        }
    }
    
    return false;
}

// Function to remove registry entries
bool removeRegistryInfo() {
    return RegDeleteTreeA(HKEY_LOCAL_MACHINE, REGISTRY_KEY.c_str()) == ERROR_SUCCESS ||
           GetLastError() == ERROR_FILE_NOT_FOUND;
}

int WINAPI WinMain(HINSTANCE hInstance, HINSTANCE hPrevInstance, LPSTR lpCmdLine, int nCmdShow) {
    // Check for admin privileges
    BOOL isElevated = FALSE;
    HANDLE hToken = nullptr;
    
    if (OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &hToken)) {
        TOKEN_ELEVATION elevation;
        DWORD size = sizeof(TOKEN_ELEVATION);
        if (GetTokenInformation(hToken, TokenElevation, &elevation, sizeof(elevation), &size)) {
            isElevated = elevation.TokenIsElevated;
        }
        CloseHandle(hToken);
    }
    
    if (!isElevated) {
        MessageBoxA(nullptr, "Please run this uninstaller as Administrator.", 
                   "Administrator Required", MB_ICONERROR);
        return 1;
    }
    
    // Confirm uninstallation
    int result = MessageBoxA(nullptr, 
                           ("Are you sure you want to uninstall " + PLUGIN_NAME + "?\n\n" +
                            "This will remove the plugin file and registry entries.").c_str(),
                           "Uninstall " + PLUGIN_NAME, 
                           MB_YESNO | MB_ICONQUESTION);
    
    if (result != IDYES) {
        MessageBoxA(nullptr, "Uninstallation cancelled.", "Cancelled", MB_ICONINFORMATION);
        return 0;
    }
    
    bool fileRemoved = removePluginFile();
    bool registryRemoved = removeRegistryInfo();
    
    if (fileRemoved && registryRemoved) {
        MessageBoxA(nullptr, 
                   (PLUGIN_NAME + " has been successfully uninstalled.\n\n" +
                    "Please restart your DAW to complete the removal.").c_str(),
                   "Uninstallation Complete", MB_ICONINFORMATION);
        return 0;
    } else {
        std::string errorMsg = "Uninstallation encountered issues:\n\n";
        if (!fileRemoved) errorMsg += "- Plugin file could not be removed\n";
        if (!registryRemoved) errorMsg += "- Registry entries could not be removed\n";
        errorMsg += "\nYou may need to manually remove remaining files.";
        
        MessageBoxA(nullptr, errorMsg.c_str(), "Uninstallation Issues", MB_ICONWARNING);
        return 1;
    }
}
