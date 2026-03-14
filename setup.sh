#!/bin/bash
echo "Setting up PICKLE VST3 Plugin..."

# Clone VST3 SDK as submodule if not already done
if [ ! -d "vst3sdk" ]; then
    echo "Cloning VST3 SDK..."
    git submodule add https://github.com/steinbergmedia/vst3sdk.git
    git submodule update --init --recursive
fi

# Create build directory
mkdir -p build
cd build

# Generate project files based on platform
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    echo "Generating Xcode project..."
    cmake -GXcode ..
    echo "To build: open build/PICKLE.xcodeproj or run: xcodebuild"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux
    echo "Generating Makefiles..."
    cmake ..
    echo "To build: run: make"
else
    echo "Unknown platform. Please use appropriate CMake generator."
fi

echo "Setup complete!"
echo "Navigate to the build directory and compile the plugin."
