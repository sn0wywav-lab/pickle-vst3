# 🚀 Visual Studio Installation Guide for PICKLE VST3

## 📋 What You Chose: Option 3 - Visual Studio (Full Development)

This is the **best option** for VST3 development! Here's your streamlined guide:

---

## ⚡ Step 1: Install Visual Studio 2022 Community

### 📥 Download & Install
1. **Download**: https://visualstudio.microsoft.com/downloads/
2. **Click**: "Visual Studio Community 2022" (FREE - no registration needed)
3. **Run**: The downloaded installer (`vs_community.exe`)

### ⚙️ Critical Installation Selection
When the installer opens:
- ✅ **Desktop development with C++** (MOST IMPORTANT)
- ✅ **Windows 10/11 SDK** (auto-selected)
- ✅ **MSVC v143 build tools** (auto-selected)

**Installation Size**: ~10GB
**Time**: 30-60 minutes

---

## ⚡ Step 2: Install CMake

### 📥 Quick Install
1. **Download**: https://cmake.org/download/
2. **Select**: "Windows x64 Installer"
3. **CRITICAL**: ✅ **Check "Add CMake to the system PATH"**
4. **Complete**: Installation

**Size**: ~50MB
**Time**: 5 minutes

---

## ⚡ Step 3: Verify Installation

### 🧪 Quick Test
Open **Command Prompt** or **PowerShell** and run:

```cmd
# Test Visual Studio
cl

# Test CMake
cmake --version
```

Both should show version information (not "command not found").

---

## ⚡ Step 4: Build PICKLE VST3!

### 🎯 Automated Build (Recommended)
```cmd
cd C:\Users\sn0wy\CascadeProjects\windsurf-project-2\pickle_vst3\installer
setup_and_build.bat
```

### 🔧 Manual Build (If automated fails)
```cmd
cd C:\Users\sn0wy\CascadeProjects\windsurf-project-2\pickle_vst3\installer
build_all.bat
```

---

## 🎉 What You'll Get

### After Successful Build:
```
installer\package\
├── install.exe      (Smart installer with version detection)
├── uninstall.exe    (Clean uninstaller)
├── PICKLE.vst3      (Your complete VST3 plugin!)
└── README.txt       (Installation guide)
```

### Plugin Features:
- 🎛️ **Pickle Processor**: OTT-style multiband compression
- 🔥 **Pickle Crunch**: Enhanced low/high distortion
- 💊 **Pickle Bitcrush**: Subtle bit reduction (below Crunch)
- 🎨 **Beautiful UI**: Blurred glass interface
- 🔄 **Smart Updates**: Version detection and management

---

## 🎯 Installation After Build

### Quick Install:
1. **Navigate**: `installer\package\`
2. **Right-click**: `install.exe`
3. **Select**: "Run as administrator"
4. **Follow**: Prompts
5. **Restart**: Your DAW

### The installer will:
- ✅ Detect existing PICKLE installations
- ✅ Handle updates automatically
- ✅ Install to correct VST3 directory
- ✅ Create registry entries

---

## ⏱️ Time Estimates

| Step | Time | Effort |
|------|------|--------|
| VS Installation | 30-60 min | Easy |
| CMake Installation | 5 min | Easy |
| Building Plugin | 5-10 min | Automated |
| Total | 40-75 min | Worth it! |

---

## 🔧 Troubleshooting

### If `cl` command not found:
- Restart Command Prompt after VS installation
- Verify "Desktop development with C++" was selected

### If `cmake` not found:
- Reinstall CMake with "Add to PATH" checked
- Restart Command Prompt

### If build fails:
- Run `check_requirements.bat` for diagnosis
- Ensure VST3 SDK is available (git clone if needed)

---

## 🎯 Why Visual Studio is Best for VST3

### ✅ Advantages:
- **Official Steinberg support** - VST3 SDK designed for VS
- **Best performance** - Optimized compiler
- **Full debugging** - Step through your code
- **Professional tools** - Industry standard
- **Future proof** - Works with all audio development

### 🎵 Perfect for Audio Development:
- VST3 SDK integration
- Windows API support
- DirectShow/ WASAPI access
- Assembly optimization possible

---

## 📞 Quick Help

### Commands to Remember:
```cmd
# Check everything is working
cd installer
check_requirements.bat

# Build the plugin
setup_and_build.bat

# Test the build
test_installer.bat
```

### If Issues:
1. **Restart** Command Prompt after installations
2. **Run as Administrator** for build scripts
3. **Check PATH** includes VS and CMake

---

## 🎉 Ready to Start!

### Your Action Plan:
1. **Install Visual Studio 2022 Community** (30-60 min)
2. **Install CMake** (5 min)
3. **Run**: `installer\setup_and_build.bat`
4. **Install**: `installer\package\install.exe`
5. **Enjoy**: Your PICKLE VST3 plugin! 🥒

---

**🚀 You're on your way to building a professional VST3 plugin!** 🎵✨
