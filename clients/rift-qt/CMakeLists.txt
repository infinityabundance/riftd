cmake_minimum_required(VERSION 3.20)
project(rift-qt LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

option(RIFT_QT_KIRIGAMI "Use Kirigami (KF6)" ON)

find_package(Qt6 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2 Widgets)
if(RIFT_QT_KIRIGAMI)
    find_package(KF6Kirigami2 QUIET)
endif()

set(RIFT_SDK_INCLUDE_DIR "${CMAKE_CURRENT_LIST_DIR}/../../crates/rift-sdk" CACHE PATH "Path to rift-sdk headers")
set(RIFT_SDK_LIB "" CACHE FILEPATH "Path to librift_sdk.so")

if(NOT RIFT_SDK_LIB)
    find_library(RIFT_SDK_LIB rift_sdk PATHS "${CMAKE_CURRENT_LIST_DIR}/../../target/release" "${CMAKE_CURRENT_LIST_DIR}/../../target/debug" NO_DEFAULT_PATH)
endif()

if(NOT RIFT_SDK_LIB)
    message(FATAL_ERROR "librift_sdk.so not found. Set -DRIFT_SDK_LIB=... to the built library.")
endif()
get_filename_component(RIFT_SDK_LIB_DIR "${RIFT_SDK_LIB}" DIRECTORY)

qt_add_executable(rift-qt
    src/main.cpp
    src/RiftBackend.h
    src/RiftBackend.cpp
    src/RiftEventsModel.h
    src/RiftEventsModel.cpp
    src/RiftPeersModel.h
    src/RiftPeersModel.cpp
)

qt_add_resources(rift-qt rift-qt-qml
    PREFIX "/"
    FILES
        qml/Main.qml
)

target_include_directories(rift-qt PRIVATE
    ${RIFT_SDK_INCLUDE_DIR}
)

target_link_libraries(rift-qt PRIVATE
    Qt6::Core
    Qt6::Gui
    Qt6::Qml
    Qt6::Quick
    Qt6::QuickControls2
    Qt6::Widgets
    ${RIFT_SDK_LIB}
)

set_target_properties(rift-qt PROPERTIES
    BUILD_RPATH "${RIFT_SDK_LIB_DIR}"
)

if(TARGET KF6::Kirigami2)
    target_compile_definitions(rift-qt PRIVATE RIFT_QT_USE_KIRIGAMI)
    target_link_libraries(rift-qt PRIVATE KF6::Kirigami2)
endif()

install(TARGETS rift-qt RUNTIME DESTINATION bin)
install(FILES resources/org.rift.Rift.desktop DESTINATION share/applications)
install(FILES resources/org.rift.Rift.svg DESTINATION share/icons/hicolor/64x64/apps)
