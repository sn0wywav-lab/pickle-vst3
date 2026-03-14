#include <windows.h>
#include <string>
#include <vector>
#include <shlobj.h>
#include <fstream>
#include <iostream>
#include <algorithm>

#pragma comment(lib, "shell32.lib")
#pragma comment(lib, "advapi32.lib")

// Plugin information
const std::string PLUGIN_NAME = "PICKLE";
const std::string PLUGIN_VERSION = "1.0.0";
const std::string PLUGIN_FILENAME = "PICKLE.vst3";

// Registry keys for version tracking
const std::string REGISTRY_KEY = "SOFTWARE\\Pickle Audio\\PICKLE";
const std::string VERSION_VALUE = "Version";
const std::string INSTALL_PATH_VALUE = "InstallPath";

struct InstallInfo {
    std::string version;
    std::string installPath;
    bool isInstalled;
};

// Function to get VST3 plugin directory
std::string getVST3Directory() {
    char path[MAX_PATH];
    
    // Try common VST3 locations in order of preference
    const char* vst3Paths[] = {
        "%PROGRAMFILES%\\Common Files\\VST3\\",
        "%PROGRAMFILES(X86)%\\Common Files\\VST3\\",
        "%LOCALAPPDATA%\\Programs\\Common\\VST3\\",
        nullptr
    };
    
    for (int i = 0; vst3Paths[i]; i++) {
        char expandedPath[MAX_PATH];
        ExpandEnvironmentStringsA(vst3Paths[i], expandedPath, MAX_PATH);
        
        if (GetFileAttributesA(expandedPath) != INVALID_FILE_ATTRIBUTES) {
            return std::string(expandedPath);
        }
    }
    
    // Fallback to Program Files
    ExpandEnvironmentStringsA("%PROGRAMFILES%\\Common Files\\VST3\\", path, MAX_PATH);
    return std::string(path);
}

// Function to check if plugin is already installed
InstallInfo checkExistingInstallation() {
    InstallInfo info = {"", "", false};
    
    HKEY hKey;
    if (RegOpenKeyExA(HKEY_LOCAL_MACHINE, REGISTRY_KEY.c_str(), 0, KEY_READ, &hKey) == ERROR_SUCCESS) {
        char version[256] = {0};
        char path[MAX_PATH] = {0};
        DWORD versionSize = sizeof(version);
        DWORD pathSize = sizeof(path);
        
        if (RegQueryValueExA(hKey, VERSION_VALUE.c_str(), nullptr, nullptr, (LPBYTE)version, &versionSize) == ERROR_SUCCESS) {
            info.version = version;
        }
        
        if (RegQueryValueExA(hKey, INSTALL_PATH_VALUE.c_str(), nullptr, nullptr, (LPBYTE)path, &pathSize) == ERROR_SUCCESS) {
            info.installPath = path;
        }
        
        RegCloseKey(hKey);
        
        // Verify the plugin file actually exists
        if (!info.installPath.empty()) {
            std::string pluginPath = info.installPath + "\\" + PLUGIN_FILENAME;
            if (GetFileAttributesA(pluginPath.c_str()) != INVALID_FILE_ATTRIBUTES) {
                info.isInstalled = true;
            }
        }
    }
    
    // Also check common VST3 directories for the plugin
    if (!info.isInstalled) {
        std::string vst3Dir = getVST3Directory();
        std::string pluginPath = vst3Dir + PLUGIN_FILENAME;
        
        if (GetFileAttributesA(pluginPath.c_str()) != INVALID_FILE_ATTRIBUTES) {
            info.isInstalled = true;
            info.installPath = vst3Dir;
            info.version = "Unknown"; // Found but not in registry
        }
    }
    
    return info;
}

// Function to compare versions (simple numeric comparison)
bool isNewerVersion(const std::string& current, const std::string& installed) {
    if (installed == "Unknown") return true;
    
    // Simple version comparison for x.x.x format
    auto splitVersion = [](const std::string& version) {
        std::vector<int> parts;
        std::string current;
        for (char c : version) {
            if (c == '.') {
                if (!current.empty()) {
                    parts.push_back(std::stoi(current));
                    current.clear();
                }
            } else {
                current += c;
            }
        }
        if (!current.empty()) {
            parts.push_back(std::stoi(current));
        }
        return parts;
    };
    
    auto currentParts = splitVersion(current);
    auto installedParts = splitVersion(installed);
    
    for (size_t i = 0; i < std::max(currentParts.size(), installedParts.size()); i++) {
        int currentVal = i < currentParts.size() ? currentParts[i] : 0;
        int installedVal = i < installedParts.size() ? installedParts[i] : 0;
        
        if (currentVal > installedVal) return true;
        if (currentVal < installedVal) return false;
    }
    
    return false; // Same version
}

// Function to create directory if it doesn't exist
bool createDirectory(const std::string& path) {
    return CreateDirectoryA(path.c_str(), nullptr) || GetLastError() == ERROR_ALREADY_EXISTS;
}

// Function to copy file with progress
bool copyFile(const std::string& source, const std::string& destination) {
    std::ifstream src(source, std::ios::binary);
    std::ofstream dst(destination, std::ios::binary);
    
    if (!src.is_open() || !dst.is_open()) {
        return false;
    }
    
    dst << src.rdbuf();
    return true;
}

// Function to write to registry
bool writeRegistryInfo(const std::string& installPath) {
    HKEY hKey;
    if (RegCreateKeyExA(HKEY_LOCAL_MACHINE, REGISTRY_KEY.c_str(), 0, nullptr, 
                       REG_OPTION_NON_VOLATILE, KEY_WRITE, nullptr, &hKey, nullptr) == ERROR_SUCCESS) {
        
        RegSetValueExA(hKey, VERSION_VALUE.c_str(), 0, REG_SZ, 
                       (LPBYTE)PLUGIN_VERSION.c_str(), PLUGIN_VERSION.length() + 1);
        RegSetValueExA(hKey, INSTALL_PATH_VALUE.c_str(), 0, REG_SZ, 
                       (LPBYTE)installPath.c_str(), installPath.length() + 1);
        
        RegCloseKey(hKey);
        return true;
    }
    
    return false;
}

// Function to show message box
int showMessage(const std::string& message, const std::string& title, UINT type) {
    return MessageBoxA(nullptr, message.c_str(), title.c_str(), type);
}

// Main installation function
bool installPlugin(bool forceUpdate = false) {
    std::string vst3Dir = getVST3Directory();
    
    // Create VST3 directory if it doesn't exist
    if (!createDirectory(vst3Dir)) {
        showMessage("Failed to create VST3 directory: " + vst3Dir, "Installation Error", MB_ICONERROR);
        return false;
    }
    
    // Get current directory (where installer is running)
    char currentDir[MAX_PATH];
    GetCurrentDirectoryA(MAX_PATH, currentDir);
    std::string sourceFile = std::string(currentDir) + "\\files\\" + PLUGIN_FILENAME;
    
    // Check if source file exists
    if (GetFileAttributesA(sourceFile.c_str()) == INVALID_FILE_ATTRIBUTES) {
        showMessage("Plugin file not found: " + sourceFile, "Installation Error", MB_ICONERROR);
        return false;
    }
    
    std::string destinationFile = vst3Dir + PLUGIN_FILENAME;
    
    // If updating, remove old version first
    if (forceUpdate && GetFileAttributesA(destinationFile.c_str()) != INVALID_FILE_ATTRIBUTES) {
        if (!DeleteFileA(destinationFile.c_str())) {
            showMessage("Failed to remove old version. Please close any DAWs and try again.", "Update Error", MB_ICONERROR);
            return false;
        }
    }
    
    // Copy plugin file
    if (!copyFile(sourceFile, destinationFile)) {
        showMessage("Failed to copy plugin file to: " + destinationFile, "Installation Error", MB_ICONERROR);
        return false;
    }
    
    // Write installation info to registry
    if (!writeRegistryInfo(vst3Dir)) {
        showMessage("Plugin installed successfully but failed to write registry information.", "Warning", MB_ICONWARNING);
    }
    
    return true;
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
        showMessage("Please run this installer as Administrator.", "Administrator Required", MB_ICONERROR);
        return 1;
    }
    
    // Check existing installation
    InstallInfo existing = checkExistingInstallation();
    
    int result = IDYES;
    bool forceUpdate = false;
    
    if (existing.isInstalled) {
        if (existing.version == "Unknown") {
            // Found plugin but no registry info
            std::string message = "An existing version of " + PLUGIN_NAME + " was found at:\n" + 
                                 existing.installPath + "\n\nDo you want to replace it with version " + 
                                 PLUGIN_VERSION + "?";
            result = showMessage(message, "Plugin Found", MB_YESNO | MB_ICONQUESTION);
            forceUpdate = (result == IDYES);
        } else if (isNewerVersion(PLUGIN_VERSION, existing.version)) {
            // Newer version available
            std::string message = "A newer version of " + PLUGIN_NAME + " is available!\n\n" +
                                 "Current version: " + existing.version + "\n" +
                                 "New version: " + PLUGIN_VERSION + "\n\n" +
                                 "Do you want to update?";
            result = showMessage(message, "Update Available", MB_YESNO | MB_ICONINFORMATION);
            forceUpdate = (result == IDYES);
        } else if (existing.version == PLUGIN_VERSION) {
            // Same version already installed
            std::string message = PLUGIN_NAME + " version " + PLUGIN_VERSION + " is already installed.\n\n" +
                                 "Do you want to reinstall/repair the installation?";
            result = showMessage(message, "Already Installed", MB_YESNO | MB_ICONQUESTION);
            forceUpdate = (result == IDYES);
        } else {
            // Installed version is newer
            std::string message = "A newer version of " + PLUGIN_NAME + " is already installed!\n\n" +
                                 "Installed version: " + existing.version + "\n" +
                                 "This version: " + PLUGIN_VERSION + "\n\n" +
                                 "Do you want to downgrade to this version?";
            result = showMessage(message, "Newer Version Found", MB_YESNO | MB_ICONWARNING);
            forceUpdate = (result == IDYES);
        }
        
        if (result != IDYES) {
            showMessage("Installation cancelled.", "Cancelled", MB_ICONINFORMATION);
            return 0;
        }
    } else {
        // Fresh installation
        std::string message = "Welcome to the " + PLUGIN_NAME + " VST3 Plugin Installer!\n\n" +
                             "This will install " + PLUGIN_NAME + " version " + PLUGIN_VERSION + 
                             " to your VST3 plugins directory.\n\nDo you want to continue?";
        result = showMessage(message, "Install " + PLUGIN_NAME, MB_YESNO | MB_ICONINFORMATION);
        
        if (result != IDYES) {
            showMessage("Installation cancelled.", "Cancelled", MB_ICONINFORMATION);
            return 0;
        }
    }
    
    // Perform installation
    if (installPlugin(forceUpdate)) {
        std::string successMsg = PLUGIN_NAME + " version " + PLUGIN_VERSION + " has been successfully installed!\n\n";
        
        if (forceUpdate) {
            successMsg += "The previous version has been replaced.\n\n";
        }
        
        successMsg += "Installation location:\n" + getVST3Directory() + "\n\n" +
                     "Please restart your DAW to use the plugin.";
        
        showMessage(successMsg, "Installation Complete", MB_ICONINFORMATION);
        return 0;
    } else {
        showMessage("Installation failed. Please check permissions and try again.", "Installation Failed", MB_ICONERROR);
        return 1;
    }
}
