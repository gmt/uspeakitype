set(USITBRIDGE_SOURCES
    usitbridge.cpp
)

add_library(usitbridge MODULE ${USITBRIDGE_SOURCES})
target_link_libraries(usitbridge
    Fcitx5::Core
    Fcitx5::Utils
    Fcitx5::Module::DBus
)

target_include_directories(usitbridge PRIVATE
    ${FCITX5_MODULE_INCLUDE_DIRS}
    /usr/include/Fcitx5/Module
)

set_target_properties(usitbridge PROPERTIES PREFIX "lib")

install(TARGETS usitbridge DESTINATION "${FCITX_INSTALL_ADDONDIR}")

configure_file(usitbridge.conf.in "${CMAKE_CURRENT_BINARY_DIR}/usitbridge.conf" COPYONLY)
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/usitbridge.conf"
    DESTINATION "${FCITX_INSTALL_PKGDATADIR}/addon"
    COMPONENT config)
