#pragma once

// Version information for PICKLE VST3 Plugin
#define PICKLE_VERSION_MAJOR 1
#define PICKLE_VERSION_MINOR 0
#define PICKLE_VERSION_PATCH 0
#define PICKLE_VERSION_STRING "1.0.0"

// Plugin information
#define PICKLE_NAME "PICKLE"
#define PICKLE_FILENAME "PICKLE.vst3"
#define PICKLE_AUTHOR "Pickle Audio"
#define PICKLE_DESCRIPTION "Pickle-themed audio effect plugin"

// Registry information
#define PICKLE_REGISTRY_KEY "SOFTWARE\\Pickle Audio\\PICKLE"
#define PICKLE_REGISTRY_VERSION "Version"
#define PICKLE_REGISTRY_INSTALL_PATH "InstallPath"

// File version structure for Windows
#define PICKLE_FILE_VERSION PICKLE_VERSION_MAJOR, PICKLE_VERSION_MINOR, PICKLE_VERSION_PATCH, 0
