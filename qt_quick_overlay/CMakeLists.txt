cmake_minimum_required(VERSION 3.16)
project(usit-qt-overlay LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_AUTOMOC ON)
find_package(Qt6 REQUIRED COMPONENTS Core Gui Qml Quick Test)
qt_add_executable(usit-qt-overlay main.cpp)
target_link_libraries(usit-qt-overlay PRIVATE Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick Qt6::Test)
set(QML_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../examples/qt_quick_overlay")
qt_add_resources(usit-qt-overlay qml PREFIX "/" BASE "${QML_DIR}" FILES
    "${QML_DIR}/Main.qml" "${QML_DIR}/ActionButton.qml"
    "${QML_DIR}/ControlDrawer.qml" "${QML_DIR}/DrawerSwitch.qml"
    "${QML_DIR}/TranscriptPanel.qml" "${QML_DIR}/SpectrogramViewport.qml"
    "${QML_DIR}/ValueSlider.qml" "${QML_DIR}/ModelSelector.qml")
