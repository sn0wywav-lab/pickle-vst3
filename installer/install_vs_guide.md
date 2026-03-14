# Step 1: Install Visual Studio 2022 Community

## 📥 Download Visual Studio 2022

1. **Download Link**: https://visualstudio.microsoft.com/downloads/
2. **Select**: "Visual Studio Community 2022" (FREE)
3. **Click**: "Free download"

## ⚙️ Installation Steps

1. **Run the installer** (vs_community.exe)
2. **Select Workloads** (IMPORTANT):
   - ✅ **Desktop development with C++** (REQUIRED)
   - ✅ **Windows 10/11 SDK** (automatically included)
   - ✅ **MSVC v143 - VS 2022 C++ x64/x86 build tools** (required)

3. **Installation Details** (under "Desktop development with C++"):
   - ✅ MSVC v143 - VS 2022 C++ x64/x86 build tools
   - ✅ Windows 10/11 SDK (latest version)
   - ✅ CMake tools (optional but helpful)

4. **Click Install** - This will take 30-60 minutes

## ✅ Verification

After installation, verify it worked:
1. Open **Command Prompt** or **PowerShell**
2. Type: `cl` and press Enter
3. You should see Microsoft C++ compiler information

## 📍 Typical Installation Path
- `C:\Program Files\Microsoft Visual Studio\2022\Community\`

## 🔧 If Installation Fails
- Make sure you have enough disk space (10GB+)
- Run installer as Administrator
- Temporarily disable antivirus

---

**Next Step**: After VS is installed, proceed to install CMake
