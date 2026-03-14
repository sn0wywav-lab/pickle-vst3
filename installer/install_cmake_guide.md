# Step 2: Install CMake

## 📥 Download CMake

1. **Download Link**: https://cmake.org/download/
2. **Select**: **Windows x64 Installer**
3. **Download** the latest version (3.15+)

## ⚙️ Installation Steps

1. **Run the installer** (cmake-x.x.x-windows-x86_64.msi)
2. **Important Options**:
   - ✅ **Add CMake to the system PATH for all users** (CRITICAL)
   - ✅ **Add CMake to the system PATH for current user** (if above fails)

3. **Complete Installation**

## ✅ Verification

After installation, verify it worked:

### Method 1: Command Prompt
```cmd
cmake --version
```

### Method 2: PowerShell  
```powershell
cmake --version
```

You should see something like:
```
cmake version 3.29.3

CMake suite maintained and supported by Kitware (kitware.com/cmake).
```

## 🔧 If CMake is not in PATH

If `cmake --version` doesn't work:

1. **Find CMake installation**: Usually `C:\Program Files\CMake\bin`
2. **Add to PATH manually**:
   - Press `Win + R`, type `sysdm.cpl`
   - Go to "Advanced" → "Environment Variables"
   - Under "System variables", find "Path"
   - Click "Edit" → "New" → Add `C:\Program Files\CMake\bin`
3. **Restart Command Prompt/PowerShell**

## 📋 Typical Installation Path
- `C:\Program Files\CMake\bin\cmake.exe`

---

**Next Step**: After VS and CMake are installed, run the build script
