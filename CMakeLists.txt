cmake_minimum_required(VERSION 3.15)
project(PICKLE)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# VST3 SDK paths - assuming it's cloned as a submodule
set(VST3_SDK_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/vst3sdk")
if(NOT EXISTS ${VST3_SDK_ROOT})
    message(FATAL_ERROR "VST3 SDK not found. Please clone as submodule: git submodule add https://github.com/steinbergmedia/vst3sdk.git")
endif()

# Include VST3 SDK
add_subdirectory(${VST3_SDK_ROOT} vst3sdk_build)

# Plugin source files
set(PICKLE_SOURCE_FILES
    source/pickle_processor.cpp
    source/pickle_controller.cpp
    source/pickle_factory.cpp
    source/pickle_entry.cpp
    source/pickle_view.cpp
)

# Create the plugin as a shared library
add_library(pickle MODULE ${PICKLE_SOURCE_FILES})

# Set target properties
set_target_properties(pickle PROPERTIES
    PREFIX ""
    OUTPUT_NAME "PICKLE"
)

# Include directories
target_include_directories(pickle PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/source
    ${VST3_SDK_ROOT}
    ${VST3_SDK_ROOT}/pluginterfaces
    ${VST3_SDK_ROOT}/public.sdk/source
)

# Link VST3 libraries
target_link_libraries(pickle
    PRIVATE
    sdk
    base
    pluginterfaces
)

# Platform-specific settings
if(WIN32)
    target_compile_definitions(pickle PRIVATE
        _CRT_SECURE_NO_WARNINGS
    )
    set_target_properties(pickle PROPERTIES
        SUFFIX ".vst3"
    )
elseif(APPLE)
    set_target_properties(pickle PROPERTIES
        BUNDLE TRUE
        MACOSX_BUNDLE_INFO_PLIST ${CMAKE_CURRENT_SOURCE_DIR}/info.plist
        MACOSX_BUNDLE_BUNDLE_NAME "PICKLE"
        MACOSX_BUNDLE_BUNDLE_VERSION "1.0.0"
        MACOSX_BUNDLE_SHORT_VERSION_STRING "1.0.0"
        MACOSX_BUNDLE_IDENTIFIER "com.pickleaudio.pickle"
    )
endif()

# Install target
install(TARGETS pickle
    LIBRARY DESTINATION "vst3"
    RUNTIME DESTINATION "vst3"
    BUNDLE DESTINATION "vst3"
)
