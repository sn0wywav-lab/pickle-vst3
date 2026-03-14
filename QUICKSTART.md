# 🥒 PICKLE VST3 Plugin - Quick Start Guide

## Overview
PICKLE is a sleek, pickle-themed audio effect plugin that adds unique character to your audio tracks with five carefully crafted parameters.

## 🚀 Quick Setup

### 1. Clone and Setup
```bash
git clone <repository-url>
cd pickle_vst3
chmod +x setup.sh
./setup.sh
```

### 2. Build on Windows
```bash
# Run the build script
build.bat

# Or manually:
mkdir build && cd build
cmake .. -G "Visual Studio 17 2022" -A x64
cmake --build . --config Release
```

### 3. Build on macOS
```bash
cd build
xcodebuild -project PICKLE.xcodeproj -scheme PICKLE -configuration Release
```

### 4. Build on Linux
```bash
cd build
make
```

## 📦 Installation
- **Windows**: Copy `build/Release/PICKLE.vst3` to `C:\Program Files\Common Files\VST3\`
- **macOS**: Copy `build/Release/PICKLE.vst3` to `~/Library/Audio/Plug-Ins/VST3/`
- **Linux**: Copy `build/PICKLE.so` to `~/.vst3/`

## 🎛️ Plugin Parameters

| Parameter | Range | Function | Pickle Analogy |
|-----------|-------|----------|-----------------|
| **Pickle Brine** | 0.0 - 1.0 | Wet/Dry mix | How much pickle brine to mix with your sound |
| **Pickle Crunch** | 0.0 - 1.0 | Asymmetric distortion | The satisfying crunch of a fresh pickle |
| **Pickle Spice** | 0.0 - 1.0 | High-frequency enhancement | Adding spice and brightness |
| **Pickle Ferment** | 0.0 - 1.0 | Delay/feedback | The aging process that adds character |
| **Pickle Juice** | 0.0 - 2.0 | Output gain | The final juicy output level |

## 🎵 Sound Characteristics
- **Warm distortion** with asymmetric clipping for harmonic richness
- **High-frequency enhancement** that adds clarity without harshness
- **Modulated delay** for subtle movement and depth
- **Smooth parameter transitions** perfect for automation

## 🔧 Technical Details
- **Format**: VST3 (64-bit)
- **Channels**: Stereo in/out
- **Sample Rates**: 44.1kHz - 192kHz
- **Buffer Sizes**: Any size supported by host
- **CPU Usage**: Very light

## 🎯 Use Cases
- **Drum processing**: Add crunch and character to snares and kicks
- **Bass enhancement**: Warm up basslines with pickle brine and crunch
- **Vocal effects**: Subtle spice and fermentation for unique textures
- **Synth processing**: Add depth and movement to synth leads
- **Mastering**: Gentle juice and brine for final polish

## 🎨 UI Design (Future)
The plugin features a sleek, modern interface with:
- Pickle-green color scheme
- Smooth, animated knobs
- Real-time parameter display
- Preset management system

## 🐛 Troubleshooting
- **Plugin not showing**: Check VST3 installation path
- **No sound**: Verify audio routing in your DAW
- **Crashes**: Ensure all dependencies are installed

## 📝 License
MIT License - feel free to modify and distribute!

---

**Made with 🥒 by Pickle Audio - Bringing the crunch to your music!**
