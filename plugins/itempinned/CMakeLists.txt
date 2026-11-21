set(copyq_plugin_itempinned_SOURCES
    $<TARGET_OBJECTS:copyq-plugin-wrapper>
    ../../src/item/itemsaverwrapper.cpp
    ../../src/common/display.cpp
    $<TARGET_OBJECTS:copyq-plugin-screen>
    )

if (APPLE)
    list(APPEND copyq_plugin_itempinned_SOURCES pinnedlock_mac.mm)
    set(copyq_plugin_itempinned_LIBRARIES
        "-framework Foundation"
        "-framework LocalAuthentication"
        )
endif()

copyq_add_plugin(itempinned)
