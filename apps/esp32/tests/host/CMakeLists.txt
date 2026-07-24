cmake_minimum_required(VERSION 3.16)
project(csi_sensing_host_tests C)

enable_testing()

add_executable(csi_sensing_test
    test_csi_sensing.c
    ../../components/csi_sensing/csi_sensing.c
)
target_include_directories(csi_sensing_test PRIVATE
    ../../components/csi_sensing/include
)
target_compile_features(csi_sensing_test PRIVATE c_std_17)
if(MSVC)
    target_compile_options(csi_sensing_test PRIVATE /W4)
else()
    target_compile_options(csi_sensing_test PRIVATE -Wall -Wextra -Werror)
    target_link_libraries(csi_sensing_test PRIVATE m)
endif()

add_test(NAME csi_sensing_test COMMAND csi_sensing_test)
