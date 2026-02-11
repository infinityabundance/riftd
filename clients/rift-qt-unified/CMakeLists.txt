cmake_minimum_required(VERSION 3.21)
project(RiftQt VERSION 0.1.4 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

# Find Qt6
find_package(Qt6 6.5 REQUIRED COMPONENTS Core Gui Widgets Quick QuickControls2 Multimedia)

# Platform-specific settings
if(APPLE)
    set(MACOSX_BUNDLE_GUI_IDENTIFIER "com.rift.desktop")
    set(MACOSX_BUNDLE_BUNDLE_VERSION ${PROJECT_VERSION})
    set(MACOSX_BUNDLE_SHORT_VERSION_STRING ${PROJECT_VERSION_MAJOR}.${PROJECT_VERSION_MINOR})
    set(MACOSX_BUNDLE_BUNDLE_NAME "Rift")
    set(CMAKE_OSX_DEPLOYMENT_TARGET "11.0")
endif()

if(WIN32)
    set(APP_ICON_RESOURCE_WINDOWS "${CMAKE_CURRENT_SOURCE_DIR}/resources/rift.rc")
endif()

# Rift SDK library
set(RIFT_SDK_LIB "" CACHE FILEPATH "Path to librift_sdk.a or rift_sdk.lib")
if(NOT RIFT_SDK_LIB)
    if(APPLE)
        set(RIFT_SDK_LIB "${CMAKE_SOURCE_DIR}/../../target/release/librift_sdk.a")
    elseif(WIN32)
        set(RIFT_SDK_LIB "${CMAKE_SOURCE_DIR}/../../target/release/rift_sdk.lib")
    else()
        set(RIFT_SDK_LIB "${CMAKE_SOURCE_DIR}/../../target/release/librift_sdk.a")
    endif()
endif()

# Sources
set(PROJECT_SOURCES
    src/main.cpp
    src/RiftBackend.cpp
    src/RiftBackend.h
    src/CallManager.cpp
    src/CallManager.h
    src/NotificationManager.cpp
    src/NotificationManager.h
)

qt_add_executable(RiftQt
    MACOSX_BUNDLE
    WIN32
    ${PROJECT_SOURCES}
    ${APP_ICON_RESOURCE_WINDOWS}
)

qt_add_qml_module(RiftQt
    URI Rift
    VERSION 1.0
    QML_FILES
        qml/Main.qml
        qml/ChannelView.qml
        qml/ChatView.qml
        qml/CallView.qml
        qml/PeerList.qml
        qml/SettingsView.qml
)

# Link libraries
target_link_libraries(RiftQt PRIVATE
    Qt6::Core
    Qt6::Gui
    Qt6::Widgets
    Qt6::Quick
    Qt6::QuickControls2
    Qt6::Multimedia
    ${RIFT_SDK_LIB}
)

# Platform-specific link libraries
if(WIN32)
    target_link_libraries(RiftQt PRIVATE
        ws2_32
        userenv
        bcrypt
        ntdll
    )
elseif(APPLE)
    find_library(SECURITY_FRAMEWORK Security)
    find_library(COREFOUNDATION_FRAMEWORK CoreFoundation)
    find_library(COREAUDIO_FRAMEWORK CoreAudio)
    find_library(AUDIOTOOLBOX_FRAMEWORK AudioToolbox)
    target_link_libraries(RiftQt PRIVATE
        ${SECURITY_FRAMEWORK}
        ${COREFOUNDATION_FRAMEWORK}
        ${COREAUDIO_FRAMEWORK}
        ${AUDIOTOOLBOX_FRAMEWORK}
        "-framework SystemConfiguration"
        "-framework UserNotifications"
    )
else()
    # Linux dependencies for Rust SDK
    find_package(PkgConfig REQUIRED)
    pkg_check_modules(OPUS REQUIRED opus)
    pkg_check_modules(ALSA REQUIRED alsa)

    target_link_libraries(RiftQt PRIVATE
        pthread
        dl
        m
        ${OPUS_LIBRARIES}
        ${ALSA_LIBRARIES}
    )
    target_include_directories(RiftQt PRIVATE
        ${OPUS_INCLUDE_DIRS}
        ${ALSA_INCLUDE_DIRS}
    )
endif()

# Install
install(TARGETS RiftQt
    BUNDLE DESTINATION .
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
)

if(APPLE)
    set_target_properties(RiftQt PROPERTIES
        MACOSX_BUNDLE TRUE
        MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/resources/Info.plist"
    )
endif()
