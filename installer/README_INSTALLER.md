# 🥒 PICKLE VST3 Plugin Installer

A professional installer system for the PICKLE VST3 audio plugin with intelligent version detection and update capabilities.

## 🚀 Features

### Smart Installation System
- **Version Detection**: Automatically detects existing installations
- **Update Management**: Prompts for updates when newer versions are available
- **Downgrade Protection**: Warns before downgrading to older versions
- **Repair Mode**: Reinstalls/repairs existing installations
- **Registry Tracking**: Maintains installation information in Windows Registry

### User-Friendly Interface
- **Admin Privilege Check**: Ensures proper permissions for installation
- **Clear Messaging**: Informative dialogs for all scenarios
- **Confirmation Prompts**: Asks before replacing existing versions
- **Success Notifications**: Clear confirmation of successful installation

### Uninstallation Support
- **Complete Removal**: Removes plugin files and registry entries
- **Clean Uninstall**: Ensures no leftover files or registry entries
- **Admin Required**: Secure uninstallation process

## 📦 Installation Package Contents

```
installer/package/
├── install.exe          # Main installer with smart detection
├── uninstall.exe        # Clean uninstaller
├── PICKLE.vst3          # The VST3 plugin file
└── README.txt           # Quick installation guide
```

## 🛠️ Building the Installer

### Prerequisites
- Visual Studio 2022 with C++ tools
- CMake 3.15+
- Windows SDK
- Administrator privileges

### Build Process
1. **Run the build script**:
   ```cmd
   cd pickle_vst3
   installer\build_all.bat
   ```

2. **The build script will**:
   - Build the VST3 plugin
   - Compile the installer executable
   - Compile the uninstaller executable
   - Package everything into `installer/package/`

### Manual Build Steps
If you prefer manual building:

1. **Build the plugin**:
   ```cmd
   mkdir build && cd build
   cmake .. -G "Visual Studio 17 2022" -A x64
   cmake --build . --config Release
   ```

2. **Build installer**:
   ```cmd
   cl /EHsc /Fe:install.exe install.cpp /link user32.lib advapi32.lib shell32.lib
   ```

3. **Build uninstaller**:
   ```cmd
   cl /EHsc /Fe:uninstall.exe uninstall.cpp /link user32.lib advapi32.lib shell32.lib
   ```

## 🎯 Installation Scenarios

### Fresh Installation
- Detects no existing installation
- Installs to standard VST3 directory
- Creates registry entries for version tracking

### Update Installation
- Detects existing version
- Compares versions intelligently
- Prompts for update confirmation
- Replaces old plugin file

### Version Downgrade
- Detects newer version already installed
- Warns user about downgrade
- Requires explicit confirmation

### Repair/Reinstall
- Detects same version already installed
- Offers repair/reinstall option
- Replaces files if confirmed

## 📍 Installation Locations

The installer searches for VST3 directories in this order:
1. `%PROGRAMFILES%\Common Files\VST3\`
2. `%PROGRAMFILES(X86)%\Common Files\VST3\`
3. `%LOCALAPPDATA%\Programs\Common\VST3\`

## 🔐 Security Features

- **Administrator Required**: Ensures proper system-level installation
- **Registry Validation**: Verifies installation integrity
- **File Verification**: Checks plugin file existence
- **Safe Replacement**: Removes old versions before installing new ones

## 🐛 Troubleshooting

### Common Issues

**"Please run this installer as Administrator"**
- Right-click `install.exe` and select "Run as administrator"

**"Failed to create VST3 directory"**
- Check disk space and permissions
- Ensure VST3 directory path is accessible

**"Plugin file not found"**
- Verify `PICKLE.vst3` exists in `files/` directory
- Rebuild the plugin if necessary

**"Failed to remove old version"**
- Close any running DAW applications
- Check if plugin is currently in use

### Manual Cleanup
If uninstallation fails, manually remove:
- Plugin file from VST3 directory
- Registry key: `HKEY_LOCAL_MACHINE\SOFTWARE\Pickle Audio\PICKLE`

## 📋 System Requirements

- **Operating System**: Windows 10/11 (64-bit)
- **Privileges**: Administrator access required
- **DAW**: Any VST3-compatible digital audio workstation
- **Space**: ~50MB for plugin and installer

## 🔄 Version History

### v1.0.0
- Initial release
- Smart version detection
- Update management system
- Complete uninstallation support
- Professional UI dialogs

## 📞 Support

For installation issues:
1. Check the troubleshooting section above
2. Ensure all prerequisites are met
3. Run installer as administrator
4. Contact support if issues persist

---

**Made with 🥒 by Pickle Audio - Professional Plugin Installation**
