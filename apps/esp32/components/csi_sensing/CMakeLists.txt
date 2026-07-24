idf_component_register(
    SRCS "csi_sensing.c"
    INCLUDE_DIRS "include"
)

target_link_libraries(${COMPONENT_LIB} PRIVATE m)
