# PICKLE VST3 Plugin

A sleek, pickle-themed audio effect plugin that adds unique character to your audio tracks.

## Also in this repository: Pickle client

[`pickle-client/`](pickle-client/) is a fast native Rust client for Discord with unlocked voice quality (up to 600 kb/s stereo Opus, selectable 8–48 kHz, no audio filters), uncapped camera quality, and support for BetterDiscord/Vencord themes and plugins. See [`pickle-client/README.md`](pickle-client/README.md).

## 🎨 UI Design

The PICKLE plugin features a stunning modern blurred glass interface:
- **Glass morphism design** with backdrop blur effects
- **Pickle-green accent colors** with smooth gradients
- **Intuitive knob layout** with main Processor knob prominently displayed
- **Subtle bitcrush knob** positioned below the Crunch knob
- **Smooth animations** and hover effects
- **Preset buttons** for quick sound selection
- **Responsive design** that scales beautifully

Open `ui_design.html` in your browser to see the visual design mockup.

## Features

### Main Features
- **Pickle Processor**: OTT-style multiband compression that makes tracks sound very forward and fat
- **Pickle Crunch**: Enhanced distortion with subtle low and high frequency focus for added fatness
- **Pickle Bitcrush**: Subtle bitcrush effect positioned below the Crunch knob

### 🎯 Use Cases
- **Drum processing**: Add crunch and fatness with the Processor and Crunch controls
- **Bass enhancement**: Use multiband compression for forwardness and low-end fatness
- **Vocal effects**: Subtle bitcrush and spice for unique textures
- **Synth processing**: Make leads cut through the mix with Processor compression
- **Mastering**: Gentle processing for final polish and forward sound

### Original Features
- **Pickle Brine**: Wet/Dry mix control
- **Pickle Spice**: High-frequency enhancement
- **Pickle Ferment**: Delay and feedback effects
- **Pickle Juice**: Output gain control

## Installation

### 🚀 Easy Installer (Recommended)
1. Run `installer\build_all.bat` to build the complete installer package
2. Navigate to `installer\package\`
3. Right-click `install.exe` and select "Run as administrator"
4. Follow the prompts - the installer will automatically:
   - Detect existing installations
   - Prompt for updates if needed
   - Install to the correct VST3 directory
   - Handle version upgrades and downgrades intelligently

### Manual Installation
If you prefer manual installation:

#### Windows
1. Build the project using Visual Studio 2022
2. Copy the resulting `.vst3` file to your VST3 plugins directory
3. Restart your DAW

#### macOS
1. Build using Xcode
2. Copy the bundle to `/Library/Audio/Plug-Ins/VST3/` or `~/Library/Audio/Plug-Ins/VST3/`
3. Restart your DAW

### 🔄 Smart Installer Features
- **Version Detection**: Automatically finds existing PICKLE installations
- **Update Management**: Prompts when updates are available
- **Downgrade Protection**: Warns before installing older versions
- **Repair Mode**: Reinstall existing installations
- **Clean Uninstall**: Complete removal with uninstaller.exe

## Building

### Prerequisites
- CMake 3.15 or higher
- VST3 SDK (included as git submodule)
- Visual Studio 2022 (Windows) or Xcode (macOS)

### Steps
1. Clone the repository with submodules:
   ```bash
   git clone --recursive https://github.com/yourusername/pickle-vst3.git
   cd pickle-vst3
   ```

2. Build:
   ```bash
   mkdir build
   cd build
   cmake .. -G "Visual Studio 17 2022" -A x64
   cmake --build . --config Release
   ```

## Plugin Parameters

| Parameter | Range | Description |
|-----------|-------|-------------|
| **Pickle Processor** | 0.0 - 1.0 | OTT-style multiband compression for forwardness and fatness |
| **Pickle Crunch** | 0.0 - 1.0 | Enhanced distortion with low/high frequency focus |
| **Pickle Bitcrush** | 0.0 - 1.0 | Subtle bitcrush effect (sub-parameter below crunch) |
| Pickle Brine | 0.0 - 1.0 | Mix between dry and processed signal |
| Pickle Spice | 0.0 - 1.0 | High-frequency enhancement |
| Pickle Ferment | 0.0 - 1.0 | Delay/feedback amount |
| Pickle Juice | 0.0 - 2.0 | Output gain |

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## About

Created with 🥒 by Pickle Audio - bringing the crunch to your music!
