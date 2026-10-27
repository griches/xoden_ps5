# Explicit opt-in until the native engine has console acceptance. The legacy
# worker remains available in ordinary builds; runtime-kit pins are unchanged.
set(PLAYDEN_XBOX_NATIVE_SOURCE "" CACHE PATH "xodus-ps5-native crate source directory")
set(PLAYDEN_XBOX_NATIVE_LIBRARY "" CACHE FILEPATH "Adapted PS5 libxodus_ps5_native.a built with job-engine")
if(PLAYDEN_XBOX_NATIVE_SOURCE OR PLAYDEN_XBOX_NATIVE_LIBRARY)
    if(NOT PLAYDEN_PLATFORM_PS5 OR NOT EXISTS "${PLAYDEN_XBOX_NATIVE_LIBRARY}" OR
       NOT EXISTS "${PLAYDEN_XBOX_NATIVE_SOURCE}/include/xodus_ps5_native.h" OR
       NOT EXISTS "${PLAYDEN_XBOX_NATIVE_SOURCE}/tools/ps5_entropy.c")
        message(FATAL_ERROR "Native Xbox requires a PS5 job-engine archive and matching crate source")
    endif()
    # The native packager treats unresolved weak hooks as imports. Explicit
    # null definitions preserve AWS-LC's normal optional-hook semantics and its
    # own allocation headers/zeroization; no custom crypto allocator is installed.
    set(PLAYDEN_XBOX_NATIVE_LINK_OPTIONS
        --wrap=arc4random_buf
        --defsym=OPENSSL_memory_alloc=0 --defsym=OPENSSL_memory_free=0
        --defsym=OPENSSL_memory_get_size=0 --defsym=OPENSSL_memory_realloc=0
        --defsym=sdallocx=0)
    target_sources(playden_library_service PRIVATE
        "${CMAKE_SOURCE_DIR}/src/stores/xbox/native_engine.cpp"
        "${CMAKE_SOURCE_DIR}/src/stores/xbox/native_compat.c"
        "${PLAYDEN_XBOX_NATIVE_SOURCE}/tools/probe_compat.c"
        "${PLAYDEN_XBOX_NATIVE_SOURCE}/tools/ps5_entropy.c")
    target_include_directories(playden_library_service PRIVATE "${PLAYDEN_XBOX_NATIVE_SOURCE}/include")
    target_compile_definitions(playden_library_service PUBLIC PLAYDEN_XBOX_NATIVE=1)
    target_link_libraries(playden_library_service PUBLIC "${PLAYDEN_XBOX_NATIVE_LIBRARY}" unwind SceRandom)
endif()
