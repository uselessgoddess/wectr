cmake_minimum_required(VERSION 3.14)
project(wectr_host LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(WECTR_WAMR_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../third_party/wasm-micro-runtime"
    CACHE PATH "Path to the WAMR")

if(NOT EXISTS "${WECTR_WAMR_DIR}/build-scripts/runtime_lib.cmake")
    message(FATAL_ERROR
        "WAMR not found at ${WECTR_WAMR_DIR}. Run: git submodule update --init --recursive")
endif()

if(NOT DEFINED WAMR_BUILD_PLATFORM)
    set(WAMR_BUILD_PLATFORM "linux")
endif()
if(NOT DEFINED WAMR_BUILD_TARGET)
    set(WAMR_BUILD_TARGET "X86_64")
endif()
set(WAMR_BUILD_INTERP 1)
set(WAMR_BUILD_FAST_INTERP 1)
set(WAMR_BUILD_AOT 0)
set(WAMR_BUILD_JIT 0)
set(WAMR_BUILD_LIBC_BUILTIN 1)
set(WAMR_BUILD_LIBC_WASI 0)
set(WAMR_BUILD_MULTI_MODULE 0)
set(WAMR_BUILD_SIMD 0)
set(WAMR_BUILD_REF_TYPES 1)
set(WAMR_BUILD_BULK_MEMORY 1)
set(WAMR_DISABLE_HW_BOUND_CHECK 1)

set(WAMR_ROOT_DIR ${WECTR_WAMR_DIR})
include(${WAMR_ROOT_DIR}/build-scripts/runtime_lib.cmake)

add_library(wamr_static STATIC ${WAMR_RUNTIME_LIB_SOURCE})
target_include_directories(wamr_static PUBLIC ${WAMR_ROOT_DIR}/core/iwasm/include)

add_library(wectr STATIC wectr.c)
target_include_directories(wectr PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(wectr PUBLIC wamr_static)
if(UNIX)
    find_package(Threads REQUIRED)
    target_link_libraries(wectr PUBLIC Threads::Threads ${CMAKE_DL_LIBS} m)
endif()

option(WECTR_BUILD_EXAMPLE "Build the example C host" ON)
if(WECTR_BUILD_EXAMPLE)
    add_executable(wectr_host ${CMAKE_CURRENT_SOURCE_DIR}/../examples/host/main.c)
    target_link_libraries(wectr_host PRIVATE wectr)
endif()
