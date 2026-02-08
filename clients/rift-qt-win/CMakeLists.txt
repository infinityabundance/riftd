cmake_minimum_required(VERSION 3.20)
project(rift-qt-win LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 REQUIRED COMPONENTS Core Gui Widgets)

set(RIFT_SDK_INCLUDE_DIR "${CMAKE_CURRENT_LIST_DIR}/../../crates/rift-sdk" CACHE PATH "Path to rift-sdk headers")
set(RIFT_SDK_LIB "" CACHE FILEPATH "Path to rift_sdk.lib")

if(NOT RIFT_SDK_LIB)
    find_library(RIFT_SDK_LIB rift_sdk PATHS "${CMAKE_CURRENT_LIST_DIR}/../../target/release" "${CMAKE_CURRENT_LIST_DIR}/../../target/debug" NO_DEFAULT_PATH)
endif()

if(NOT RIFT_SDK_LIB)
    message(FATAL_ERROR "rift_sdk.lib not found. Set -DRIFT_SDK_LIB=... to the import library.")
endif()

add_executable(rift-qt-win WIN32
    src/main.cpp
    src/MainWindow.h
    src/MainWindow.cpp
    src/RiftBackend.h
    src/RiftBackend.cpp
    src/RiftEventsModel.h
    src/RiftEventsModel.cpp
    src/RiftPeersModel.h
    src/RiftPeersModel.cpp
    resources/rift.rc
)

target_include_directories(rift-qt-win PRIVATE ${RIFT_SDK_INCLUDE_DIR})

target_link_libraries(rift-qt-win PRIVATE
    Qt6::Core
    Qt6::Gui
    Qt6::Widgets
    ${RIFT_SDK_LIB}
)
